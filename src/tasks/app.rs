use core::{cell::RefCell, mem};

use defmt::{info, unwrap, warn, Format};
use embassy_executor::Spawner;
use embassy_nrf::{
    self,
    gpio::{AnyPin, Input, Level, Output, OutputDrive, Pin, Pull},
    gpiote::{self, InputChannel},
    interrupt::{self, SAADC, SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0},
    peripherals::{self, GPIOTE_CH0, P0_06, P0_14, P0_15, P0_19, P0_20, PPI_CH0, TWISPI0},
    ppi::{self, Ppi},
    pwm::SimplePwm,
    saadc::{self, Saadc},
    timerv2::{self, CounterType, TimerType},
    twim::{self, Twim},
    Peripheral, PeripheralRef, Peripherals,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, pubsub::Publisher};
use embassy_sync::{channel::Channel, pubsub::PubSubChannel};
use embassy_time::{Duration, Instant, Timer};

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;

#[path = "../drivers/environment.rs"]
mod environment;

#[path = "../drivers/soil_sensor.rs"]
mod soil_sensor;

#[path = "../drivers/rgbled.rs"]
mod rgbled;

mod sensors;

use futures::{
    future::{join, select},
    pin_mut,
};
use ltr303_async::{self, LTR303Result};
use shared_bus::{BusManager, NullMutex};
use shtc3_async::{self, SHTC3Result};

use self::soil_sensor::ProbeData;

// Sensor data transmission channel. Queue of 4. 1 publisher, 3 subscribers
pub static SENSOR_DATA_BUS: PubSubChannel<ThreadModeRawMutex, DataPacket, 4, 3, 1> =
    PubSubChannel::new();

#[cfg(debug_assertions)]
static MEAS_INTERVAL: Duration = Duration::from_secs(3);
#[cfg(not(debug_assertions))]
static MEAS_INTERVAL: Duration = Duration::from_secs(30);

// Data we get from main PCB:
//  2 bytes for battery voltage  => u16; unit: mV
//  2 bytes for air temperature  => u16; unit: 0.1 Kelvin
//  2 bytes for air humidity     => u16; unit: 0.01%
//  2 bytes for illuminance      => u16; unit: Lux
// Data we get from (optional) soil probe:
//  2 bytes for soil temperature => u16; unit: 0.1 Kelvin
//  4 bytes for soil moisture    => u32; unit: Hertz
//
// TODO:
//  1) We can encode soil moisture in percentages if we can find a way to directly map
//     frequency to %.
//  2) We can further "compress" the bytes. For example, temperature in Kelvin can be
//     expressed with 9 bits. 0-512
#[derive(Format, Clone)]
pub struct SensorData {
    pub battery_voltage: u32,
    pub sht_data: shtc3_async::SHTC3Result,
    pub ltr_data: ltr303_async::LTR303Result,
    pub soil_temperature: i32,
    pub soil_moisture: u32,
}

#[derive(Format, Clone, Default)]
pub struct EnvironmentData {
    air_temperature: u16, // unit: 0.1K
    air_humidity: u16,    // unit: 0.1%
    illuminance: u16,     // unit: Lux
}

impl EnvironmentData {
    fn new(sht_data: SHTC3Result, ltr_data: LTR303Result) -> Self {
        EnvironmentData {
            air_temperature: ((sht_data.temperature.as_millidegrees_celsius() + 273150) / 100)
                as u16,
            air_humidity: (sht_data.humidity.as_millipercent() / 100) as u16,
            illuminance: ltr_data.lux,
        }
    }
}

// 14 bytes total.
#[derive(Format, Clone)]
pub struct DataPacket {
    pub battery_voltage: u16, // unit: mV
    pub env_data: EnvironmentData,
    pub probe_data: ProbeData,
}

impl DataPacket {
    pub fn to_bytes_array(&self) -> [u8; 14] {
        let mut arr = [0u8; 14];
        // Encode battery voltage
        arr[0] = self.battery_voltage.to_be_bytes()[0];
        arr[1] = self.battery_voltage.to_be_bytes()[1];
        // Encode air temperature
        arr[2] = self.env_data.air_temperature.to_be_bytes()[0];
        arr[3] = self.env_data.air_temperature.to_be_bytes()[1];
        // Encode air humidity
        arr[4] = self.env_data.air_humidity.to_be_bytes()[0];
        arr[5] = self.env_data.air_humidity.to_be_bytes()[1];
        // Encode solar illuminance
        arr[6] = self.env_data.illuminance.to_be_bytes()[0];
        arr[7] = self.env_data.illuminance.to_be_bytes()[1];
        // Probe data
        // Encode soil temperature
        arr[8] = self.probe_data.soil_temperature.to_be_bytes()[0];
        arr[9] = self.probe_data.soil_temperature.to_be_bytes()[1];
        // Encode soil moisture
        arr[10] = self.probe_data.soil_moisture.to_be_bytes()[0];
        arr[11] = self.probe_data.soil_moisture.to_be_bytes()[1];
        arr[12] = self.probe_data.soil_moisture.to_be_bytes()[2];
        arr[13] = self.probe_data.soil_moisture.to_be_bytes()[3];

        arr
    }
}

impl Default for SensorData {
    fn default() -> Self {
        Self {
            battery_voltage: Default::default(),
            sht_data: Default::default(),
            ltr_data: Default::default(),
            soil_temperature: Default::default(),
            soil_moisture: Default::default(),
        }
    }
}

pub struct Hardware<'a, P0: Pin, P1: Pin, P2: Pin> {
    // One enable pin for external sensors (frequency + tmp112)
    enable_pin: Output<'a, P0>,
    // One I2C bus for SHTC3 and LTR303-ALS, as well as TMP112.
    i2c_bus: BusManager<NullMutex<Twim<'a, TWISPI0>>>,
    // Two v2 timers for the frequency measurement as well as one PPI channel.
    freq_cnter: timerv2::Timer<CounterType>,
    freq_timer: timerv2::Timer<TimerType>,
    probe_detect: Input<'a, P1>,
    adc: Saadc<'a, 1>,
    // Private variables. Why? Because they get dropped if I don't store them here.
    _ppi_ch: Ppi<'a, PPI_CH0, 1, 1>,
    _freq_in: InputChannel<'a, GPIOTE_CH0, P2>,
}
impl<'a, P0, P1, P2> Hardware<'a, P0, P1, P2>
where
    P0: Pin,
    P1: Pin,
    P2: Pin,
{
    fn new(
        pin_sda: &'a mut impl Pin,
        pin_scl: &'a mut impl Pin,
        pin_probe_en: &'a mut P0,
        pin_probe_detect: &'a mut P1,
        pin_adc: &'a mut impl Peripheral<P = impl saadc::Input>,
        pin_freq_in: &'a mut P2,
        saadc: &'a mut impl Peripheral<P = peripherals::SAADC>,
        twim: &'a mut impl Peripheral<P = peripherals::TWISPI0>,
        gpiote_ch: &'a mut GPIOTE_CH0,
        ppi_ch: &'a mut PPI_CH0,
        adc_irq: &'a mut impl Peripheral<P = interrupt::SAADC>,
        i2c_irq: &'a mut impl Peripheral<P = interrupt::SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0>,
    ) -> Self {
        // Soil enable pin used by soil probe sensor.
        let mut probe_en = Output::new(pin_probe_en, Level::Low, OutputDrive::Standard);
        probe_en.set_low();

        // ADC initialization
        let mut config = saadc::Config::default();
        config.oversample = saadc::Oversample::OVER64X;
        let channel_cfg = saadc::ChannelConfig::single_ended(pin_adc);
        let saadc = saadc::Saadc::new(saadc, adc_irq, config, [channel_cfg]);

        // I2C initialization
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K400; // 400k seems to be best for low power consumption.

        let i2c_bus = Twim::new(twim, i2c_irq, pin_sda, pin_scl, i2c_config);
        // Create a bus manager to be able to share i2c buses easily.
        let i2c_bus = shared_bus::BusManagerSimple::new(i2c_bus);

        // Counter + Timer initialization
        let freq_cnter = timerv2::Timer::new(timerv2::TimerInstance::TIMER1)
            .into_counter()
            .with_bitmode(timerv2::Bitmode::B32);

        let freq_timer = timerv2::Timer::new(timerv2::TimerInstance::TIMER2)
            .into_timer()
            .with_bitmode(timerv2::Bitmode::B32)
            .with_frequency(timerv2::Frequency::F1MHz);

        let freq_in = InputChannel::new(
            gpiote_ch,
            Input::new(pin_freq_in, embassy_nrf::gpio::Pull::Up),
            embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
        );

        let mut ppi_ch = Ppi::new_one_to_one(ppi_ch, freq_in.event_in(), freq_cnter.task_count());
        ppi_ch.enable();

        let probe_detect = Input::new(pin_probe_detect, Pull::Up);

        Self {
            enable_pin: probe_en,
            i2c_bus: i2c_bus,
            freq_cnter: freq_cnter,
            freq_timer: freq_timer,
            probe_detect: probe_detect,
            adc: saadc,
            _ppi_ch: ppi_ch,
            _freq_in: freq_in,
        }
    }
}

struct LowPowerPeripherals {
    pin_sda: AnyPin,
    pin_scl: AnyPin,
    pin_probe_en: AnyPin,
    pin_probe_detect: AnyPin,
    pin_adc: saadc::AnyInput,
    pin_freq_in: AnyPin,
    saadc: peripherals::SAADC,
    twim: peripherals::TWISPI0,
    gpiote_ch: GPIOTE_CH0,
    ppi_ch: PPI_CH0,
}

struct RgbPins {
    pin_red: AnyPin,
    pin_green: AnyPin,
    pin_blue: AnyPin,
}
struct HighPowerPeripherals {
    pin_chg_detect: AnyPin,
    pin_plug_detect: AnyPin,
    pins_rgb: RgbPins,
    pwm_rgb: peripherals::PWM0,
}

#[derive(Default, Clone, Format)]
pub struct PlantBuddyStatus {
    plugged_in: Option<bool>,
    charging: Option<bool>,
}

// async fn run_high_power(&mut pwm, &mut pin_red, &mut pin_green, &mut pin_blue, &mut charging_input) {
async fn monitor_charging(
    charging_detect: &mut Input<'_, impl Pin>,
    pwm: &mut SimplePwm<'_, peripherals::PWM0>,
) {
    info!("This task runs only when plugged in!");
    loop {
        if charging_detect.is_high() {
            pwm.set_duty(0, 0);
            pwm.set_duty(1, 255);
            pwm.set_duty(2, 0);
            charging_detect.wait_for_low().await;
        } else {
            // Set RGB to green. I could send a color via a channel.
            pwm.set_duty(0, 0);
            pwm.set_duty(1, 0);
            pwm.set_duty(2, 255);
            charging_detect.wait_for_high().await
        }
    }
}

async fn run_high_power(mut peripherals: HighPowerPeripherals) {
    let mut plugged_detect = Input::new(peripherals.pin_plug_detect, Pull::Up);
    let mut charging_detect = Input::new(peripherals.pin_chg_detect, Pull::Up);
    loop {
        // Wait for Plantbuddy to be plugged in.
        plugged_detect.wait_for_low().await;
        info!("Plantbuddy plugged in!");
        let mut rgbled = SimplePwm::new_3ch(
            &mut peripherals.pwm_rgb,
            &mut peripherals.pins_rgb.pin_red,
            &mut peripherals.pins_rgb.pin_green,
            &mut peripherals.pins_rgb.pin_blue,
        );
        rgbled.set_max_duty(255);
        // After plugged in, run the high-power coroutine
        let while_plugged_fut = monitor_charging(&mut charging_detect, &mut rgbled);
        pin_mut!(while_plugged_fut);
        let plugged_out_fut = plugged_detect.wait_for_high();
        pin_mut!(plugged_out_fut);

        // This will run the high power task only while PB is plugged in. If it gets plugged out,
        // the high power task gets dropped.
        select(while_plugged_fut, plugged_out_fut).await;
        info!("Dropped high power task!");
    }
}

async fn run_low_power(mut peripherals: LowPowerPeripherals) {
    let mut adc_irq = interrupt::take!(SAADC);
    let mut i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);

    loop {
        let hw = Hardware::new(
            &mut peripherals.pin_sda,
            &mut peripherals.pin_scl,
            &mut peripherals.pin_probe_en,
            &mut peripherals.pin_probe_detect,
            &mut peripherals.pin_adc,
            &mut peripherals.pin_freq_in,
            &mut peripherals.saadc,
            &mut peripherals.twim,
            &mut peripherals.gpiote_ch,
            &mut peripherals.ppi_ch,
            &mut adc_irq,
            &mut i2c_irq,
        );

        let start_time = Instant::now();
        let sensors = sensors::Sensors::new();
        let data_packet = sensors.sample(hw).await;
        info!("{:?}", data_packet);

        let publisher = SENSOR_DATA_BUS.publisher().unwrap();
        publisher.publish_immediate(data_packet);

        Timer::after(MEAS_INTERVAL - start_time.elapsed()).await;
    }
}

#[embassy_executor::task]
pub async fn application_task(mut p: Peripherals) {
    let lp_peripherals = LowPowerPeripherals {
        pin_sda: p.P0_14.degrade(),
        pin_scl: p.P0_15.degrade(),
        pin_probe_en: p.P0_06.degrade(),
        pin_probe_detect: p.P0_20.degrade(),
        pin_adc: p.P0_03.into(),
        pin_freq_in: p.P0_19.degrade(),
        saadc: p.SAADC,
        twim: p.TWISPI0,
        gpiote_ch: p.GPIOTE_CH0,
        ppi_ch: p.PPI_CH0,
    };
    let low_power_fut = run_low_power(lp_peripherals);
    pin_mut!(low_power_fut);

    let hp_peripherals = HighPowerPeripherals {
        pin_chg_detect: p.P0_29.degrade(),
        pin_plug_detect: p.P0_31.degrade(),
        pins_rgb: RgbPins {
            pin_red: p.P0_22.degrade(),
            pin_green: p.P0_23.degrade(),
            pin_blue: p.P0_24.degrade(),
        },
        pwm_rgb: p.PWM0,
    };
    let high_power_fut = run_high_power(hp_peripherals);
    pin_mut!(high_power_fut);

    join(low_power_fut, high_power_fut).await;
}

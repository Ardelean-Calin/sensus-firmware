use core::mem;

use defmt::{info, unwrap, warn, Format};
use embassy_nrf::{
    self,
    gpio::{Input, Level, Output, OutputDrive, Pin, Pull},
    gpiote::{self, InputChannel},
    interrupt::{self, SAADC, SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0},
    peripherals::{self, GPIOTE_CH0, P0_06, P0_19, P0_20, PPI_CH0, TWISPI0},
    ppi::{self, Ppi},
    saadc::{self, Saadc},
    timerv2::{self, CounterType, TimerType},
    twim::{self, Twim},
    Peripheral, Peripherals,
};
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, pubsub::Publisher};
use embassy_time::{Duration, Instant, Timer};

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;

#[path = "../drivers/environment.rs"]
mod environment;

#[path = "../drivers/soil_sensor.rs"]
mod soil_sensor;

#[path = "../drivers/rgbled.rs"]
mod rgbled;

// mod sensors;

use futures::{
    future::{join, join3, select},
    pin_mut,
};
use ltr303_async::{self, LTR303Result};
use shared_bus::{BusManager, NullMutex};
use shtc3_async::{self, SHTC3Result};

use crate::app::{
    battery_sensor::BatterySensor, environment::EnvironmentSensors, soil_sensor::SoilSensor,
};

use self::soil_sensor::ProbeData;

// Sensor data transmission channel. Queue of 4. 1 publisher, 3 subscribers
pub static SENSOR_DATA_BUS: PubSubChannel<ThreadModeRawMutex, DataPacket, 4, 3, 1> =
    PubSubChannel::new();

pub static GPIO_MONITOR_BUS: PubSubChannel<ThreadModeRawMutex, PlantBuddyStatus, 4, 3, 1> =
    PubSubChannel::new();

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

#[derive(Format, Clone)]

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

#[embassy_executor::task]
pub async fn application_task(p: Peripherals) {
    #[allow(unused_doc_comments)]
    /**
     * Cum ar fi sa separ task-urile in doua?
     * 1) Sensors task => Aduna date de la senzori, deinitializeaza perifericele cand termina cu ele
     * 2) Diagnostics task => Monitorizeaza diferite GPIO-uri, detecteaza daca incarcam, suntem plugged in, etc.
     *  2.a) BONUS! Daca suntem plugged in, pot rula inca un task/coroutine, si anume SerialCommTask
     *       Pot inclusiv face ceva de genul:
     *          select(plugged_in.is_low().await, serialCommTask().await)
     *       Astfel daca deconectez USB-C, ul, se distruge automat si serialCommTask
     *  
     *  NOTE: Nu ma pot baza pe intreruperi de GPIO, vad ca consuma prea mult curent. Va trebui sa am un task ciclic
     *        ex. 100ms, care verifica nivelul GPIO-urilor, il stocheaza, si apoi merge inapoi la somn.
     */
    // let rgbled = RGBLED::new_rgb(&mut p.PWM0, &mut p.P0_22, &mut p.P0_23, &mut p.P0_24);

    // Used interrupts:
    let adc_irq = interrupt::take!(SAADC);
    let i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    #[allow(unused_doc_comments)]
    /**
     * Task-uri:
     *  1) Adunare date -> ruleaza la fiecare 30-60s - CICLIC!
     *  2) Trimitere date prin BLE -> automat atunci cand date noi sunt prezente
     *  3) Receptionare date prin BLE -> nu e implementat inca
     *  4) Monitorizare GPIO-uri -> ruleaza in paralel cu toate.
     *  5) Trimitere date via Serial -> ruleaza doar daca suntem plugged in.
     */
    let gpio_monitor_fut = gpio_monitor_future(p.P0_31, p.P0_29);
    pin_mut!(gpio_monitor_fut);
    let sensor_data_fut = sensor_data_future::<TWISPI0, GPIOTE_CH0, PPI_CH0>(
        p.P0_14,
        p.P0_15,
        p.P0_06,
        p.P0_20,
        p.P0_03,
        p.P0_19,
        p.SAADC,
        p.TWISPI0,
        p.GPIOTE_CH0,
        p.PPI_CH0,
        adc_irq,
        i2c_irq,
    );
    pin_mut!(sensor_data_fut);

    join(sensor_data_fut, gpio_monitor_fut).await;
}

async fn sensor_data_future<
    I2C: twim::Instance,
    GPIOTE: gpiote::Channel,
    PPI: ppi::ConfigurableChannel,
>(
    mut pin_sda: impl Pin,
    mut pin_scl: impl Pin,
    mut pin_probe_en: impl Pin,
    mut pin_probe_detect: impl Pin,
    mut pin_adc: impl Peripheral<P = impl saadc::Input>,
    mut pin_freq_in: impl Pin,
    mut saadc: impl Peripheral<P = peripherals::SAADC>,
    mut twim: impl Peripheral<P = peripherals::TWISPI0>,
    mut gpiote_ch: impl Peripheral<P = GPIOTE>,
    mut ppi_ch: impl Peripheral<P = PPI>,
    mut adc_irq: impl Peripheral<P = interrupt::SAADC>,
    mut i2c_irq: impl Peripheral<P = interrupt::SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0>,
) {
    loop {
        // Soil enable pin used by soil probe sensor.
        let mut probe_en = Output::new(&mut pin_probe_en, Level::Low, OutputDrive::Standard);
        probe_en.set_low();

        // ADC initialization
        let mut config = saadc::Config::default();
        config.oversample = saadc::Oversample::OVER64X;
        let channel_cfg = saadc::ChannelConfig::single_ended(&mut pin_adc);
        let saadc = saadc::Saadc::new(&mut saadc, &mut adc_irq, config, [channel_cfg]);

        // I2C initialization
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K400; // 400k seems to be best for low power consumption.

        let i2c_bus = Twim::new(
            &mut twim,
            &mut i2c_irq,
            &mut pin_sda,
            &mut pin_scl,
            i2c_config,
        );
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
            &mut gpiote_ch,
            Input::new(&mut pin_freq_in, embassy_nrf::gpio::Pull::Up),
            embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
        );

        let mut ppi_ch =
            Ppi::new_one_to_one(&mut ppi_ch, freq_in.event_in(), freq_cnter.task_count());
        ppi_ch.enable();

        let probe_detect = Input::new(&mut pin_probe_detect, Pull::Up);

        // Environement data: air temperature & humidity, ambient light.
        let mut env_sensors = EnvironmentSensors::new(i2c_bus.acquire_i2c(), i2c_bus.acquire_i2c());
        // Probe data: soil moisture & temperature.
        let mut probe_sensor = SoilSensor::new(
            freq_timer,
            freq_cnter,
            i2c_bus.acquire_i2c(),
            probe_en,
            probe_detect,
        );
        // Battery voltage sensor. TODO could also be battery status
        let mut batt_sensor = BatterySensor::new(saadc);

        // Sample everything at the same time to save processing time.
        let (environment_data, probe_data, batt_mv) = join3(
            env_sensors.sample(),
            probe_sensor.sample(),
            batt_sensor.sample_mv(),
        )
        .await;

        let data_packet = DataPacket {
            battery_voltage: batt_mv,
            env_data: environment_data,
            probe_data: probe_data.unwrap_or_default(),
        };

        info!("{:?}", data_packet);
        let data_publisher = SENSOR_DATA_BUS.publisher().unwrap();
        data_publisher.publish_immediate(data_packet);

        Timer::after(Duration::from_millis(1000)).await;
    }
}

#[derive(Default, Clone, Format)]
pub struct PlantBuddyStatus {
    plugged_in: Option<bool>,
    charging: Option<bool>,
}

async fn gpio_monitor_future(mut plugged_in_pin: impl Pin, mut charging_pin: impl Pin) {
    loop {
        let plugged_in_input = Input::new(&mut plugged_in_pin, Pull::Up);
        let charging_input = Input::new(&mut charging_pin, Pull::Up);
        let charging: bool = charging_input.get_level().into();
        let plugged_in: bool = plugged_in_input.get_level().into();
        let pin_status = PlantBuddyStatus {
            charging: Some(!charging),
            plugged_in: Some(!plugged_in),
        };
        info!("{:?}", pin_status);

        // Publish the new pin data. To be used by other tasks
        let publisher = GPIO_MONITOR_BUS.publisher().unwrap();
        publisher.publish_immediate(pin_status);

        // Drop the peripherals to save power.
        mem::drop(plugged_in_input);
        mem::drop(charging_input);

        // I can not use the wait_for_any_edge future since it seems to draw too much power. So I sleep instead.
        Timer::after(Duration::from_millis(100)).await
    }
}

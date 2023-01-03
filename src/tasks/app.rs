use defmt::{info, warn, Format};
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::saadc::VddInput;
use embassy_nrf::{
    self,
    gpio::{Input, Level, Output, OutputDrive, Pin, Pull},
    gpiote::InputChannel,
    interrupt::{self},
    peripherals::{self, GPIOTE_CH0, PPI_CH0, TWISPI0},
    ppi::Ppi,
    pwm::SimplePwm,
    saadc::{self, Saadc},
    timerv2::{self, CounterType, TimerType},
    twim::{self, Twim},
    Peripheral,
};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::pubsub::PubSubChannel;
use embassy_time::{Duration, Ticker};

use futures::{
    future::{join, select},
    pin_mut, StreamExt,
};
use shared_bus::{BusManager, NullMutex};

use ltr303_async::{self};
use shtc3_async::{self};

use crate::ApplicationPeripherals;
use crate::HighPowerPeripherals;

use super::sensors;

// Sensor data transmission channel. Queue of 4. 1 publisher, 3 subscribers
pub static SENSOR_DATA_BUS: PubSubChannel<ThreadModeRawMutex, sensors::types::DataPacket, 4, 3, 1> =
    PubSubChannel::new();

#[cfg(debug_assertions)]
static MEAS_INTERVAL: Duration = Duration::from_millis(3000); // TODO: have a default val but change via BLE
#[cfg(not(debug_assertions))]
static MEAS_INTERVAL: Duration = Duration::from_secs(30);

#[derive(PartialEq)]
enum PowerMode {
    LowPower,
    HighPower,
}

static CHANNEL: Channel<ThreadModeRawMutex, PowerMode, 1> = Channel::new();

struct PowerModeDetect {}

impl PowerModeDetect {
    fn new() -> Self {
        PowerModeDetect {}
    }

    async fn wait_for(&self, power_mode: PowerMode) {
        loop {
            let mode = CHANNEL.recv().await;
            if mode == power_mode {
                break;
            }
        }
    }
    async fn wait_for_high_power() {
        PowerModeDetect::new().wait_for(PowerMode::HighPower).await;
    }
    async fn wait_for_low_power() {
        PowerModeDetect::new().wait_for(PowerMode::LowPower).await;
    }
}

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
#[derive(Format, Clone, Default)]
pub struct SensorData {
    pub battery_voltage: u32,
    pub sht_data: shtc3_async::SHTC3Result,
    pub ltr_data: ltr303_async::LTR303Result,
    pub soil_temperature: i32,
    pub soil_moisture: u32,
}

#[derive(Default, Clone, Format)]
pub struct PlantBuddyStatus {
    plugged_in: Option<bool>,
    charging: Option<bool>,
}

pub struct Hardware<'a, P0: Pin, P1: Pin, P2: Pin> {
    // One enable pin for external sensors (frequency + tmp112)
    pub enable_pin: Output<'a, P0>,
    // One I2C bus for SHTC3 and LTR303-ALS, as well as TMP112.
    pub i2c_bus: BusManager<NullMutex<Twim<'a, TWISPI0>>>,
    // Two v2 timers for the frequency measurement as well as one PPI channel.
    pub freq_cnter: timerv2::Timer<CounterType>,
    pub freq_timer: timerv2::Timer<TimerType>,
    pub probe_detect: Input<'a, P1>,
    pub adc: Saadc<'a, 1>,
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
        let channel_cfg = saadc::ChannelConfig::single_ended(VddInput);
        let adc = saadc::Saadc::new(saadc, adc_irq, config, [channel_cfg]);

        // I2C initialization
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K100; // 400k seems to be best for low power consumption.
        i2c_config.scl_pullup = true;
        i2c_config.sda_pullup = true;

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

        let _freq_in = InputChannel::new(
            gpiote_ch,
            Input::new(pin_freq_in, embassy_nrf::gpio::Pull::Up),
            embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
        );

        let mut _ppi_ch = Ppi::new_one_to_one(ppi_ch, _freq_in.event_in(), freq_cnter.task_count());
        _ppi_ch.enable();

        let probe_detect = Input::new(pin_probe_detect, Pull::Up);

        Self {
            enable_pin: probe_en,
            i2c_bus,
            freq_cnter,
            freq_timer,
            probe_detect,
            adc,
            _ppi_ch,
            _freq_in,
        }
    }
}

// async fn run_high_power(&mut pwm, &mut pin_red, &mut pin_green, &mut pin_blue, &mut charging_input) {
async fn monitor_charging(
    charging_detect: &mut Input<'_, impl Pin>,
    pwm: &mut SimplePwm<'_, peripherals::PWM0>,
) {
    // info!("This task runs only when plugged in!");
    // If charging, show a green LED.
    loop {
        if charging_detect.is_high() {
            pwm.set_duty(0, 0);
            pwm.set_duty(1, 0);
            pwm.set_duty(2, 0);
            charging_detect.wait_for_low().await;
        } else {
            pwm.set_duty(0, 0);
            pwm.set_duty(1, 255);
            pwm.set_duty(2, 0);
            charging_detect.wait_for_high().await
        }
    }
}

async fn run_high_power(mut peripherals: HighPowerPeripherals) {
    let mut charging_detect = Input::new(peripherals.pin_chg_detect, Pull::Up);
    loop {
        // Wait for Plantbuddy to be plugged in. High power peripherals are uninitialized until I plug in
        PowerModeDetect::wait_for_high_power().await;
        // Theoretically, the peripherals initialized in the previous loop will be dropped at the end of the loop.
        info!("Plantbuddy plugged in!");
        let mut rgbled = SimplePwm::new_3ch(
            &mut peripherals.pwm_rgb,
            &mut peripherals.pins_rgb.pin_red,
            &mut peripherals.pins_rgb.pin_green,
            &mut peripherals.pins_rgb.pin_blue,
        );
        rgbled.set_max_duty(255);
        // After plugged in, run the high-power coroutine
        let charging_monitor_fut = monitor_charging(&mut charging_detect, &mut rgbled);
        pin_mut!(charging_monitor_fut);
        let usb_comm_fut = super::serial::serial_task(
            &mut peripherals.uart,
            &mut peripherals.pin_uart_tx,
            &mut peripherals.pin_uart_rx,
        );
        let plugged_out_fut = PowerModeDetect::wait_for_low_power();
        pin_mut!(plugged_out_fut);

        // Create a high power future. This one runs only when plugged in and only UNTIL plugged in.
        let high_power_fut = join(charging_monitor_fut, usb_comm_fut);
        pin_mut!(high_power_fut);

        // This will run the high power task only while PB is plugged in. If it gets plugged out,
        // the high power task gets dropped.
        select(high_power_fut, plugged_out_fut).await;
    }
}

/// This task runs only when in low-power mode.
#[embassy_executor::task]
pub async fn low_power_task(app_peripherals: ApplicationPeripherals) {
    // run_low_power(app_peripherals).await;
}

/// This task runs only when in high-power mode.
#[embassy_executor::task]
pub async fn high_power_task(hp_peripherals: HighPowerPeripherals) {
    run_high_power(hp_peripherals).await;
}

/// This task runs always. Independent of power mode.
#[embassy_executor::task]
pub async fn application_task(mut peripherals: ApplicationPeripherals) {
    let mut adc_irq = interrupt::take!(SAADC);
    adc_irq.set_priority(interrupt::Priority::P7);
    let mut i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    i2c_irq.set_priority(interrupt::Priority::P7);

    let mut ticker = Ticker::every(MEAS_INTERVAL);
    loop {
        let hw = Hardware::new(
            &mut peripherals.pin_sda,
            &mut peripherals.pin_scl,
            &mut peripherals.pin_probe_en,
            &mut peripherals.pin_probe_detect,
            &mut peripherals.pin_freq_in,
            &mut peripherals.saadc,
            &mut peripherals.twim,
            &mut peripherals.gpiote_ch,
            &mut peripherals.ppi_ch,
            &mut adc_irq,
            &mut i2c_irq,
        );

        let sensors = super::sensors::Sensors::new();
        if let Ok(data_packet) = sensors.sample(hw).await {
            info!("Got new data: {:?}", data_packet);
            let publisher = SENSOR_DATA_BUS.publisher().unwrap();
            publisher.publish_immediate(data_packet);
            ticker.next().await;
        } else {
            // Try three times... Afterwards report error and sleep. TODO.
            warn!("Error sampling sensor.");
        };
    }
}

/// Monitors the plugged-in state and publishes a global state that can be used by other tasks
/// to only run when plugged in or plugged out.
#[embassy_executor::task]
pub async fn power_state_task(plugged_in_pin: AnyPin) {
    let mut plugged_detect = Input::new(plugged_in_pin, Pull::Up);
    // By default, we are in low-power state.
    loop {
        plugged_detect.wait_for_low().await;
        CHANNEL.send(PowerMode::HighPower).await;
        plugged_detect.wait_for_high().await;
        CHANNEL.send(PowerMode::LowPower).await;
    }
}

pub(crate) mod soil_sensor;

use defmt::{error, info};
use embassy_nrf::{
    gpio::{AnyPin, Input, Level, Output, Pull},
    gpiote::InputChannel,
    interrupt::{self, InterruptExt},
    peripherals::{self, GPIOTE_CH0, PPI_CH0},
    ppi::Ppi,
    timerv2, twim,
};
use embassy_time::{with_timeout, Duration};
use futures::pin_mut;

use crate::{types::I2cPins, AppState, APP_STATE_CHANNEL};

#[embassy_executor::task]
pub async fn soil_task(
    mut probe_detect: AnyPin,
    mut probe_enable: AnyPin,
    mut i2c_pins: I2cPins,
    mut probe_twim: peripherals::TWISPI1,
    mut probe_freq_in: AnyPin,
    mut probe_gpiote_ch: GPIOTE_CH0,
    mut probe_ppi_ch: PPI_CH0,
) {
    let mut app_state_subscriber = APP_STATE_CHANNEL.subscriber().unwrap();
    let mut i2c_irq = interrupt::take!(SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1);
    i2c_irq.set_priority(interrupt::Priority::P7);

    loop {
        if let AppState::SampleProbe = app_state_subscriber.next_message_pure().await {
            // Probe enable pin
            let enable = Output::new(
                &mut probe_enable,
                Level::Low,
                embassy_nrf::gpio::OutputDrive::Standard,
            );

            // I2C initialization
            let mut i2c_config = twim::Config::default();
            i2c_config.frequency = twim::Frequency::K100; // 100k seems to be best for low power consumption.
            i2c_config.scl_pullup = true; // 5k1 pull-ups are provided on PCB
            i2c_config.sda_pullup = true;

            let i2c_bus = twim::Twim::new(
                &mut probe_twim,
                &mut i2c_irq,
                &mut i2c_pins.pin_sda,
                &mut i2c_pins.pin_scl,
                i2c_config,
            );

            // Counter + Timer initialization
            let freq_cnter = timerv2::Timer::new(timerv2::TimerInstance::TIMER1)
                .into_counter()
                .with_bitmode(timerv2::Bitmode::B32);

            let freq_timer = timerv2::Timer::new(timerv2::TimerInstance::TIMER2)
                .into_timer()
                .with_bitmode(timerv2::Bitmode::B32)
                .with_frequency(timerv2::Frequency::F1MHz);

            let freq_in = InputChannel::new(
                &mut probe_gpiote_ch,
                Input::new(&mut probe_freq_in, embassy_nrf::gpio::Pull::Up),
                embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
            );

            let mut ppi_ch = Ppi::new_one_to_one(
                &mut probe_ppi_ch,
                freq_in.event_in(),
                freq_cnter.task_count(),
            );
            ppi_ch.enable();

            let detect = Input::new(&mut probe_detect, Pull::Up);

            // Actual sensor operation
            let mut sensor =
                soil_sensor::SoilSensor::new(freq_timer, freq_cnter, i2c_bus, enable, detect);
            let sample_fut = sensor.sample();
            pin_mut!(sample_fut);

            if let Ok(Ok(probe_data)) = with_timeout(Duration::from_millis(100), sample_fut).await {
                info!("Probe: {:?}", probe_data);
            } else {
                // Increase an error counter. If 3 consecutive errors, disable probe until restart.
                error!("Error sampling probe! Increasing error counter.");
            }
        }
    }
}

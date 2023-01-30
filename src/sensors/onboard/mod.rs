pub mod environment;

use crate::{types::I2cPins, AppState, APP_STATE_CHANNEL};
use defmt::{error, info, unwrap};
use embassy_nrf::{
    gpio::AnyPin,
    interrupt::{self, InterruptExt},
    peripherals, saadc, twim,
};

mod battery_sensor;
use battery_sensor::BatterySensor;
use embassy_time::{with_timeout, Duration};
use futures::{future::join, pin_mut};

#[embassy_executor::task]
pub async fn onboard_task(
    mut i2c_pins: I2cPins,
    mut pin_interrupt: AnyPin,
    mut onboard_twim: peripherals::TWISPI0,
    mut saadc: peripherals::SAADC,
) {
    let mut app_state_subscriber = unwrap!(APP_STATE_CHANNEL.subscriber());
    let mut adc_irq = interrupt::take!(SAADC);
    adc_irq.set_priority(interrupt::Priority::P7);
    let mut i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    i2c_irq.set_priority(interrupt::Priority::P7);

    loop {
        if let AppState::SampleOnboard = app_state_subscriber.next_message_pure().await {
            // ADC initialization
            let mut config = saadc::Config::default();
            config.oversample = saadc::Oversample::OVER64X;

            let channel_cfg = saadc::ChannelConfig::single_ended(saadc::VddInput);
            let adc = saadc::Saadc::new(&mut saadc, &mut adc_irq, config, [channel_cfg]);
            let mut batt_sensor = BatterySensor::new(adc);

            // I2C initialization
            let mut i2c_config = twim::Config::default();
            i2c_config.frequency = twim::Frequency::K100; // 100k seems to be best for low power consumption.
            i2c_config.scl_pullup = true;
            i2c_config.sda_pullup = true;

            let i2c_bus = twim::Twim::new(
                &mut onboard_twim,
                &mut i2c_irq,
                &mut i2c_pins.pin_sda,
                &mut i2c_pins.pin_scl,
                i2c_config,
            );
            // Create a bus manager to be able to share i2c buses easily.
            let i2c_bus = shared_bus::BusManagerSimple::new(i2c_bus);
            let mut sht = shtc3_async::Shtc3::new(i2c_bus.acquire_i2c());

            let mut delay_provider = embassy_time::Delay;
            let sht_fut = sht.sample(&mut delay_provider);
            pin_mut!(sht_fut);

            let batt_fut = batt_sensor.sample_mv();
            pin_mut!(batt_fut);

            let sensor_fut = join(sht_fut, batt_fut);
            if let Ok((Ok(sht_res), Ok(batt_res))) =
                with_timeout(Duration::from_millis(200), sensor_fut).await
            {
                info!("Onboard: {:?}\t{:?}", sht_res, batt_res);
            } else {
                error!("Error sampling onboard sensors.");
            }

            // let opt_fut = opt3001.sample();
            // let mut opt3001 =
            //     Opt300x::new_opt3001(i2c_bus.acquire_i2c(), opt300x::SlaveAddr::Default);
            // let result = unwrap!(sht.sample(&mut embassy_time::Delay).await);
            // info!("SHTc3 result: {:?}", result);
            // let x = opt3001.read_lux();
        }
    }
}

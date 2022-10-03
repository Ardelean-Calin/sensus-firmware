#[path = "../../common.rs"]
mod common;

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;

use embassy_nrf::peripherals::TWISPI0;
// My own drivers.
use ltr303_async::Ltr303;
use shtc3_async::Shtc3;

use futures::future::join;

use defmt::{info, *};
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::interrupt;
use embassy_nrf::twim::{self, Twim};
use embassy_time::{Delay, Duration, Timer};

use battery_sensor::BatterySensor;
use shared_bus;

/// Periodicity of the i2c sensor measurement.
const SENSOR_PERIOD_MS: u16 = 5000;

/// When called, samples all the i2c sensors and then drops
/// the i2c bus to save power.
async fn i2c_sensors_sample(
    i2c_bus: Twim<'_, TWISPI0>,
) -> (shtc3_async::Measurement, ltr303_async::Measurement) {
    debug!("Sampling i2c sensors...");
    // Create the I2C bus which will be dropped after sampling is done!
    let bus = shared_bus::BusManagerSimple::new(i2c_bus);
    let proxy1 = bus.acquire_i2c();
    let proxy2 = bus.acquire_i2c();

    let mut sht_sensor = Shtc3::new(proxy1);
    let mut ltr_sensor = Ltr303::new(proxy2);
    let mut delay1 = Delay;
    let mut delay2 = Delay;

    let (result_sht, result_ltr) = join(
        sht_sensor.sample(&mut delay1),
        ltr_sensor.sample(&mut delay2),
    )
    .await;

    (result_sht.unwrap(), result_ltr.unwrap())
}

/// When called, samples the battery voltage and returns its value in millivolts.
async fn battery_sensor_sample<'a>(mut sensor: BatterySensor<'a>) -> u32 {
    sensor.init().await;
    let battery_voltage = sensor.sample_mv().await;
    battery_voltage
}

#[embassy_executor::task]
pub async fn sensors_task() {
    let mut config = embassy_nrf::config::Config::default();
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;

    // Peripherals config
    let mut p = embassy_nrf::init(config);
    let mut i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    let mut adc_irq = interrupt::take!(SAADC);
    // TODO: Final revision will not need this.
    let mut sen = Output::new(p.P0_06, Level::Low, OutputDrive::Standard);
    // end TODO

    loop {
        // TODO: Final revision will not need this.
        sen.set_high();
        Timer::after(Duration::from_millis(2)).await;
        // end TODO
        let battery_sensor = BatterySensor::new(&mut p.P0_29, &mut p.SAADC, &mut adc_irq);
        let mut config = twim::Config::default();
        config.frequency = twim::Frequency::K400; // 400k seems to be best for low power consumption.

        let i2c_bus = Twim::new(
            &mut p.TWISPI0,
            &mut i2c_irq,
            &mut p.P0_08,
            &mut p.P0_09,
            config,
        );

        // i2c_bus gets owned by i2c_sensors_sample, which drops its value at the end of the function call!
        // The power consumption is therefore minimal, and the peripheral is recreated each loop iteration.
        // We can specify a timeout using the timer below.
        // NOTE: join3 doesn't seem to work!
        let (voltage, (sht_data, ltr_data)) = join(
            battery_sensor_sample(battery_sensor),
            i2c_sensors_sample(i2c_bus),
        )
        .await;
        // TODO: Final revision will not need this.
        sen.set_low();
        // end TODO

        info!("Battery voltage: {}mV", voltage);
        info!("SHT measurement result: {}", sht_data);
        info!("LTR measurement result: {}", ltr_data);

        // I know I don't include the measurement time here, but I don't really need to be super precise...
        Timer::after(Duration::from_millis(SENSOR_PERIOD_MS as u64)).await;
    }
}

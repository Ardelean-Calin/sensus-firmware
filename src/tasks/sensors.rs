#[path = "../../common.rs"]
mod common;

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;

// My own drivers.
use ltr303_async;
use shtc3_async::{Measurement, PowerMode, Shtc3};

use embassy_nrf::interrupt::SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0;
use embedded_hal_async::i2c::I2c;
use futures::future::join;

use defmt::{info, *};
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pin, Pull};
use embassy_nrf::twim::{self, Twim};
use embassy_nrf::{interrupt, Peripherals};
use embassy_time::{Delay, Duration, Timer};

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::mutex::Mutex;

use battery_sensor::BatterySensor;

async fn shtc3_sample<'a, M: RawMutex, BUS: I2c>(dev: I2cDevice<'a, M, BUS>) -> Measurement {
    let mut sht = Shtc3::new(dev);
    let mut delay = Delay;

    unwrap!(sht.wakeup(&mut delay).await);
    let result = unwrap!(sht.measure(PowerMode::LowPower, &mut delay).await);
    info!(
        "Got the following measurement: RH: {}%\tT: {}C",
        result.humidity.as_percent(),
        result.temperature.as_degrees_celsius()
    );
    unwrap!(sht.sleep().await);
    result
}

async fn sensors_sample(
    p: &mut Peripherals,
    irq1: &mut SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0,
    irq2: &mut embassy_nrf::interrupt::SAADC,
) {
    let mut sen = Output::new(&mut p.P0_06, Level::Low, OutputDrive::Standard);
    sen.set_high();
    loop {
        {
            // Initialize and enable the power to the sensors. TODO! Remove from final version.
            // let mut sen = Output::new(&mut p.P0_06, Level::Low, OutputDrive::Standard);
            // sen.set_high();
            // core::mem::drop(sen);
            Timer::after(Duration::from_millis(2)).await;

            // Configure used interfaces
            // 1. ADC
            let mut battery_sensor = BatterySensor::new(&mut p.P0_29, &mut p.SAADC, irq2);
            battery_sensor.init().await;

            // 2. TWIM
            let mut config = twim::Config::default();
            config.frequency = twim::Frequency::K250;

            let i2c = Twim::new(
                &mut p.TWISPI0,
                &mut *irq1,
                &mut p.P0_08,
                &mut p.P0_09,
                config,
            );
            let i2c_bus = Mutex::<NoopRawMutex, _>::new(i2c);
            let i2c_dev1 = I2cDevice::new(&i2c_bus);
            let i2c_dev2 = I2cDevice::new(&i2c_bus);

            // let (shtc3_measurement, ltr303_measurement) = join(shtc3_sample(i2c_dev1), ltr303_sample(i2c_dev2));

            let (shtc3_meas, batt_meas) =
                join(shtc3_sample(i2c_dev1), battery_sensor.sample()).await;
        }
        Timer::after(Duration::from_millis(1000)).await;
    }
}

#[embassy_executor::task]
pub async fn sensors_task() {
    let mut config = embassy_nrf::config::Config::default();
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    // Peripherals config
    let mut p = embassy_nrf::init(config);
    let mut i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    let mut adc_irq = interrupt::take!(SAADC);

    sensors_sample(&mut p, &mut i2c_irq, &mut adc_irq).await;
}

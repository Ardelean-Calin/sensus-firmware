pub mod types;

use embassy_nrf::gpio::{AnyPin, Input};
use embassy_time::Delay;
use opt300x_async::{IntegrationTime, Opt300x, SlaveAddr};

use self::types::{EnvironmentError, EnvironmentSample};

pub async fn init(i2c_bus: super::types::BusManagerType<'_>) -> Result<(), EnvironmentError> {
    let mut shtc3 = shtc3_async::Shtc3::new(i2c_bus.acquire_i2c());
    // let mut opt3001 = Opt300x::new_opt3001(i2c_bus.acquire_i2c(), SlaveAddr::Default);

    // Put the sensors to sleep.
    shtc3.sleep().map_err(|_| EnvironmentError::SHTCommError)?;
    // opt3001.sleep()

    Ok(())
}

pub async fn sample_environment(
    i2c_bus: super::types::BusManagerType<'_>,
    mut wait_pin: Input<'_, AnyPin>,
) -> Result<EnvironmentSample, EnvironmentError> {
    let mut shtc3 = shtc3_async::Shtc3::new(i2c_bus.acquire_i2c());
    let mut opt3001 = Opt300x::new_opt3001(i2c_bus.acquire_i2c(), SlaveAddr::Default);
    opt3001
        .set_integration_time(IntegrationTime::Ms100)
        .map_err(|_| EnvironmentError::OPTCommError)?;
    opt3001
        .enable_end_of_conversion_mode()
        .map_err(|_| EnvironmentError::OPTCommError)?;

    let shtc3_result = shtc3
        .sample(&mut Delay)
        .await
        .map_err(|_| EnvironmentError::SHTCommError)?;
    let opt3001_result = opt3001
        .read_lux(&mut wait_pin)
        .await
        .map_err(|_| EnvironmentError::OPTCommError)?;
    let temperature = shtc3_result.temperature.as_degrees_celsius();
    let humidity = shtc3_result.humidity.as_percent();

    let _x = shtc3.sleep();

    Ok(EnvironmentSample {
        illuminance: opt3001_result.result,
        temperature,
        humidity,
    })
}

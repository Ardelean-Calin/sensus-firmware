use embassy_nrf::gpio::Pin;
use futures::{future::join3, pin_mut};

pub mod types;
use types::*;

use crate::sensors::{
    battery_sensor::BatterySensor, environment::EnvironmentSensors, soil_sensor::SoilSensor,
};

use super::app::Hardware;

pub enum SensorError {
    I2CError,
    ProbeError,
}

pub struct Sensors {}
impl Sensors {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn sample<'a, 'b, P0: Pin, P1: Pin, P2: Pin>(
        &'a self,
        hw: Hardware<'b, P0, P1, P2>,
    ) -> Result<DataPacket, SensorError> {
        // Environment data: air temperature & humidity, ambient light.
        let mut env_sensors =
            EnvironmentSensors::new(hw.i2c_bus.acquire_i2c(), hw.i2c_bus.acquire_i2c());
        // Probe data: soil moisture & temperature.
        let mut probe_sensor = SoilSensor::new(
            hw.freq_timer,
            hw.freq_cnter,
            hw.i2c_bus.acquire_i2c(),
            hw.enable_pin,
            hw.probe_detect,
        );
        // Battery voltage sensor. TODO could also be battery status
        let mut batt_sensor = BatterySensor::new(hw.adc);

        let env_fut = env_sensors.sample();
        let probe_fut = probe_sensor.sample();
        let batt_fut = batt_sensor.sample_mv();

        pin_mut!(env_fut);
        pin_mut!(probe_fut);
        pin_mut!(batt_fut);

        // Sample everything at the same time to save processing time.
        let (environment_result, probe_result, batt_mv) = join3(env_fut, probe_fut, batt_fut).await;
        let environment_data = environment_result.map_err(|_| SensorError::I2CError)?;
        // let probe_data = probe_result.map_err(|_| SensorError::ProbeError)?;

        // I could have some type of field representing invalid data. InvalidData<LastData>. This way, in case
        // of an error I keep the last received value (or 0 if no value) and just wrap it inside InvalidData
        // to mark it as being non-valid.
        Ok(DataPacket {
            battery_voltage: batt_mv,
            env_data: environment_data,
            probe_data: probe_result.unwrap_or_default(),
        })
        // At the end, all our sensors are dropped since we own Hardware. So all peripherals found there
        // get dropped. That includes i2c, gpio, etc.
    }
}

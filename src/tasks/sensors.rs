use embassy_nrf::gpio::Pin;
use embassy_time::Duration;
use futures::future::join3;

use super::{
    battery_sensor::BatterySensor, environment::EnvironmentSensors, soil_sensor::SoilSensor,
    DataPacket, Hardware,
};

// Constants TODO: configure different delay when release active.
pub const MEAS_INTERVAL: Duration = Duration::from_secs(30);

pub struct Sensors {}
impl Sensors {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn sample<'a, 'b, P0: Pin, P1: Pin, P2: Pin>(
        &'a self,
        hw: Hardware<'b, P0, P1, P2>,
    ) -> DataPacket {
        // Environement data: air temperature & humidity, ambient light.
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

        // Sample everything at the same time to save processing time.
        let (environment_data, probe_data, batt_mv) = join3(
            env_sensors.sample(),
            probe_sensor.sample(),
            batt_sensor.sample_mv(),
        )
        .await;

        // I could have some type of field representing invalid data. InvalidData<LastData>. This way, in case
        // of an error I keep the last received value (or 0 if no value) and just wrap it inside InvalidData
        // to mark it as being non-valid.
        DataPacket {
            battery_voltage: batt_mv,
            env_data: environment_data,
            probe_data: probe_data.unwrap_or_default(),
        }
        // At the end, all our sensors are dropped since we own Hardware. So all peripherals found there
        // get dropped. That includes i2c, gpio, etc.
    }
}

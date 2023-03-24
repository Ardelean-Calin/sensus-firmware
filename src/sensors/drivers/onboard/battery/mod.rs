pub mod types;

use embassy_nrf::saadc::Saadc;

use self::types::{BatteryLevel, BatterySensor};

impl<'a> BatterySensor<'a> {
    pub fn new(adc: Saadc<'a, 1>) -> Self {
        BatterySensor { saadc: adc }
    }

    async fn sample_mv(&mut self) -> u32 {
        let mut buf = [0i16; 1];
        // self.saadc.calibrate().await; //TODO: enabling this causes the measurements to appear double.
        self.saadc.sample(&mut buf).await;
        u32::from(buf[0].unsigned_abs()) * 100000 / 113778
    }

    pub async fn sample(&mut self) -> BatteryLevel {
        let voltage_mv = self.sample_mv().await;
        BatteryLevel {
            value: (voltage_mv as f32) / 1000f32,
        }
    }
}

pub async fn sample_battery_level(mut sensor: BatterySensor<'_>) -> BatteryLevel {
    sensor.sample().await
}

use defmt::{info, Format};
use embedded_hal::blocking::i2c::{Read, Write, WriteRead};

use embassy_time::Delay;
use futures::future::join;
use ltr303_async::{LTR303Result, Ltr303};
use serde::{Deserialize, Serialize};
use shtc3_async::{SHTC3Result, Shtc3};

#[derive(Format, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentData {
    air_temperature: i16, // unit: 0.01C
    air_humidity: u16,    // unit: 0.01%
    illuminance: u16,     // unit: Lux
}

impl EnvironmentData {
    fn new(sht_data: SHTC3Result, ltr_data: LTR303Result) -> Self {
        EnvironmentData {
            air_temperature: (((sht_data.temperature.as_millidegrees_celsius() - 1500) / 10i32)
                as i16),
            air_humidity: ((sht_data.humidity.as_millipercent() / 10u32) as u16),
            illuminance: ltr_data.lux,
        }
    }

    pub fn get_air_temp(&self) -> i16 {
        self.air_temperature
    }

    pub fn get_air_humidity(&self) -> u16 {
        self.air_humidity
    }

    pub fn get_illuminance(&self) -> u16 {
        self.illuminance
    }
}

pub struct EnvironmentSensors<T> {
    sht_sensor: Shtc3<T>,
    ltr_sensor: Ltr303<T>,
}

impl<T, E> EnvironmentSensors<T>
where
    T: Read<Error = E> + Write<Error = E> + WriteRead<Error = E>,
    E: core::fmt::Debug,
{
    pub fn new(i2c_sht: T, i2c_ltr: T) -> EnvironmentSensors<T> {
        let sht_sensor = Shtc3::new(i2c_sht);
        let ltr_sensor = Ltr303::new(i2c_ltr);
        EnvironmentSensors {
            sht_sensor,
            ltr_sensor,
        }
    }

    pub async fn sample(&mut self) -> EnvironmentData {
        let mut delay1 = Delay;
        let mut delay2 = Delay;

        let (result_sht, result_ltr) = join(
            self.sht_sensor.sample(&mut delay1),
            self.ltr_sensor.sample(&mut delay2),
        )
        .await;
        info!("Sampled env sensor!");
        EnvironmentData::new(
            result_sht.unwrap_or(Default::default()),
            result_ltr.unwrap_or(Default::default()),
        )
    }
}

use embedded_hal::blocking::i2c::{Read, Write, WriteRead};

use embassy_time::Delay;
use futures::future::join;
use ltr303_async::Ltr303;
use shtc3_async::Shtc3;

use super::EnvironmentData;

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

        let data = EnvironmentData::new(result_sht.unwrap(), result_ltr.unwrap());

        data
    }
}

use embedded_hal::blocking::i2c::{Read, Write, WriteRead};

use embassy_time::Delay;
use futures::future::join;
use ltr303_async::Ltr303;
use shtc3_async::Shtc3;

pub struct EnvironmentSensors<T> {
    i2c_sht: T,
    i2c_ltr: T,
}

impl<T, E> EnvironmentSensors<T>
where
    T: Read<Error = E> + Write<Error = E> + WriteRead<Error = E>,
    E: core::fmt::Debug,
{
    pub fn new(i2c_sht: T, i2c_ltr: T) -> EnvironmentSensors<T> {
        EnvironmentSensors { i2c_sht, i2c_ltr }
    }

    pub async fn sample(self) -> (shtc3_async::SHTC3Result, ltr303_async::LTR303Result) {
        let mut sht_sensor = Shtc3::new(self.i2c_sht);
        let mut ltr_sensor = Ltr303::new(self.i2c_ltr);
        let mut delay1 = Delay;
        let mut delay2 = Delay;

        let (result_sht, result_ltr) = join(
            sht_sensor.sample(&mut delay1),
            ltr_sensor.sample(&mut delay2),
        )
        .await;

        (result_sht.unwrap(), result_ltr.unwrap())
    }
}

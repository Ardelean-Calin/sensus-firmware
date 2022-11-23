use embassy_nrf::saadc::{self, Saadc};

pub struct BatterySensor<'a> {
    saadc: saadc::Saadc<'a, 1>,
}

impl<'a> BatterySensor<'a> {
    pub fn new(adc: Saadc<'a, 1>) -> Self {
        BatterySensor { saadc: adc }
    }

    pub async fn sample_mv(&mut self) -> u32 {
        let mut buf = [0i16; 1];
        self.saadc.calibrate().await;
        self.saadc.sample(&mut buf).await;
        let voltage: u32 = u32::from(buf[0].unsigned_abs()) * 200000 / 113778;
        voltage
    }
}

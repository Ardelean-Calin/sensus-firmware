use defmt::info;
use embassy_nrf::interrupt;
use embassy_nrf::peripherals::SAADC;
use embassy_nrf::saadc::{self, AnyInput, Oversample};

pub struct BatterySensor<'a> {
    saadc: saadc::Saadc<'a, 1>,
}

impl<'a> BatterySensor<'a> {
    pub fn new(adc_pin: AnyInput, adc: SAADC) -> Self {
        // ADC initialization
        let mut config = saadc::Config::default();
        config.oversample = Oversample::OVER64X;
        let channel_cfg = saadc::ChannelConfig::single_ended(adc_pin);
        let saadc = saadc::Saadc::new(adc, interrupt::take!(SAADC), config, [channel_cfg]);

        BatterySensor { saadc }
    }

    pub async fn calibrate(&self) {
        self.saadc.calibrate().await;
    }

    pub async fn sample(&mut self) -> u32 {
        let mut buf = [0i16; 1];
        self.saadc.sample(&mut buf).await;
        let voltage: u32 = u32::from(buf[0].unsigned_abs()) * 200000 / 113778;
        info!("Battery Sensor: got voltage: {}", voltage);
        voltage
    }
}

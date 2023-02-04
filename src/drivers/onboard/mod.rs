pub mod battery;
pub mod environment;
pub mod types;

use battery::types::BatterySensor;
use embassy_nrf::{saadc, twim};
use types::{OnboardHardware, OnboardPeripherals};

impl<'a> OnboardHardware<'a> {
    pub fn from_peripherals(per: &'a mut OnboardPeripherals) -> OnboardHardware {
        // ADC initialization
        let mut config = saadc::Config::default();
        config.oversample = saadc::Oversample::OVER64X;

        let channel_cfg = saadc::ChannelConfig::single_ended(saadc::VddInput);
        let adc = saadc::Saadc::new(
            &mut per.instance_saadc,
            &mut per.adc_irq,
            config,
            [channel_cfg],
        );
        let battery = BatterySensor::new(adc);

        // let i2c = bitbang_hal::i2c::I2cBB::new(pin_scl, pin_sda, clock);
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K400; // 100k seems to be best for low power consumption.
        i2c_config.scl_pullup = true;
        i2c_config.sda_pullup = true;

        let i2c_bus = twim::Twim::new(
            &mut per.instance_twim,
            &mut per.i2c_irq,
            &mut per.pin_sda,
            &mut per.pin_scl,
            i2c_config,
        );
        // Create a bus manager to be able to share i2c buses easily.
        let i2c_bus = shared_bus::BusManagerSimple::new(i2c_bus);

        OnboardHardware { i2c_bus, battery }
    }
}

pub mod battery;
pub mod environment;
pub mod types;

use crate::sensors::types::OnboardPeripherals;
use battery::types::BatterySensor;
use embassy_nrf::{bind_interrupts, gpio::Input, peripherals, saadc, twim};
use types::OnboardHardware;

bind_interrupts!(struct AdcIrqs {
    SAADC => saadc::InterruptHandler;
});

bind_interrupts!(struct I2cIrqs {
    SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0 => twim::InterruptHandler<peripherals::TWISPI0>;
});

impl<'a> OnboardHardware<'a> {
    pub fn from_peripherals(per: &'a mut OnboardPeripherals) -> OnboardHardware {
        // ADC initialization
        let mut config = saadc::Config::default();
        config.oversample = saadc::Oversample::OVER64X;

        let channel_cfg = saadc::ChannelConfig::single_ended(saadc::VddInput);
        let adc = saadc::Saadc::new(&mut per.instance_saadc, AdcIrqs, config, [channel_cfg]);
        let battery = BatterySensor::new(adc);

        // let i2c = bitbang_hal::i2c::I2cBB::new(pin_scl, pin_sda, clock);
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K100; // 100k seems to be best for low power consumption.
        i2c_config.scl_pullup = true;
        i2c_config.sda_pullup = true;

        let i2c_bus = twim::Twim::new(
            &mut per.instance_twim,
            I2cIrqs,
            &mut per.pin_sda,
            &mut per.pin_scl,
            i2c_config,
        );
        // Create a bus manager to be able to share i2c buses easily.
        let i2c_bus = shared_bus::BusManagerSimple::new(i2c_bus);

        // Create an interrupt pin for announcing conversion ready.
        let wait_int = Input::new(&mut per.pin_interrupt, embassy_nrf::gpio::Pull::Up);

        OnboardHardware {
            i2c_bus,
            battery,
            wait_pin: wait_int,
        }
    }
}

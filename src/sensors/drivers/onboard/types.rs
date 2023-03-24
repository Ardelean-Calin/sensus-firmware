use super::battery::types::BatterySensor;

use embassy_nrf::gpio::{AnyPin, Input};

pub type BusManagerType<'a> = shared_bus::BusManager<
    shared_bus::NullMutex<embassy_nrf::twim::Twim<'a, embassy_nrf::peripherals::TWISPI0>>,
>;

pub struct OnboardHardware<'a> {
    pub i2c_bus: BusManagerType<'a>,
    pub battery: BatterySensor<'a>,
    pub wait_pin: Input<'a, AnyPin>,
}

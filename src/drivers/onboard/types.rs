use super::{
    battery::types::{BatteryLevel, BatterySensor},
    environment::types::EnvironmentSample,
};

use defmt::Format;
use embassy_nrf::{
    gpio::{AnyPin, Input},
    peripherals::{SAADC, TWISPI0},
};

pub struct OnboardPeripherals {
    pub pin_sda: AnyPin,
    pub pin_scl: AnyPin,
    pub pin_interrupt: AnyPin,
    pub instance_twim: TWISPI0,
    pub instance_saadc: SAADC,
    pub adc_irq: embassy_nrf::interrupt::SAADC,
    pub i2c_irq: embassy_nrf::interrupt::SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0,
}

#[derive(Format, Clone, Copy)]
pub struct OnboardSample {
    pub environment_data: EnvironmentSample,
    pub battery_level: BatteryLevel,
}

pub type BusManagerType<'a> = shared_bus::BusManager<
    shared_bus::NullMutex<embassy_nrf::twim::Twim<'a, embassy_nrf::peripherals::TWISPI0>>,
>;

pub struct OnboardHardware<'a> {
    pub i2c_bus: BusManagerType<'a>,
    pub battery: BatterySensor<'a>,
    pub wait_pin: Input<'a, AnyPin>,
}

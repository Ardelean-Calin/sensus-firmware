use defmt::Format;
use embassy_nrf::{
    gpio::{AnyPin, Input, Output},
    gpiote::AnyChannel,
    interrupt::SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1,
    ppi::AnyConfigurableChannel,
    twim::Twim,
};

use super::FrequencySensor;

pub struct ProbePeripherals {
    // Used pins and hardware peripherals
    pub pin_probe_detect: AnyPin,
    pub pin_probe_enable: AnyPin,
    pub pin_probe_sda: AnyPin,
    pub pin_probe_scl: AnyPin,
    pub pin_probe_freq: AnyPin,
    pub instance_twim: embassy_nrf::peripherals::TWISPI1,
    pub instance_gpiote: AnyChannel,
    pub instance_ppi: AnyConfigurableChannel,
    pub i2c_irq: SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1,
}

pub struct ProbeHardware<'a> {
    pub input_probe_detect: Input<'a, AnyPin>,
    pub output_probe_enable: Output<'a, AnyPin>,
    pub i2c_bus: Twim<'a, crate::peripherals::TWISPI1>,
    pub freq_sensor: FrequencySensor<'a>,
}

#[derive(Format, Clone, Copy)]
pub struct ProbeSample {
    pub moisture: f32,    // 0 - 100%
    pub temperature: f32, // °C
}

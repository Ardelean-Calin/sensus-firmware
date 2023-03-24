use embassy_nrf::{
    gpio::{AnyPin, Input, Output},
    twim::Twim,
};

use super::FrequencySensor;

pub struct ProbeHardware<'a> {
    pub input_probe_detect: Input<'a, AnyPin>,
    pub output_probe_enable: Output<'a, AnyPin>,
    pub i2c_bus: Twim<'a, crate::peripherals::TWISPI1>,
    pub freq_sensor: FrequencySensor<'a>,
}

use defmt::Format;
use embassy_nrf::{
    gpio::AnyPin,
    gpiote::AnyChannel,
    peripherals::{SAADC, TWISPI0},
    ppi::AnyConfigurableChannel,
};

use crate::sensors::drivers::onboard::environment::types::EnvironmentSample;
use crate::{common::types::Filter, sensors::drivers::onboard::battery::types::BatteryLevel};

#[derive(Format, Debug, Clone, Copy)]
pub enum Error {
    /// Probe Errors
    ProbeTimeout,
    ProbeDisconnected,
    ProbeI2cFailed,
    FrequencySensor,
    // Onboard sensor errors.
    OnboardResetFailed,
    OnboardTimeout,
    SHTComm,
    OPTComm,
}

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
}

pub struct OnboardPeripherals {
    pub pin_sda: AnyPin,
    pub pin_scl: AnyPin,
    pub pin_interrupt: AnyPin,
    pub instance_twim: TWISPI0,
    pub instance_saadc: SAADC,
}

#[derive(Format, Clone, Copy)]
pub struct OnboardSample {
    pub environment_data: EnvironmentSample,
    pub battery_level: BatteryLevel,
}

#[derive(Format, Clone, Copy)]
pub struct ProbeSample {
    pub moisture: f32,    // 0 - 100%
    pub temperature: f32, // °C
}

pub struct OnboardFilter {
    env_filter: Filter<EnvironmentSample>,
    bat_filter: Filter<BatteryLevel>,
}

impl Default for OnboardFilter {
    fn default() -> Self {
        Self {
            env_filter: Filter::<EnvironmentSample>::default(),
            bat_filter: Filter::<BatteryLevel>::new(0.181),
        }
    }
}

impl OnboardFilter {
    #[allow(dead_code)]
    pub fn new(alpha_env: f32, alpha_bat: f32) -> Self {
        OnboardFilter {
            env_filter: Filter::<EnvironmentSample>::new(alpha_env),
            bat_filter: Filter::<BatteryLevel>::new(alpha_bat),
        }
    }

    pub fn feed(&mut self, data: OnboardSample) -> OnboardSample {
        OnboardSample {
            environment_data: self.env_filter.feed(data.environment_data),
            battery_level: self.bat_filter.feed(data.battery_level),
        }
    }
}

pub type ProbeFilter = Filter<ProbeSample>;

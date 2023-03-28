use defmt::Format;
use embassy_nrf::{
    gpio::AnyPin,
    gpiote::AnyChannel,
    peripherals::{SAADC, TWISPI0},
    ppi::AnyConfigurableChannel,
};
use serde::Serialize;

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

pub struct OnboardPeripherals {
    pub pin_sda: AnyPin,
    pub pin_scl: AnyPin,
    pub pin_interrupt: AnyPin,
    pub instance_twim: TWISPI0,
    pub instance_saadc: SAADC,
}

#[derive(Serialize, Format, Clone, Copy)]
pub struct OnboardSample {
    pub environment_data: EnvironmentSample,
    pub battery_level: BatteryLevel,
}

#[derive(Serialize, Format, Clone)]
pub struct OnboardFilter {
    env_filter: Filter<EnvironmentSample>,
    bat_filter: Filter<BatteryLevel>,
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

#[derive(Serialize, Format, Clone, Copy, Default)]
pub struct ProbeSample {
    pub moisture: f32,    // 0 - 100%
    pub temperature: f32, // °C
}

pub type ProbeFilter = Filter<ProbeSample>;

#[derive(Serialize, Format, Clone, Default)]
pub struct SensorDataFiltered {
    onboard: OnboardFilter,
    probe: ProbeFilter,
}

//
// Implementations
//

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

    pub fn get_value(&self) -> OnboardSample {
        OnboardSample {
            environment_data: self.env_filter.get_value().unwrap_or_default(),
            battery_level: self.bat_filter.get_value().unwrap_or_default(),
        }
    }
}

impl SensorDataFiltered {
    pub fn feed_onboard(&mut self, sample: OnboardSample) {
        self.onboard.feed(sample);
    }

    pub fn feed_probe(&mut self, sample: ProbeSample) {
        self.probe.feed(sample);
    }

    pub fn get_onboard(&self) -> OnboardSample {
        self.onboard.get_value()
    }

    pub fn get_probe(&self) -> ProbeSample {
        self.probe.get_value().unwrap_or_default()
    }
}

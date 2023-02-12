use crate::{
    common::types::Filter,
    drivers::onboard::{
        battery::types::BatteryLevel, environment::types::EnvironmentSample, types::OnboardSample,
    },
};

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
            ..data
        }
    }
}

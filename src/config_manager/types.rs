use core::str::FromStr;

use defmt::Format;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Format, Clone)]
pub enum ConfigError {
    SerializationError,
    InvalidSampleRate,
}

// TODO. Limit all values to 1 second minimum.
#[repr(C)]
#[derive(Serialize, Deserialize, Format, Clone)]
struct SampleRate {
    #[serde(with = "postcard::fixint::le")]
    onboard_sdt_plugged_ms: u32,
    #[serde(with = "postcard::fixint::le")]
    probe_sdt_plugged_ms: u32,
    #[serde(with = "postcard::fixint::le")]
    onboard_sdt_battery_ms: u32,
    #[serde(with = "postcard::fixint::le")]
    probe_sdt_battery_ms: u32,
}

// Declarations
#[repr(C)]
#[derive(Serialize, Deserialize, Format, Clone)]
pub struct SensusConfig {
    sampling_rate: SampleRate,
    status_led: StatusLedControl,
    #[defmt(Display2Format)]
    name: heapless::String<16>,
}

#[derive(Serialize, Deserialize, Format, Default, Clone)]
pub enum StatusLedControl {
    Always,
    #[default]
    PluggedIn,
    Off,
}

#[derive(Serialize, Deserialize, Format, Clone)]
pub enum ConfigPayload {
    ConfigGet,
    ConfigSet(SensusConfig),
}

#[derive(Serialize, Format, Clone)]
pub enum ConfigResponse {
    SetConfig, // Set config successfully.
    GetConfig(SensusConfig),
}

//
// Implementations
//

impl Default for SampleRate {
    fn default() -> Self {
        Self {
            onboard_sdt_plugged_ms: 10000,
            probe_sdt_plugged_ms: 10000,
            onboard_sdt_battery_ms: 30000,
            probe_sdt_battery_ms: 30000,
        }
    }
}

impl Default for SensusConfig {
    fn default() -> Self {
        Self {
            sampling_rate: Default::default(),
            status_led: StatusLedControl::PluggedIn,
            name: heapless::String::from_str("Sensus").unwrap(),
        }
    }
}

impl SensusConfig {
    pub fn verify(self) -> Result<Self, ConfigError> {
        if self.sampling_rate.onboard_sdt_battery_ms < 1000
            || self.sampling_rate.onboard_sdt_plugged_ms < 1000
            || self.sampling_rate.probe_sdt_battery_ms < 1000
            || self.sampling_rate.probe_sdt_plugged_ms < 1000
        {
            return Err(ConfigError::InvalidSampleRate);
        }

        Ok(self)
    }
}

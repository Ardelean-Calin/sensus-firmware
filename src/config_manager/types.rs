use core::str::FromStr;

use defmt::Format;
use heapless::Vec;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Format, Clone)]
pub enum ConfigError {
    SerializationError,
    InvalidSampleRate,
    Flash(u8),
}

// TODO. Limit all values to 1 second minimum.
#[repr(C)]
#[derive(Serialize, Deserialize, Format, Clone, PartialEq)]
pub struct SamplePeriod {
    #[serde(with = "postcard::fixint::le")]
    pub onboard_sdt_plugged_ms: u32,
    #[serde(with = "postcard::fixint::le")]
    pub probe_sdt_plugged_ms: u32,
    #[serde(with = "postcard::fixint::le")]
    pub onboard_sdt_battery_ms: u32,
    #[serde(with = "postcard::fixint::le")]
    pub probe_sdt_battery_ms: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct DisplayableVec<T, const N: usize>(Vec<T, N>);

impl<T, const N: usize> DisplayableVec<T, N> {
    pub fn inner(self) -> Vec<T, N> {
        self.0
    }
}

impl<T, const N: usize> Format for DisplayableVec<T, N>
where
    T: defmt::Format,
{
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", self.0.as_slice())
    }
}

// Declarations
#[repr(C)]
#[derive(Serialize, Deserialize, Format, Clone, PartialEq)]
pub struct SensusConfig {
    pub sampling_period: SamplePeriod,
    #[defmt(Display2Format)] // TODO. Remove and replace with Format implementation
    pub name: heapless::String<29>,
    pub probe_calibration: DisplayableVec<f32, 20>,
}

#[derive(Serialize, Deserialize, Format, Clone)]
pub enum ConfigPayload {
    ConfigGet,
    ConfigSet(SensusConfig),
}

#[derive(Serialize, Format, Clone)]
pub enum ConfigResponse {
    GetConfig(SensusConfig),
    SetConfig, // Set config successfully.
}

//
// Implementations
//

impl Default for SamplePeriod {
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
            sampling_period: Default::default(),
            name: heapless::String::from_str("Sensus").unwrap(),
            probe_calibration: DisplayableVec(Vec::from_slice(&[1e5, 1.0, 1.7e6, 0.0]).unwrap()),
        }
    }
}

impl SensusConfig {
    pub fn verify(self) -> Result<Self, ConfigError> {
        if self.sampling_period.onboard_sdt_battery_ms < 1000
            || self.sampling_period.onboard_sdt_plugged_ms < 1000
            || self.sampling_period.probe_sdt_battery_ms < 1000
            || self.sampling_period.probe_sdt_plugged_ms < 1000
        {
            return Err(ConfigError::InvalidSampleRate);
        }

        Ok(self)
    }
}

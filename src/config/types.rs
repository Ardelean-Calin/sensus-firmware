use core::str::FromStr;

use defmt::Format;
use serde::{Deserialize, Serialize};

// Declarations
#[repr(C)]
#[derive(Serialize, Deserialize, Format, Clone)]
pub struct SensusConfig {
    #[serde(with = "postcard::fixint::be")]
    onboard_sample_interval_ms: u32,
    #[serde(with = "postcard::fixint::be")]
    probe_sample_interval_ms: u32,
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

// Implementations
impl Default for SensusConfig {
    fn default() -> Self {
        Self {
            onboard_sample_interval_ms: 30000,
            probe_sample_interval_ms: 30000,
            status_led: StatusLedControl::PluggedIn,
            name: heapless::String::from_str("Sensus").unwrap(),
        }
    }
}

impl SensusConfig {
    pub fn set_onboard_sample_interval_ms(&mut self, time_ms: u32) {
        self.onboard_sample_interval_ms = time_ms;
    }

    pub fn set_probe_sample_interval_ms(&mut self, time_ms: u32) {
        self.probe_sample_interval_ms = time_ms;
    }

    pub fn set_status_led(&mut self, led: StatusLedControl) {
        self.status_led = led;
    }

    pub fn set_name(&mut self, name: &str) {
        let new_name = defmt::unwrap!(heapless::String::from_str(name));
        self.name = new_name;
    }
}

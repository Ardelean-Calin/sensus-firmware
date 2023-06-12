use core::{iter::zip, str::FromStr};

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

#[derive(Serialize, Deserialize, Format, Clone, PartialEq, PartialOrd, Ord, Eq)]
struct CalibrationPoint {
    frequency: u32,
    percentage: u8,
}

#[derive(Serialize, Deserialize, Format, Clone, PartialEq)]
pub struct ProbeCalibration {
    points: DisplayableVec<CalibrationPoint, 10>,
}

impl ProbeCalibration {
    pub fn as_vec(&self) -> Vec<f32, 20> {
        let mut vec = Vec::<f32, 20>::new();

        for point in self.points.0.clone() {
            defmt::unwrap!(vec.push(point.frequency as f32));
            defmt::unwrap!(vec.push((point.percentage as f32) / 100.0));
        }

        vec
    }

    fn from_vec(probe_calibration: DisplayableVec<f32, 20>) -> ProbeCalibration {
        let raw_points = probe_calibration.inner();

        // TODO: Sort by frequencies.
        let frequencies = raw_points.clone().into_iter().step_by(2);
        let percentages = raw_points.into_iter().skip(1).step_by(2);

        // My actual calibration points
        let mut points = Vec::<CalibrationPoint, 10>::new();

        for (f, p) in zip(frequencies, percentages) {
            let point = CalibrationPoint {
                frequency: f as u32,
                percentage: (p * 100.0) as u8,
            };

            match points.binary_search(&point) {
                Ok(_pos) => {} // element already in vector @ `pos`
                Err(pos) => defmt::unwrap!(points.insert(pos, point)),
            };
        }

        ProbeCalibration {
            points: DisplayableVec(points),
        }
    }
}

// Declarations
#[repr(C)]
#[derive(Serialize, Deserialize, Format, Clone, PartialEq)]
pub struct SensusConfig {
    pub sampling_period: SamplePeriod,
    #[defmt(Display2Format)] // TODO. Remove and replace with Format implementation
    pub name: heapless::String<29>,
    pub probe_calibration: ProbeCalibration,
}

#[repr(C)]
#[derive(Serialize, Deserialize, Format, Clone, PartialEq)]
pub struct SensusConfigOld {
    pub sampling_period: SamplePeriod,
    #[defmt(Display2Format)] // TODO. Remove and replace with Format implementation
    pub name: heapless::String<29>,
    pub probe_calibration: DisplayableVec<f32, 20>,
}

impl From<SensusConfigOld> for SensusConfig {
    fn from(value: SensusConfigOld) -> Self {
        let probecal = ProbeCalibration::from_vec(value.probe_calibration);

        SensusConfig {
            sampling_period: value.sampling_period,
            name: value.name,
            probe_calibration: probecal,
        }
    }
}

impl From<SensusConfig> for SensusConfigOld {
    fn from(value: SensusConfig) -> Self {
        SensusConfigOld {
            sampling_period: value.sampling_period,
            name: value.name,
            probe_calibration: DisplayableVec(value.probe_calibration.as_vec()),
        }
    }
}

#[derive(Serialize, Deserialize, Format, Clone)]
pub enum ConfigPayload {
    ConfigGet,
    ConfigSet(SensusConfigOld),
}

#[derive(Serialize, Format, Clone)]
pub enum ConfigResponse {
    GetConfig(SensusConfigOld),
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
        let points = defmt::unwrap!(Vec::from_slice(&[
            CalibrationPoint {
                frequency: 100000,
                percentage: 100,
            },
            CalibrationPoint {
                frequency: 1700000,
                percentage: 0,
            },
        ]));

        let probecal = ProbeCalibration {
            points: DisplayableVec(points),
        };

        Self {
            sampling_period: Default::default(),
            name: defmt::unwrap!(heapless::String::from_str("Sensus")),
            probe_calibration: probecal,
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

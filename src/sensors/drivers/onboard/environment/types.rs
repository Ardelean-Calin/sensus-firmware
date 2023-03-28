use core::ops::{Add, Mul, Sub};

use defmt::Format;
use serde::Serialize;

#[derive(Serialize, Format, Clone, Copy, Default)]
pub struct EnvironmentSample {
    pub illuminance: f32,
    pub temperature: f32,
    pub humidity: f32,
}

impl Add for EnvironmentSample {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        EnvironmentSample {
            illuminance: self.illuminance + rhs.illuminance,
            temperature: self.temperature + rhs.temperature,
            humidity: self.humidity + rhs.humidity,
        }
    }
}

impl Sub for EnvironmentSample {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        EnvironmentSample {
            illuminance: self.illuminance - rhs.illuminance,
            temperature: self.temperature - rhs.temperature,
            humidity: self.humidity - rhs.humidity,
        }
    }
}

impl Mul<f32> for EnvironmentSample {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        EnvironmentSample {
            illuminance: self.illuminance * rhs,
            temperature: self.temperature * rhs,
            humidity: self.humidity * rhs,
        }
    }
}

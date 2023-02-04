use core::ops::{Add, Mul, Sub};

use defmt::Format;
use embassy_nrf::saadc;

pub struct BatterySensor<'a> {
    pub saadc: saadc::Saadc<'a, 1>,
}

#[derive(Format, Clone, Copy)]
pub struct BatteryLevel {
    pub value: f32,
}

impl Add for BatteryLevel {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        BatteryLevel {
            value: self.value + rhs.value,
        }
    }
}

impl Sub for BatteryLevel {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        BatteryLevel {
            value: self.value - rhs.value,
        }
    }
}

impl Mul<f32> for BatteryLevel {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        BatteryLevel {
            value: self.value * rhs,
        }
    }
}

use super::types::*;
use core::ops::{Add, Div, Mul, Range, Sub};

impl Default for RGBColor {
    fn default() -> Self {
        Self {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        }
    }
}

impl From<(f32, f32, f32)> for RGBColor {
    fn from(value: (f32, f32, f32)) -> Self {
        RGBColor {
            red: value.0,
            green: value.1,
            blue: value.2,
        }
    }
}

impl Add for RGBColor {
    type Output = RGBColor;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            red: self.red + rhs.red,
            green: self.green + rhs.green,
            blue: self.blue + rhs.blue,
        }
    }
}

impl Sub for RGBColor {
    type Output = RGBColor;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            red: self.red - rhs.red,
            green: self.green - rhs.green,
            blue: self.blue - rhs.blue,
        }
    }
}

impl Div<usize> for RGBColor {
    type Output = RGBColor;

    fn div(self, rhs: usize) -> Self::Output {
        Self {
            red: self.red / (rhs as f32),
            green: self.green / (rhs as f32),
            blue: self.blue / (rhs as f32),
        }
    }
}

impl Mul<usize> for RGBColor {
    type Output = RGBColor;

    fn mul(self, rhs: usize) -> Self::Output {
        Self {
            red: self.red * (rhs as f32),
            green: self.green * (rhs as f32),
            blue: self.blue * (rhs as f32),
        }
    }
}

impl RGBTransition {
    pub fn new(time_ms: u16, to_rgb: RGBColor) -> Self {
        RGBTransition { time_ms, to_rgb }
    }

    pub fn colors_from(
        &mut self,
        start_color: RGBColor,
        timestep_ms: u16,
    ) -> impl Iterator<Item = RGBColor> {
        let no_items = (self.time_ms / timestep_ms) as usize;

        let _start_color = start_color;
        let _end_color = self.to_rgb;

        let color_increment = (_end_color - _start_color) / no_items;

        let range = Range {
            start: 0,
            end: no_items + 1,
        };

        range.map(move |index| _start_color + (color_increment * index))
    }
}

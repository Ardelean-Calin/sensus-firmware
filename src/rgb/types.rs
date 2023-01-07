/// RGB percentage
#[derive(Clone, Copy)]
pub struct RGBColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

#[derive(Clone, Default)]
pub struct RGBTransition {
    pub time_ms: u16, // Time in milliseconds for this transition
    pub to_rgb: RGBColor,
}

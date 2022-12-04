use embassy_nrf::{
    gpio::{AnyPin, Pin as GpioPin},
    peripherals::PWM0,
    pwm::{Instance, SimplePwm},
    Peripheral, PeripheralRef, Peripherals,
};

#[derive(Default)]
struct RGBVal {
    r: u8,
    g: u8,
    b: u8,
}

pub struct RGBLED<'a> {
    pwm_controller: SimplePwm<'a, PWM0>,
    _rgb_val: RGBVal,
}

impl<'a> RGBLED<'a> {
    pub fn new_rgb(p: &'a mut Peripherals) -> Self {
        let pwm_controller =
            SimplePwm::new_3ch(&mut p.PWM0, &mut p.P0_22, &mut p.P0_23, &mut p.P0_25);
        pwm_controller.set_max_duty(0xFF);
        Self {
            pwm_controller,
            _rgb_val: RGBVal::default(),
        }
    }

    /// Sets the color of the RGB LED.
    pub fn set_color_rgb(&mut self, r: u8, g: u8, b: u8) {
        self._rgb_val = RGBVal { r, g, b };
        self.pwm_controller.set_duty(0, r.into());
        self.pwm_controller.set_duty(1, g.into());
        self.pwm_controller.set_duty(2, b.into());
    }
}

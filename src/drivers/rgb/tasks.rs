use defmt::info;
use embassy_nrf::{gpio::AnyPin, peripherals::PWM0, pwm::SimplePwm};
use embassy_time::{Duration, Timer};

use super::types::{RGBColor, RGBTransition};
// use crate::{PLUGGED_DETECT, RGB_ROUTER};

const MAX_DUTY: u16 = 4095;
const TIMESTEP_MS: u64 = 10;

#[embassy_executor::task]
pub async fn rgb_task(
    mut pwm: PWM0,
    mut pin_red: AnyPin,
    mut pin_green: AnyPin,
    mut pin_blue: AnyPin,
) {
    info!("rgb_task task created.");
    // run_while_plugged_in!(PLUGGED_DETECT, async {
    defmt::warn!("RGB task started");
    let mut current_color = RGBColor::default();
    let mut mypwm = SimplePwm::new_3ch(&mut pwm, &mut pin_red, &mut pin_green, &mut pin_blue);
    mypwm.set_max_duty(MAX_DUTY);
    mypwm.set_duty(0, 0);
    mypwm.set_duty(1, 0);
    mypwm.set_duty(2, 0);
    mypwm.enable();

    loop {
        mypwm.set_duty(1, MAX_DUTY);
        Timer::after(Duration::from_millis(500)).await;
        mypwm.set_duty(1, 0u16);
        Timer::after(Duration::from_millis(500)).await;
    }
    // })
    // .await;
}

// #[embassy_executor::task]
// pub async fn heartbeat_task() {
//     info!("Heartbeat task created.");
//     loop {
//         // This channel has a capacity of 1, so it blocks until the RGBTransition is taken and consumed.
//         RGB_ROUTER
//             .send(RGBTransition::new(1000, (0.0, 0.1, 0.0).into()))
//             .await;
//         RGB_ROUTER
//             .send(RGBTransition::new(1000, (0.0, 0.5, 0.0).into()))
//             .await;
//     }
// }

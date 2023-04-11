use core::option::Iter;

use embassy_nrf::{peripherals::PWM0, pwm::SimplePwm};
use embassy_time::{Duration, Instant, Ticker};
use futures::StreamExt;

use crate::power_manager::{wait_for_hp, wait_for_lp};

// mod impls;
// mod types;
// pub mod tasks;
// // This is an external struct that will be used to transition the LEDs.
// pub(crate) use types::RGBTransition;

#[derive(Default)]
enum LEDBrightness {
    #[default]
    Percent0 = 0,
    Percent25 = 1024,
    Percent50 = 2048,
    Percent75 = 3072,
    Percent100 = 4095,
}

/// Using these global app states I can determine how my LED pattern should look like.
///
/// Plugged in:
///   - Communication via BLE: blue LED blinks fast.
///   - Communication via UART: green LED blinks fast.
///   - No Communication: green LED constantly on (or slow heartbeat).
///   - Error: RED LED blinks fast.
///
/// How to implement:
/// Since all my indications are with blinks, I could increase a MutexCounter whenever I have communication.
/// Then my task iterates through all mutex counters and blinks for every counter that has a non-zero value.
/// If all counters have a zero value, we go back to the default heartbeat pattern.
enum AppStates {
    // OnBatteryNoComm,
    // OnBatteryCommBLE,
    PluggedInNoComm,
    PluggedInCommUART,
    PluggedInCommBLE,
    PluggedInError,
}

struct Keyframe {
    time_ms: u16,
    value: RgbValue,
}

type Keyframes = [Option<Keyframe>; 10];

// My valid states:
// USB-powered
//  - no UART connection
//      - DFU ongoing
//      - DFU !ongoing
//  - UART connection
//      - DFU ongoing
//      - DFU !ongoing
//

// I will create such function pairs for every pattern I want.
/// Runs when we have external power.
pub fn activate_ext_pwr_pattern() {}
pub fn deactivate_ext_pwr_pattern() {}

/// Runs when we are connected to the PC app.
pub fn activate_connected_pattern() {}
pub fn deactivate_connected_pattern() {}

/// Runs when DFU is ongoing.
pub fn activate_dfu_pattern() {}
pub fn deactivate_dfu_pattern() {}

#[derive(Default)]
struct RgbValue {
    red: LEDBrightness,
    green: LEDBrightness,
    blue: LEDBrightness,
}

impl RgbValue {
    fn green() -> Self {
        Self {
            red: LEDBrightness::Percent0,
            green: LEDBrightness::Percent50,
            blue: LEDBrightness::Percent0,
        }
    }
}

// #[derive(Default)]
// struct RgbProvider {
//     rgb: RgbValue,
// }

// impl RgbProvider {
//     fn heartbeat() -> Iter<'_, RgbValue> {}
// }

struct StatusLed<'a, T>
where
    T: embassy_nrf::pwm::Instance,
{
    pwm: SimplePwm<'a, T>,
}
impl<'a, T: embassy_nrf::pwm::Instance> StatusLed<'a, T> {
    fn new(
        pwm: &'a mut T,
        pin_red: &'a mut embassy_nrf::gpio::AnyPin,
        pin_green: &'a mut embassy_nrf::gpio::AnyPin,
        pin_blue: &'a mut embassy_nrf::gpio::AnyPin,
    ) -> Self {
        let mut mypwm = SimplePwm::new_3ch(pwm, pin_red, pin_green, pin_blue);
        mypwm.set_max_duty(4096);
        mypwm.set_duty(0, 0);
        mypwm.set_duty(1, 0);
        mypwm.set_duty(2, 0);
        StatusLed { pwm: mypwm }
    }

    fn set_value(&mut self, rgb: RgbValue) {
        self.pwm.set_duty(0, rgb.red as u16);
        self.pwm.set_duty(1, rgb.green as u16);
        self.pwm.set_duty(2, rgb.blue as u16);
    }
}

const PERIODICITY: Duration = Duration::from_hz(10);
async fn rgb_ticker(mut status_led: StatusLed<'_, PWM0>) {
    let mut ticker = Ticker::every(PERIODICITY);
    // let mut rgb_provider = RgbProvider::default();
    // let heartbeat_provider = RgbProvider::heartbeat();
    loop {
        // TODO. Integrate time somehow
        // Fallback: no blinks sheduled. We heartbeat.
        // let rgb_value = rgb_provider.next(Instant::now()).unwrap();
        // status_led.set_value(rgb_value);
        status_led.set_value(RgbValue::green());
        ticker.next().await;
    }
}

#[embassy_executor::task]
pub async fn rgb_task(// mut pwm: embassy_nrf::peripherals::PWM0,
    // mut pin_red: embassy_nrf::gpio::AnyPin,
    // mut pin_green: embassy_nrf::gpio::AnyPin,
    // mut pin_blue: embassy_nrf::gpio::AnyPin,
) {
    defmt::info!("Started RGB task");
    loop {
        wait_for_hp().await;
        defmt::info!("RGB task went into High Power");
        wait_for_lp().await;
        defmt::info!("RGB task went into Low Power");
    }
}

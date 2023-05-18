use core::cmp::{max, min};

use defmt::unwrap;
use embassy_futures::select::{select, select3};
use embassy_nrf::{
    peripherals::PWM0,
    ppi::Ppi,
    pwm::{
        Prescaler, Sequence, SequenceConfig, SequenceLoad, SequencePwm, Sequencer, SimplePwm,
        SingleSequenceMode, SingleSequencer,
    },
};
use embassy_time::{Duration, Ticker, Timer};
use heapless::Vec;

use crate::{
    config_manager::{types::StatusLedControl, SENSUS_CONFIG},
    globals::RX_BUS,
    power_manager::{wait_for_hp, wait_for_lp},
};

// mod impls;
// mod types;
// pub mod tasks;
// // This is an external struct that will be used to transition the LEDs.
// pub(crate) use types::RGBTransition;

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
    value: HSVColor,
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

trait Abs {
    fn abs(&self) -> f32;
}

impl Abs for f32 {
    fn abs(&self) -> f32 {
        if self.max(0.0) == 0.0 {
            -self.clone()
        } else {
            self.clone()
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
struct RGBColor {
    r: f32,
    g: f32,
    b: f32,
}

#[derive(Default, Clone, Copy)]
struct HSVColor {
    h: f32, // angle: 0 - 360
    s: f32, // 0 - 1
    v: f32, // 0 - 1
}

impl HSVColor {
    fn red() -> Self {
        Self {
            h: 0.0,
            s: 1.0,
            v: 1.0,
        }
    }

    fn green() -> Self {
        Self {
            h: 120.0,
            s: 1.0,
            v: 1.0,
        }
    }

    fn blue() -> Self {
        Self {
            h: 240.0,
            s: 1.0,
            v: 1.0,
        }
    }

    fn to_rgb(&self) -> RGBColor {
        fn formula(color: &HSVColor, n: usize) -> f32 {
            let k = ((n as f32) + color.h / 60.0) % 6.0;

            color.v - color.v * color.s * 0.0f32.max(k.min(1.0f32.min(4.0f32 - k)))
        }

        let red = formula(self, 5);
        let green = formula(self, 3);
        let blue = formula(self, 1);

        RGBColor {
            r: red,
            g: green,
            b: blue,
        }
    }

    fn off() -> Self {
        Self {
            h: 0.0,
            s: 0.0,
            v: 0.0,
        }
    }
}

impl core::ops::Sub<HSVColor> for HSVColor {
    type Output = HSVColor;

    fn sub(self, rhs: HSVColor) -> Self::Output {
        HSVColor {
            h: (self.h - rhs.h),
            s: (self.s - rhs.s),
            v: (self.v - rhs.v),
        }
    }
}

impl core::ops::Div<usize> for HSVColor {
    type Output = HSVColor;

    fn div(self, rhs: usize) -> Self::Output {
        HSVColor {
            h: self.h / (rhs as f32),
            s: self.s / (rhs as f32),
            v: self.v / (rhs as f32),
        }
    }
}

impl core::ops::Add<HSVColor> for HSVColor {
    type Output = HSVColor;

    fn add(self, rhs: HSVColor) -> Self::Output {
        HSVColor {
            h: self.h + rhs.h,
            s: self.s + rhs.s,
            v: self.v + rhs.v,
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
    pwm: SequencePwm<'a, T>,
    raw_value: HSVColor,
    brightness: f32,
}
impl<'a, T: embassy_nrf::pwm::Instance> StatusLed<'a, T> {
    fn new(
        pwm: &'a mut T,
        pin_red: &'a mut embassy_nrf::gpio::AnyPin,
        pin_green: &'a mut embassy_nrf::gpio::AnyPin,
        pin_blue: &'a mut embassy_nrf::gpio::AnyPin,
    ) -> Self {
        let mut config = embassy_nrf::pwm::Config::default();
        config.sequence_load = SequenceLoad::Individual; // [r0, g0, b0, r1, g1, b1, etc.]
        config.prescaler = Prescaler::Div16; // Resulting clock: 1MHz

        let mypwm = unwrap!(SequencePwm::new_3ch(
            pwm, pin_red, pin_green, pin_blue, config,
        ));
        StatusLed {
            pwm: mypwm,
            raw_value: HSVColor::off(),
            brightness: 4095.0,
        }
    }

    async fn transition_value(&mut self, target: HSVColor, duration: Duration) {
        const STEPS: usize = 100;
        const SIZE: usize = STEPS * 4; // 4, one for each channel, Ch0, Ch1, Ch2 and Ch3. Doesn't matter that we don't use every channel
        let difference = target - self.raw_value;
        // To transition from the current color to that color, I need to do STEPS number of delta_val
        // increments.
        let delta_val = difference / STEPS;
        let mut seq_config = SequenceConfig::default();
        // For some reason, refresh is in millis aka thousands of periods...
        let delta_t = duration.as_millis() / (STEPS as u64);
        seq_config.refresh = (delta_t - 1) as u32;

        let sequence: Vec<u16, SIZE> = (0..STEPS)
            .scan(self.raw_value, |state, _| {
                *state = *state + delta_val;
                Some(*state)
            })
            .map(|v| v.to_rgb())
            .flat_map(|v| {
                [
                    (v.r * 1000.0) as u16,
                    (v.g * 1000.0) as u16,
                    (v.b * 1000.0) as u16,
                    0u16, // Appearently this is needed, as we have 4 PWM channels
                ]
            })
            .collect();
        self.raw_value = target;
        let sequencer = SingleSequencer::new(&mut self.pwm, &sequence, seq_config);
        unwrap!(sequencer.start(SingleSequenceMode::Times(1)));
        Timer::after(duration).await;
        sequencer.stop();
    }

    // async fn transition_brightnes()

    async fn self_check(&mut self) {
        self.transition_value(
            HSVColor {
                h: 0.0,
                s: 1.0,
                v: 1.0,
            },
            Duration::from_millis(300),
        )
        .await;
        self.transition_value(
            HSVColor {
                h: 300.0,
                s: 1.0,
                v: 1.0,
            },
            Duration::from_millis(3000),
        )
        .await;
        self.transition_value(HSVColor::off(), Duration::from_millis(300))
            .await;
    }
}

#[embassy_executor::task]
pub async fn rgb_task(
    mut pwm: embassy_nrf::peripherals::PWM0,
    mut pin_red: embassy_nrf::gpio::AnyPin,
    mut pin_green: embassy_nrf::gpio::AnyPin,
    mut pin_blue: embassy_nrf::gpio::AnyPin,
) {
    let mut data_rx = RX_BUS
        .dyn_subscriber()
        .expect("Failed to acquire subscriber.");
    defmt::info!("Started RGB task");
    {
        let mut statusled = StatusLed::new(&mut pwm, &mut pin_red, &mut pin_green, &mut pin_blue);
        statusled.self_check().await;
    }
    loop {
        wait_for_hp().await;
        {
            let mut statusled =
                StatusLed::new(&mut pwm, &mut pin_red, &mut pin_green, &mut pin_blue);
            for _ in 0..3 {
                statusled
                    .transition_value(HSVColor::green(), Duration::from_millis(250))
                    .await;
                statusled
                    .transition_value(HSVColor::off(), Duration::from_millis(250))
                    .await;
            }
        }
        select(wait_for_lp(), async {
            loop {
                match select(
                    data_rx.next_message(),
                    Timer::after(Duration::from_millis(500)),
                )
                .await
                {
                    embassy_futures::select::Either::First(_) => {
                        let mut statusled =
                            StatusLed::new(&mut pwm, &mut pin_red, &mut pin_green, &mut pin_blue);
                        statusled
                            .transition_value(HSVColor::green(), Duration::from_millis(100))
                            .await;
                        statusled
                            .transition_value(HSVColor::off(), Duration::from_millis(100))
                            .await;
                    }
                    embassy_futures::select::Either::Second(_) => {
                        // TODO: Some kind of connected status, maybe?
                    }
                };
            }
        })
        .await;
    }
}

use defmt::unwrap;
use embassy_futures::select::select;
use embassy_nrf::pwm::{
    Prescaler, SequenceConfig, SequenceLoad, SequencePwm, SingleSequenceMode, SingleSequencer,
};
use embassy_time::{Duration, Timer};
use heapless::Vec;

use crate::{
    globals::RX_BUS,
    power_manager::{wait_for_hp, wait_for_lp},
};

#[derive(Default, Clone, Copy)]
struct RgbValue {
    red: f32,
    green: f32,
    blue: f32,
}

impl RgbValue {
    fn red() -> Self {
        Self {
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        }
    }

    fn green() -> Self {
        Self {
            red: 0.0,
            green: 1.0,
            blue: 0.0,
        }
    }

    fn blue() -> Self {
        Self {
            red: 0.0,
            green: 0.0,
            blue: 1.0,
        }
    }

    fn off() -> Self {
        Self {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        }
    }
}

impl core::ops::Sub<RgbValue> for RgbValue {
    type Output = RgbValue;

    fn sub(self, rhs: RgbValue) -> Self::Output {
        RgbValue {
            red: (self.red - rhs.red),
            green: (self.green - rhs.green),
            blue: (self.blue - rhs.blue),
        }
    }
}

impl core::ops::Div<usize> for RgbValue {
    type Output = RgbValue;

    fn div(self, rhs: usize) -> Self::Output {
        RgbValue {
            red: self.red / (rhs as f32),
            green: self.green / (rhs as f32),
            blue: self.blue / (rhs as f32),
        }
    }
}

impl core::ops::Add<RgbValue> for RgbValue {
    type Output = RgbValue;

    fn add(self, rhs: RgbValue) -> Self::Output {
        RgbValue {
            red: self.red + rhs.red,
            green: self.green + rhs.green,
            blue: self.blue + rhs.blue,
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
    raw_value: RgbValue,
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
            raw_value: RgbValue::off(),
        }
    }

    async fn transition_value(&mut self, target: RgbValue, duration: Duration) {
        const STEPS: usize = 24;
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
            .flat_map(|v| {
                [
                    (v.red * 1000.0) as u16,
                    (v.green * 1000.0) as u16,
                    (v.blue * 1000.0) as u16,
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

    async fn self_check(&mut self) {
        for _ in 0..2 {
            self.transition_value(RgbValue::red(), Duration::from_millis(300))
                .await;
            self.transition_value(RgbValue::green(), Duration::from_millis(300))
                .await;
            self.transition_value(RgbValue::blue(), Duration::from_millis(300))
                .await;
        }
        self.transition_value(RgbValue::off(), Duration::from_millis(300))
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
                    .transition_value(RgbValue::green(), Duration::from_millis(250))
                    .await;
                statusled
                    .transition_value(RgbValue::off(), Duration::from_millis(250))
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
                            .transition_value(RgbValue::green(), Duration::from_millis(100))
                            .await;
                        statusled
                            .transition_value(RgbValue::off(), Duration::from_millis(100))
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

mod types;

use defmt::unwrap;

use embassy_futures::join::join;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::with_timeout;
use embassy_time::Instant;
use futures::StreamExt;

use crate::drivers::onboard::battery;
use crate::drivers::onboard::environment;
use crate::drivers::onboard::types::{
    OnboardError, OnboardHardware, OnboardPeripherals, OnboardSample,
};

use types::{OnboardSM, OnboardSMState};

pub static ONBOARD_DATA: Mutex<ThreadModeRawMutex, Option<OnboardSample>> = Mutex::new(None);

pub async fn run(mut per: OnboardPeripherals) {
    let mut sm = OnboardSM::new();
    loop {
        defmt::info!("Current state: {:?}", &sm.state);
        match sm.state {
            OnboardSMState::FirstRun => {
                let hw = OnboardHardware::from_peripherals(&mut per);
                unwrap!(environment::init(hw.i2c_bus).await);
                sm = sm.with_state(OnboardSMState::Start);
            }
            OnboardSMState::Start => {
                // TODO: Get configuration data from global config
                sm = sm.with_state(OnboardSMState::Measure);
                // sm = sm.with_state(OnboardSMState::Sleep);
            }
            OnboardSMState::Measure => {
                let res = with_timeout(embassy_time::Duration::from_millis(200), async {
                    let hw = OnboardHardware::from_peripherals(&mut per);
                    let environment_data = environment::sample_environment(hw.i2c_bus).await;
                    let battery_level = battery::sample_battery_level(hw.battery).await;

                    let sample = OnboardSample {
                        environment_data,
                        battery_level,
                        current_time: Instant::now(),
                    };

                    Some(sample)
                })
                .await;

                match res {
                    Ok(sample) => {
                        sm = sm.with_state(OnboardSMState::Publish(sample));
                    }
                    Err(_) => {
                        sm = sm.with_state(OnboardSMState::Error(OnboardError::Timeout));
                    }
                }
            }
            OnboardSMState::Publish(sample) => {
                let mut data = ONBOARD_DATA.lock().await;
                *data = sample;
                sm = sm.with_state(OnboardSMState::Sleep);
            }
            OnboardSMState::Sleep => {
                sm.ticker.next().await;
                sm = sm.with_state(OnboardSMState::Measure);
                // sm = sm.with_state(OnboardSMState::Start);
            }
            OnboardSMState::Error(_) => {
                // TODO. Do something in case of error.
                defmt::error!("Onboard sensors timeout error.");
                sm = sm.with_state(OnboardSMState::Sleep);
            }
        }
    }
}

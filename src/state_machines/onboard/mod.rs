mod types;

use defmt::unwrap;
use embassy_time::{with_timeout, Duration, TimeoutError};
use futures::StreamExt;

use crate::coroutines::packet_builder::ONBOARD_DATA_SIG;
use crate::drivers::onboard::battery;
use crate::drivers::onboard::environment;
use crate::drivers::onboard::types::{
    OnboardError, OnboardHardware, OnboardPeripherals, OnboardSample,
};

use types::{OnboardSM, OnboardSMState};

pub async fn run(mut per: OnboardPeripherals) {
    let mut sm = OnboardSM::new();
    loop {
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
                let res: Result<Result<OnboardSample, OnboardError>, TimeoutError> =
                    with_timeout(Duration::from_millis(200), async {
                        let hw = OnboardHardware::from_peripherals(&mut per);

                        let environment_data =
                            environment::sample_environment(hw.i2c_bus, hw.wait_int)
                                .await
                                .map_err(OnboardError::Environment)?;
                        let battery_level = battery::sample_battery_level(hw.battery).await;

                        let sample = OnboardSample {
                            environment_data,
                            battery_level,
                        };

                        Ok(sample)
                    })
                    .await;

                match res {
                    Ok(Ok(sample)) => {
                        sm = sm.with_state(OnboardSMState::Publish(sample));
                    }
                    Ok(Err(e)) => {
                        defmt::error!("TODO. Got an error while sampling.");
                        sm = sm.with_state(OnboardSMState::Error(e));
                    }
                    Err(_timeout) => {
                        sm = sm.with_state(OnboardSMState::Error(OnboardError::Timeout));
                    }
                }
            }
            OnboardSMState::Publish(sample) => {
                ONBOARD_DATA_SIG.signal(sample);
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

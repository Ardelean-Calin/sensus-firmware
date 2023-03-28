mod types;

use defmt::{error, trace};
use embassy_time::Timer;
use embassy_time::{with_timeout, Duration};

use crate::globals::ONBOARD_DATA_SIG;
use crate::sensors::drivers::onboard::battery;
use crate::sensors::drivers::onboard::environment;
use crate::sensors::drivers::onboard::types::OnboardHardware;
use crate::sensors::types::Error;
use crate::sensors::types::OnboardPeripherals;
use crate::sensors::types::OnboardSample;

use types::{OnboardSM, OnboardSMState};

/// Executes one tick of the state-machine and returns the errors if any.
async fn tick(sm: &mut OnboardSM, per: &mut OnboardPeripherals) -> Result<(), Error> {
    match sm.state {
        OnboardSMState::FirstRun => {
            let hw = OnboardHardware::from_peripherals(per);
            environment::reset(hw.i2c_bus)?;
            // Delay for about 10ms to ensure all I2C sensors have started up.
            Timer::after(Duration::from_millis(10)).await;
            sm.state = OnboardSMState::Start;
        }
        OnboardSMState::Start => {
            // TODO: Get configuration data from global config
            sm.state = OnboardSMState::Measure;
        }
        OnboardSMState::Measure => {
            let sample = with_timeout(Duration::from_millis(200), async {
                let hw = OnboardHardware::from_peripherals(per);

                let environment_data =
                    environment::sample_environment(hw.i2c_bus, hw.wait_pin).await?;
                let battery_level = battery::sample_battery_level(hw.battery).await;

                let sample = OnboardSample {
                    environment_data,
                    battery_level,
                };

                Ok(sample)
            })
            .await
            .map_err(|_| Error::OnboardTimeout)
            .flatten()?;

            trace!("Got new INSTANTANEOUS onboard sample: {:?}", sample);

            sm.state = OnboardSMState::Publish(sample);
        }
        OnboardSMState::Publish(sample) => {
            ONBOARD_DATA_SIG.signal(sample);
            sm.state = OnboardSMState::Sleep;
        }
        OnboardSMState::Sleep => {
            sm.ticker.next().await;
            sm.state = OnboardSMState::Measure;
        }
    };

    Ok(())
}

///  Runs the onboard sensor state machine.
pub async fn run(mut per: OnboardPeripherals) {
    let mut sm = OnboardSM::new();
    loop {
        let result = tick(&mut sm, &mut per).await;
        match result {
            Ok(_) => {}
            Err(e) => {
                match e {
                    Error::OnboardResetFailed => {
                        error!("CRITICAL! Onboard sensors Reset error.");
                    }
                    Error::OnboardTimeout => {
                        error!("Onboard sensors timeout error.");
                    }
                    Error::SHTComm => {
                        error!("Error communicating with SHTC3.")
                    }
                    Error::OPTComm => {
                        error!("Error communicating with OPT3001.")
                    }
                    _ => {
                        error!("Unexpected Error: {:?}", e)
                    }
                };
                sm.state = OnboardSMState::Sleep;
            }
        }
    }
}

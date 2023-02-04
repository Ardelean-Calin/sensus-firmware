mod types;

use defmt::unwrap;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use embassy_time::{with_timeout, Duration};
use futures::StreamExt;

use crate::{
    drivers::probe::sample_soil,
    drivers::probe::types::{ProbeError, ProbeHardware, ProbePeripherals, ProbeSample},
};

use types::{ProbeSM, ProbeSMState};

pub static PROBE_DATA: Mutex<ThreadModeRawMutex, Option<ProbeSample>> = Mutex::new(None);

pub async fn run(mut per: ProbePeripherals) {
    // Local variables.
    let mut sm = ProbeSM::new();
    loop {
        defmt::info!("Probe state: {:?}", sm.state);
        match sm.state {
            ProbeSMState::Start => {
                // TODO: Get probe configuration data from global config
                // ticker_interval set in sm.ticker.
                sm = sm.with_state(ProbeSMState::Measure);
            }
            ProbeSMState::Measure => {
                let res = with_timeout(Duration::from_millis(200), async {
                    let hw = ProbeHardware::from_peripherals(&mut per);
                    let sample = sample_soil(hw).await;
                    sample
                })
                .await;

                match res {
                    Ok(sample) => {
                        sm = sm.with_state(ProbeSMState::Publish(sample));
                    }
                    Err(_) => {
                        sm = sm.with_state(ProbeSMState::Error(ProbeError::TimeoutError));
                    }
                }
            }
            ProbeSMState::Publish(sample) => {
                let mut data = PROBE_DATA.lock().await;
                *data = sample;
                sm = sm.with_state(ProbeSMState::Sleep);
            }
            ProbeSMState::Sleep => {
                sm.ticker.next().await;
                sm = sm.with_state(ProbeSMState::Measure);
            }
            ProbeSMState::Error(_) => {
                // TODO. Do something in case of error.
                defmt::error!("Probe timeout error.");
                sm = sm.with_state(ProbeSMState::Sleep);
            }
        }
    }
}

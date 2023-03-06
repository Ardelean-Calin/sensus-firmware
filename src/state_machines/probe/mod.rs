mod types;

use embassy_time::{with_timeout, Duration};
use futures::StreamExt;

use crate::{
    coroutines::packet_builder::PROBE_DATA_SIG,
    drivers::probe::sample_soil,
    drivers::probe::types::{ProbeHardware, ProbePeripherals},
    types::Error,
};

use types::{ProbeSM, ProbeSMState};

pub async fn run(mut per: ProbePeripherals) {
    // Local variables.
    let mut sm = ProbeSM::new();
    loop {
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
                    Ok(Some(sample)) => {
                        sm = sm.with_state(ProbeSMState::Publish(sample));
                    }
                    Ok(None) => {
                        defmt::error!("TODO: Error sampling probe.");
                        sm = sm.with_state(ProbeSMState::Sleep);
                    }
                    Err(_) => {
                        sm = sm.with_state(ProbeSMState::Error(Error::ProbeTimeout));
                    }
                }
            }
            ProbeSMState::Publish(sample) => {
                PROBE_DATA_SIG.signal(sample);
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

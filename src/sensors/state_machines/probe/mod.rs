mod types;

use defmt::{error, trace};
use embassy_time::{with_timeout, Duration};
use futures::StreamExt;

use crate::{
    globals::PROBE_DATA_SIG, sensors::drivers::probe::sample_soil,
    sensors::drivers::probe::types::ProbeHardware, sensors::types::Error,
    sensors::types::ProbePeripherals,
};

use types::{ProbeSM, ProbeSMState};

/// Executes one tick of the state-machine and returns the errors if any.
async fn tick(sm: &mut ProbeSM, per: &mut ProbePeripherals) -> Result<(), Error> {
    match sm.state {
        ProbeSMState::Start => {
            // TODO: Get probe configuration data from global config
            // ticker_interval set in sm.ticker.
            sm.state = ProbeSMState::Measure;
        }
        ProbeSMState::Measure => {
            let sample = with_timeout(Duration::from_millis(200), async {
                let hw = ProbeHardware::from_peripherals(per);
                let sample = sample_soil(hw).await;
                sample
            })
            .await
            .map_err(|_| Error::ProbeTimeout)
            .flatten()?;

            trace!("Got a new probe sample: {:?}", sample);

            sm.state = ProbeSMState::Publish(sample);
        }
        ProbeSMState::Publish(sample) => {
            PROBE_DATA_SIG.signal(sample);
            sm.state = ProbeSMState::Sleep;
        }
        ProbeSMState::Sleep => {
            sm.ticker.next().await;
            sm.state = ProbeSMState::Measure;
        }
    };

    Ok(())
}

/// Runs the probe state machine.
pub async fn run(mut per: ProbePeripherals) {
    // Local variables.
    let mut sm = ProbeSM::new();
    loop {
        let res = tick(&mut sm, &mut per).await;

        match res {
            Ok(_) => {}
            Err(e) => {
                error!("Error sampling probe: {:?}", e);
                // Note. Here I can match e.
                sm.state = ProbeSMState::Sleep;
            }
        }
    }
}

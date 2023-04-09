mod types;

use defmt::{error, trace};
use embassy_time::{with_timeout, Duration, Ticker, Timer};

use crate::{
    globals::PROBE_DATA_SIG,
    sensors::drivers::probe::types::ProbeHardware,
    sensors::types::Error,
    sensors::types::ProbePeripherals,
    sensors::{drivers::probe::sample_soil, PROBE_SAMPLE_PERIOD},
};

use types::{ProbeSM, ProbeSMState};

/// Executes one tick of the state-machine and returns the errors if any.
async fn tick(sm: &mut ProbeSM, per: &mut ProbePeripherals) -> Result<(), Error> {
    match sm.state {
        ProbeSMState::Start => {
            // Get probe configuration data from global config
            let period = PROBE_SAMPLE_PERIOD.load(core::sync::atomic::Ordering::Relaxed) as u64;
            sm.ticker = Ticker::every(Duration::from_millis(period));

            // Just to be safe, keep the power line of the probe low for a bit on first start.
            // This should reset any attached circuits.
            let mut hw = ProbeHardware::from_peripherals(per);
            hw.output_probe_enable.set_low();
            Timer::after(Duration::from_millis(100)).await;

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
pub async fn run(per_ref: &mut ProbePeripherals) {
    // Local variables.
    let mut sm = ProbeSM::new();
    loop {
        let res = tick(&mut sm, per_ref).await;

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

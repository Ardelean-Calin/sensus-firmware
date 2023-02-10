use defmt::Format;
use embassy_time::{Duration, Ticker};

use crate::drivers::probe::types::{ProbeError, ProbeSample};

#[derive(Format)]
pub enum ProbeSMState {
    Start,
    Measure,
    Publish(ProbeSample),
    Error(ProbeError),
    Sleep,
}

pub struct ProbeSM {
    pub state: ProbeSMState,
    pub ticker: Ticker,
}

impl ProbeSM {
    pub fn new() -> Self {
        ProbeSM {
            state: ProbeSMState::Start,
            ticker: Ticker::every(Duration::from_secs(3)),
        }
    }

    pub fn with_state(self, state: ProbeSMState) -> Self {
        Self { state, ..self }
    }

    pub fn with_ticker(self, ticker: Ticker) -> Self {
        Self { ticker, ..self }
    }
}

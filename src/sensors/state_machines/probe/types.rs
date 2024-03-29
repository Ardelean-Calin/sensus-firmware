use defmt::Format;
use embassy_time::{Duration, Ticker};

use crate::sensors::types::ProbeSample;

#[derive(Format)]
pub enum ProbeSMState {
    /// Startup code. Should only run once.
    Start,
    Measure,
    Publish(ProbeSample),
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
            ticker: Ticker::every(Duration::from_secs(10)),
        }
    }

    #[allow(dead_code)]
    pub fn with_ticker(self, ticker: Ticker) -> Self {
        Self { ticker, ..self }
    }
}

use defmt::Format;
use embassy_time::{Duration, Ticker};

use crate::drivers::probe::types::ProbeSample;

#[derive(Format)]
pub enum ProbeSMState {
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
            ticker: Ticker::every(Duration::from_secs(3)),
        }
    }

    #[allow(dead_code)]
    pub fn with_ticker(self, ticker: Ticker) -> Self {
        Self { ticker, ..self }
    }
}

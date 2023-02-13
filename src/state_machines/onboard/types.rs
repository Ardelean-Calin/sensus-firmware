use defmt::Format;
use embassy_time::{Duration, Ticker};

use crate::drivers::onboard::types::{OnboardError, OnboardSample};

#[derive(Format)]
pub enum OnboardSMState {
    FirstRun,
    Start,
    Measure,
    Publish(OnboardSample),
    Error(OnboardError),
    Sleep,
}

pub struct OnboardSM {
    pub state: OnboardSMState,
    pub ticker: Ticker,
}

impl OnboardSM {
    pub fn new() -> Self {
        OnboardSM {
            state: OnboardSMState::FirstRun,
            ticker: Ticker::every(Duration::from_secs(3)),
        }
    }

    pub fn with_state(self, state: OnboardSMState) -> Self {
        Self { state, ..self }
    }

    pub fn with_ticker(self, ticker: Ticker) -> Self {
        Self { ticker, ..self }
    }
}

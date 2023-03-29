use defmt::Format;
use embassy_time::{Duration, Ticker};

use crate::sensors::types::OnboardSample;

#[derive(Format)]
pub enum OnboardSMState {
    Start,
    Measure,
    Publish(OnboardSample),
    Sleep,
}

pub struct OnboardSM {
    pub state: OnboardSMState,
    pub ticker: Ticker,
}

impl OnboardSM {
    pub fn new() -> Self {
        OnboardSM {
            state: OnboardSMState::Start,
            ticker: Ticker::every(Duration::from_secs(10)),
        }
    }

    #[allow(dead_code)]
    pub fn with_ticker(self, ticker: Ticker) -> Self {
        Self { ticker, ..self }
    }
}

pub enum BleSMState {
    Advertising,
    GattDisconnected,
}

pub struct BleSM {
    pub state: BleSMState,
    // pub ticker: Ticker,
}

impl BleSM {
    pub fn new() -> Self {
        BleSM {
            state: BleSMState::Advertising,
        }
    }

    pub fn with_state(self, state: BleSMState) -> Self {
        Self { state }
    }
}

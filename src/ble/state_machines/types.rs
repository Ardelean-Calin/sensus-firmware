use defmt::Format;

#[derive(Format)]
pub enum BleSMState {
    Startup,
    WaitForAdvdata, // Waits for new advertisment data. If gotten, completes a future
    Debounce,
    Advertising,
    #[allow(dead_code)]
    GattDisconnected,
}

#[derive(Format)]
pub struct BleSM {
    pub state: BleSMState,
    // pub ticker: Ticker,
}

impl BleSM {
    pub fn new() -> Self {
        BleSM {
            state: BleSMState::Startup,
        }
    }

    pub fn with_state(self, state: BleSMState) -> Self {
        Self { state }
    }
}

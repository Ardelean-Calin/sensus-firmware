use defmt::Format;
use heapless::Vec;

#[derive(Format)]
pub enum DfuError {
    FrameCounterError,
}

#[derive(Format)]
pub enum DfuState {
    Idle,
    NextFrame,
    FlashPage,
    Done,
    Error(DfuError),
}

#[derive(Format)]
pub struct DfuStateMachine {
    pub frame_counter: u8,
    pub page_offset: usize,
    pub binary_size: usize,
    pub state: DfuState,
}

impl DfuStateMachine {
    pub fn new() -> Self {
        DfuStateMachine {
            frame_counter: 0,
            page_offset: 0,
            binary_size: 0,
            state: DfuState::Idle,
        }
    }

    pub fn with_state(self, state: DfuState) -> Self {
        Self { state, ..self }
    }
}

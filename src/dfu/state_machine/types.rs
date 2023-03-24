use defmt::Format;

use crate::types::DfuError;

pub struct DfuStateMachine {
    pub frame_counter: u8,
    pub binary_size: usize,
    pub state: DfuState,
}

impl DfuStateMachine {
    pub fn new() -> Self {
        DfuStateMachine {
            frame_counter: 0,
            binary_size: 0,
            state: DfuState::Idle,
        }
    }
}

#[derive(Format, Clone)]
pub enum DfuState {
    Idle,
    WaitBlock,
    Error(DfuError),
    Done,
}

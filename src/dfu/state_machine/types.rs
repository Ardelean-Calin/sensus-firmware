use defmt::Format;

use crate::dfu::types::DfuError;

pub struct DfuStateMachine {
    pub frame_counter: u8,
    pub binary_size: usize,
    pub state: DfuSmState,
}

#[derive(Format, Clone)]
pub enum DfuSmState {
    Idle,
    WaitBlock,
    FlashPage,
    Error(DfuError),
    Done,
}

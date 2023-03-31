use defmt::Format;

use crate::dfu::types::{DfuBlock, DfuError};

pub struct DfuStateMachine {
    pub current_block: u16,
    pub total_no_blocks: u16,
    pub binary_size: usize,
    pub state: DfuSmState,
}

#[derive(Format, Clone)]
pub enum DfuSmState {
    Waiting,
    RequestBlock,
    ProcessBlock(DfuBlock),
    Error(DfuError),
    Done,
}

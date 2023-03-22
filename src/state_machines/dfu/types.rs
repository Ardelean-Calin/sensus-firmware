use defmt::Format;
use heapless::Vec;

use crate::types::DfuError;

#[derive(Clone, Default)]
pub struct Page {
    pub offset: usize,
    pub data: Vec<u8, 4096>,
}

impl Page {
    pub fn length(&self) -> usize {
        self.data.len()
    }

    pub fn is_full(&self) -> bool {
        self.data.is_full()
    }

    pub fn clear_data(&mut self) {
        self.data.clear();
    }
}

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

use defmt::Format;
use heapless::Vec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
pub struct Page {
    pub offset: usize,
    pub data: Vec<u8, 4096>,
}

#[derive(Clone, Serialize, Deserialize, Format)]
pub enum DfuPayload {
    Header(DfuHeader),
    Block(DfuBlock),
    RequestFwVersion,
}

#[derive(Deserialize, Serialize, Clone, Format)]
pub struct DfuHeader {
    #[serde(with = "postcard::fixint::le")]
    pub binary_size: u32,
}

#[derive(Clone, Serialize, Deserialize, Format)]
pub struct DfuBlock {
    pub counter: u8,
    pub data: [u8; 32],
}

#[derive(Serialize, Clone, Format)]
pub enum DfuError {
    StateMachineError,
    CounterError,
    TimeoutError,
    UnexpectedFrame,
}

// Implementations

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

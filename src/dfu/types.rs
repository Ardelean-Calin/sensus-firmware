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
    #[serde(with = "postcard::fixint::le")]
    StartDfu(u32),
    Block(DfuBlock),
    RequestFwVersion,
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
    pub fn new() -> Self {
        Self {
            offset: 0,
            data: Vec::new(),
        }
    }

    pub fn length(&self) -> usize {
        self.data.len()
    }

    pub fn is_full(&self) -> bool {
        self.data.is_full()
    }

    pub fn reset(&mut self) {
        self.data.clear();
        self.offset = 0;
    }

    pub fn increment_page(&mut self) {
        self.data.clear();
        self.offset += 4096;
    }
}

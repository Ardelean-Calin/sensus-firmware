use defmt::Format;
use heapless::Vec;
use serde::{Deserialize, Serialize};

/// The size of each DFU transaction
const BLOCK_SIZE: usize = 64;

#[derive(Clone, Default)]
pub struct Page {
    pub offset: usize,
    pub data: Vec<u8, 4096>,
}

#[repr(C)]
#[derive(Clone, Serialize, Deserialize, Format)]
pub enum DfuPayload {
    StartDfu(DfuHeader),
    Block(DfuBlock),
    RequestFwVersion,
}

#[derive(Clone, Serialize, Deserialize, Format)]
pub struct DfuHeader {
    #[serde(with = "postcard::fixint::le")]
    pub binary_size: u32,
    #[serde(with = "postcard::fixint::le")]
    pub no_blocks: u16,
}

#[repr(C)]
#[derive(Clone, Serialize, Deserialize, Format)]
pub struct DfuBlock {
    #[serde(with = "postcard::fixint::le")]
    pub block_idx: u16,
    #[defmt(Debug2Format)]
    pub data: Vec<u8, BLOCK_SIZE>,
}

#[derive(Serialize, Clone, Format)]
pub enum DfuError {
    StateMachineError,
    TimeoutError,
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
}

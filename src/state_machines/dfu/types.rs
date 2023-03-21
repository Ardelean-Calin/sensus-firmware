use defmt::Format;
use heapless::Vec;

#[derive(Clone, Default)]
pub struct Page {
    pub offset: usize,
    pub data: Vec<u8, 4096>,
}

impl Page {
    pub fn empty() -> Self {
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

    pub fn clear_data(&mut self) {
        self.data.clear();
    }
}

#[derive(Format, Clone)]
pub enum DfuState {
    Idle,
    NextFrame,
    Done,
}

pub struct DfuStateMachine {
    pub frame_counter: u8,
    pub binary_size: usize,
    pub page: Page,
    pub state: DfuState,
}

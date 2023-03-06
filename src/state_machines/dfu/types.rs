use defmt::Format;
use heapless::Vec;

#[derive(Clone, Default)]
pub struct Page {
    pub data: Vec<u8, 4096>,
}

impl Page {
    fn empty() -> Self {
        Self { data: Vec::new() }
    }

    pub fn length(&self) -> usize {
        self.data.len()
    }

    pub fn is_full(&self) -> bool {
        self.data.is_full()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

#[derive(Format)]
pub enum DfuState {
    Idle,
    NextFrame,
    FlashPage,
    Done,
}

pub struct DfuStateMachine {
    pub frame_counter: u8,
    pub page_offset: usize,
    pub binary_size: usize,
    pub page: Page,
    pub state: DfuState,
}

impl DfuStateMachine {
    pub fn new() -> Self {
        DfuStateMachine {
            frame_counter: 0,
            page_offset: 0,
            binary_size: 0,
            page: Page::empty(),
            state: DfuState::Idle,
        }
    }
}

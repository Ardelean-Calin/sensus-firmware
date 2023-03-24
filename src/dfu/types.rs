use heapless::Vec;

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

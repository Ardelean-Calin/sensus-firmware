use defmt::Format;
use heapless::Vec;
use serde::{Deserialize, Serialize};

#[allow(non_camel_case_types)]
#[derive(Format, Serialize, Deserialize, Clone, Default, PartialEq)]
pub enum PacketID {
    #[default]
    STREAM_START = 0x31, // Starts data streaming via UART
    STREAM_STOP = 0x32, // Stops data streaming via UART
    DFU_START = 0x33,   // Represents the start of a dfu operation

    REQ_NO_PAGES = 0x34,  // Request remaining pages in DFU process.
    DFU_NO_PAGES = 0x35,  // The received number of pages.
    REQ_NEXT_PAGE = 0x36, // Indicates to the updater to increment the page number.

    DFU_NO_FRAMES = 0x37, // The number of 128-byte frames in the requested page.
    REQ_NEXT_FRAME = 0x38, // Updater, please give me the next 128-byte frame.

    DFU_FRAME = 0x39, // This is how we represent a DFU frame.
    DFU_DONE = 0x3A,  // Sent by us to mark that the DFU is done.
    REQ_RETRY = 0xFE, // Retry sending the last frame.
    Error = 0xFF,     // Represents an error
}

#[derive(Clone)]
pub enum CommError {
    PhysError, // Error at the physical layer (UART or BLE)
    MalformedPacket,
    Timeout,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CommPacket {
    pub id: PacketID,
    pub data: heapless::Vec<u8, 128>,
}

impl CommPacket {
    pub fn retry() -> CommPacket {
        CommPacket {
            id: PacketID::REQ_RETRY,
            data: Vec::new(),
        }
    }

    pub fn is(&self, id: PacketID) -> bool {
        self.id == id
    }
}

#[derive(Format, Clone, Copy)]
pub struct FilteredFloat {
    value: Option<f32>,
    alpha: f32,
}

impl FilteredFloat {
    /// Creates a new filtered float with given alpha constant.
    fn new(alpha: f32) -> Self {
        if !(0.0..=1.0).contains(&alpha) {
            panic!(
                "Wrong alpha value of {:?}. Expected a number between 0 and 1!",
                alpha
            );
        }

        Self { value: None, alpha }
    }

    /// Feeds a new value to the filter, resulting in the stored value being the filtered one.
    fn feed(&mut self, new_value: f32) {
        if let Some(prev_val) = self.value {
            let filtered = prev_val + self.alpha * (new_value - prev_val);
            self.value = Some(filtered);
        } else {
            self.value = Some(new_value);
        }
    }
}

impl Default for FilteredFloat {
    /// Creates a default FilteredFloat. The default behavior is to tend towards more filtering.
    fn default() -> Self {
        Self {
            value: Default::default(),
            alpha: 0.1,
        }
    }
}

impl From<f32> for FilteredFloat {
    fn from(value: f32) -> Self {
        FilteredFloat {
            value: Some(value),
            ..Default::default()
        }
    }
}

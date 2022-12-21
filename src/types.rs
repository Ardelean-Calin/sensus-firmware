use defmt::Format;
use heapless::Vec;
use serde::{Deserialize, Serialize};

#[allow(non_camel_case_types)]
#[derive(Format, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
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
    ERROR = 0xFF,     // Represents an error
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

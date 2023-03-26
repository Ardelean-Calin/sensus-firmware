use defmt::Format;

use serde::{Deserialize, Serialize};

use crate::config::types::ConfigPayload;
use crate::dfu::types::DfuPayload;

#[derive(Format, Clone, Serialize)]
pub enum PacketError {
    /// Error with the physical reception of bytes. For example due to noise on UART.
    PhysError,
    /// General decoding error when trying to create a packet from raw bytes.
    DeserializationError,
    PacketCRC,
}

#[allow(clippy::enum_variant_names)]
#[derive(Format, Debug, Clone, Copy)]
pub enum UartError {
    /// Error at the physical layer (UART or BLE).
    UartRx,
    UartTx,
    UartBufferFull,
}

#[repr(C)]
#[derive(Serialize, Deserialize, Clone, Format)]
pub struct ConfigHeader {
    // pub enable_logging: bool,
}

#[derive(Clone, Serialize, Deserialize, Format)]
pub struct CommPacket {
    pub payload: CommPacketType,
    #[serde(with = "postcard::fixint::le")]
    pub crc: u16,
}

// NOTE: Due to postcard's limitations I cannot give them ID's unfortunately.
#[repr(u8)]
#[derive(Clone, Serialize, Deserialize, Format)]
pub enum CommPacketType {
    DfuPacket(DfuPayload),
    ConfigPacket(ConfigPayload),
    // LogPacket(LogPayload),
}

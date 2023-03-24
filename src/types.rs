use defmt::Format;

use serde::{Deserialize, Serialize};

#[derive(Format, Clone, Serialize)]
pub enum PacketError {
    /// Error with the physical reception of bytes. For example due to noise on UART.
    PhysError,
    /// General decoding error when trying to create a packet from raw bytes.
    DeserializationError,
    PacketCRC,
}

#[derive(Serialize, Clone)]
pub enum CommResponse {
    OK(ResponseTypeOk),
    NOK(ResponseTypeErr),
}

// pub enum ResponseType {
//     ResponseTypeOk = {
//         Dfu(DfuOkType),
//         Config,
//         Log,}
// }
#[derive(Serialize, Clone)]
pub enum ResponseTypeOk {
    NoData,
    Dfu(DfuOkType),
    Config,
    Log,
}

#[derive(Serialize, Clone)]
pub enum ResponseTypeErr {
    Packet(PacketError),
    Dfu(DfuError),
}

#[derive(Serialize, Clone)]
pub enum DfuOkType {
    FirmwareVersion([u8; 6]),
    NextFrame,
    DfuDone,
}

#[derive(Serialize, Clone, Format)]
pub enum DfuError {
    StateMachineError,
    CounterError,
    UnexpectedFrame,
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
    // LogPacket(LogPayload),
    // ConfigPacket(ConfigPayload),
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

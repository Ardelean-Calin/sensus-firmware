use defmt::Format;
use serde::{Deserialize, Serialize};

use crate::dfu::types::DfuError;

use crate::config_manager::types::{ConfigError, ConfigPayload, ConfigResponse};
use crate::dfu::types::DfuPayload;
use crate::sensors::types::SensorDataFiltered;

#[derive(Serialize, Format, Clone)]
pub enum CommResponse {
    Ok(ResponseTypeOk),
    Err(ResponseTypeErr),
}

#[derive(Serialize, Format, Clone)]
pub enum ResponseTypeOk {
    #[allow(dead_code)]
    NoData,
    Dfu(DfuResponse),
    Config(ConfigResponse), // Returns either a config or just an OK if we stored the config
    SensorData(SensorDataFiltered),
}

#[derive(Serialize, Format, Clone)]
pub enum ResponseTypeErr {
    Packet(PacketError),
    Dfu(DfuError),
    Config(ConfigError),
}

#[derive(Serialize, Format, Clone)]
pub enum DfuResponse {
    FirmwareVersion(&'static str),
    NextBlock,
    DfuDone,
}

#[derive(Format, Clone, Serialize)]
pub enum PacketError {
    /// Error with the physical reception of bytes. For example due to noise on UART.
    PhysError,
    /// General decoding error when trying to create a packet from raw bytes.
    DeserializationError,
    PacketCRC,
}

#[derive(Clone, Serialize, Deserialize, Format)]
pub struct CommPacket {
    pub payload: CommPacketType,
    #[serde(with = "postcard::fixint::le")]
    pub crc: u16,
}

// NOTE: I din't find out how to give them custom IDs: https://github.com/jamesmunns/postcard/issues/55
#[repr(u8)]
#[derive(Clone, Serialize, Deserialize, Format)]
pub enum CommPacketType {
    DfuPacket(DfuPayload),
    ConfigPacket(ConfigPayload),
    GetLatestSensordata,
}

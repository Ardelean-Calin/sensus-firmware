use crc::{Crc, CRC_16_GSM};
use defmt::Format;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, pubsub::PubSubChannel};

use postcard::from_bytes;
use serde::{de, Deserialize, Deserializer, Serialize};

pub const CRC_GSM: Crc<u16> = Crc::<u16>::new(&CRC_16_GSM);

#[derive(Format, Debug, Clone, Copy)]
pub enum Error {
    /// General decoding error when trying to create a packet from raw bytes.
    DfuPacketDecode(&'static str),
    DfuPacketCRC,
    /// DFU-related Errors
    DfuCounterError,
    DfuTimeout,
    /// Error at the physical layer (UART or BLE).
    UartRx,
    UartTx,
    UartBufferFull,
    /// A COBS decoding error happened due to for ex. missing bytes.
    CobsDecodeError,
    /// Probe Errors
    ProbeTimeout,
    ProbeDisconnected,
    ProbeI2cError,
    FrequencySensor,
    // Onboard sensor errors.
    OnboardResetFailed,
    OnboardTimeout,
    SHTCommError,
    OPTCommError,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DfuBlockPayload {
    pub counter: u8,
    pub data: [u8; 32],
}

impl Format for DfuBlockPayload {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "counter({:?})  data({:?})",
            self.counter,
            self.data.as_slice()
        );
    }
}

fn u32_deserializer<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let s: [u8; 4] = de::Deserialize::deserialize(deserializer).unwrap();
    let res = u32::from_le_bytes(s);
    Ok(res)
}

#[repr(C)]
#[derive(Serialize, Deserialize, Clone, Format)]
pub struct DfuHeader {
    #[serde(deserialize_with = "u32_deserializer")]
    pub binary_size: u32,
}

// NOTE: Due to postcard's limitations I cannot give them ID's unfortunately.
#[repr(u8)]
#[derive(Clone, Serialize, Deserialize, Format)]
pub enum RawPacket {
    // Responses (from us to them)
    RespOK = 0x00,
    RespNOK = 0x01,
    RespHandshake = 0x02,
    RespDfuRequestBlock = 0x03,
    RespDfuDone = 0x04,
    RespDfuFailed = 0x05,
    // Received packet IDs
    RecvHandshake = 0x06,
    RecvDfuStart(DfuHeader) = 0x07,
    RecvDfuBlock(DfuBlockPayload) = 0x08,
}

#[derive(Clone, Serialize)]
pub struct Packet {
    pub raw: RawPacket,
    // Checksum of Header + Payload
    pub checksum: u16,
}

impl Packet {
    pub(crate) fn from_slice(slice: &[u8]) -> Result<Self, Error> {
        let mut payload_iter = slice.iter();

        // Extract the checksum and check if it's a fine checksum
        let checksum = match (payload_iter.next_back(), payload_iter.next_back()) {
            (Some(byte1), Some(byte2)) => Ok(u16::from_be_bytes([*byte1, *byte2])),
            _ => Err(Error::DfuPacketDecode("Could not extract CRC.")),
        }?;
        let actual_checksum = CRC_GSM.checksum(payload_iter.as_slice());

        // Raise an error if checksum doesn't match.
        if checksum != actual_checksum {
            return Err(Error::DfuPacketCRC);
        }

        // Checksum is fine... continue
        let payload_bytes = payload_iter.as_slice();
        // Build Payload
        let raw: RawPacket =
            from_bytes(payload_bytes).expect("Error during serialization into RawPacket.");

        Ok(Packet {
            raw,
            // Checksum of Header + Payload
            checksum,
        })
    }
}

/// Used by BLE & UART to send data to the DFU State Machine. That's why we have two publishers.
pub static RX_BUS: PubSubChannel<ThreadModeRawMutex, Result<RawPacket, Error>, 3, 1, 2> =
    PubSubChannel::new();
/// Used by DFU to send data. Either via UART or BLE => that's why we have two subscribers.
pub static TX_BUS: PubSubChannel<ThreadModeRawMutex, RawPacket, 3, 2, 1> = PubSubChannel::new();

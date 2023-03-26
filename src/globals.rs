use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::signal::Signal;

use crate::ble::types::AdvertismentPayload;
use crate::comm_manager::types::CommResponse;
use crate::sensors::types::OnboardSample;
use crate::sensors::types::ProbeSample;
use crate::types::{CommPacket, PacketError};

/// Used by BLE & UART to send data to the DFU State Machine. That's why we have two publishers.
pub static RX_BUS: PubSubChannel<ThreadModeRawMutex, Result<CommPacket, PacketError>, 3, 1, 2> =
    PubSubChannel::new();

/// Used by DFU to send data. Either via UART or BLE => that's why we have two subscribers.
pub static TX_BUS: PubSubChannel<ThreadModeRawMutex, CommResponse, 3, 2, 1> = PubSubChannel::new();

// These busses are used to transmit the latest onboard and probe sensor data.
pub static ONBOARD_DATA_SIG: Signal<ThreadModeRawMutex, OnboardSample> = Signal::new();
pub static PROBE_DATA_SIG: Signal<ThreadModeRawMutex, ProbeSample> = Signal::new();

/// Receives advertisment payload.
pub static BLE_ADV_PKT_QUEUE: Channel<ThreadModeRawMutex, AdvertismentPayload, 1> = Channel::new();

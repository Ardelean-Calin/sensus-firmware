use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::signal::Signal;
use heapless::Vec;

use crate::drivers::ble::types::AdvertismentPayload;
use crate::drivers::onboard::types::OnboardSample;
use crate::drivers::probe::types::ProbeSample;
use crate::state_machines::dfu::types::Page;
use crate::types::{CommPacket, CommResponse, PacketError};

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

pub static GLOBAL_PAGE: Mutex<ThreadModeRawMutex, Page> = Mutex::new(Page {
    offset: 0,
    data: Vec::<u8, 4096>::new(),
});

pub static DFU_SIG_NEW_PAGE: Signal<ThreadModeRawMutex, bool> = Signal::new();
pub static DFU_SIG_FLASHED: Signal<ThreadModeRawMutex, bool> = Signal::new();
pub static DFU_SIG_DONE: Signal<ThreadModeRawMutex, bool> = Signal::new();

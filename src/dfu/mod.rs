pub mod types;

mod state_machine;

use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use types::DfuPayload;

/// Used to send data to the DFU state machine to process.
static PAYLOAD_PROVIDER: Signal<ThreadModeRawMutex, DfuPayload> = Signal::new();

#[embassy_executor::task]
pub async fn dfu_task() {
    // The DFU state machine runs forever in the background.
    state_machine::run().await;
}

/// Feeds a newly received DFU Payload to the DFU state machine always running in the background.
///
/// This function is basically the public interface to our DFU mechanism! This is the only thing
/// we need to run in order to do DFU.
pub async fn process_payload(payload: DfuPayload) {
    PAYLOAD_PROVIDER.signal(payload);
}

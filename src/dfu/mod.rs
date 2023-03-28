pub mod types;

mod state_machine;

use crate::comm_manager::types::DfuResponse;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use types::DfuPayload;

use self::types::DfuError;

/// Used to send data to the DFU state machine to process.
static INPUT_SIG: Signal<ThreadModeRawMutex, DfuPayload> = Signal::new();
/// Used to receive responses from the DFU state machine. It tells us how we should respond to the master.
static OUTPUT_SIG: Signal<ThreadModeRawMutex, Result<DfuResponse, DfuError>> = Signal::new();

#[embassy_executor::task]
pub async fn dfu_task() {
    // The DFU state machine runs forever in the background.
    state_machine::run().await;
}

/// Feeds a newly received DFU Payload to the DFU state machine always running in the background.
///
/// This function is basically the public interface to our DFU mechanism! This is the only thing
/// we need to run in order to do DFU.
pub async fn process_payload(payload: DfuPayload) -> Result<DfuResponse, DfuError> {
    INPUT_SIG.signal(payload);

    let response = OUTPUT_SIG.wait().await;
    response
}

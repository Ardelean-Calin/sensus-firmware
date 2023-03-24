pub mod types;

mod state_machine;

use crate::{
    globals::{DFU_SIG_DONE, DFU_SIG_FLASHED, DFU_SIG_NEW_PAGE, GLOBAL_PAGE},
    types::CommResponse,
};
use embassy_boot_nrf::FirmwareUpdater;
use embassy_futures::{
    join::join,
    select::{select, Either},
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use nrf_softdevice::Flash;
use types::DfuPayload;

static INPUT_SIG: Signal<ThreadModeRawMutex, DfuPayload> = Signal::new();
static OUTPUT_SIG: Signal<ThreadModeRawMutex, CommResponse> = Signal::new();

// This FW version should be
// static FW_VERSION: AtomicU32 = AtomicU32::new(0x01);

/// Runs continously and monitors wether flashing a page is necessary. Also resets the system
/// if DFU is done.
async fn dfu_flash_loop(mut flash: Flash) {
    let mut updater = FirmwareUpdater::default();
    loop {
        match select(DFU_SIG_NEW_PAGE.wait(), DFU_SIG_DONE.wait()).await {
            Either::First(_) => {
                let page = GLOBAL_PAGE.lock().await;
                // Flashes the received page.
                updater
                    .write_firmware(page.offset, page.data.as_slice(), &mut flash, page.length())
                    .await
                    .unwrap();
                DFU_SIG_FLASHED.signal(true);
            }
            Either::Second(_) => {
                defmt::info!("DFU Done! Resetting in 3 seconds...");
                Timer::after(Duration::from_secs(3)).await;
                // Mark the firmware as updated and reset!
                let mut magic = [0; 4];
                updater.mark_updated(&mut flash, &mut magic).await.unwrap();
                // Reset microcontroller.
                cortex_m::peripheral::SCB::sys_reset();
            }
        }
    }
}

#[embassy_executor::task]
pub async fn dfu_task(flash: Flash) {
    join(
        dfu_flash_loop(flash),
        // The DFU state machine runs forever in the background.
        state_machine::run(),
    )
    .await;
}

/// Feeds a newly received DFU Payload to the DFU state machine always running in the background.
pub async fn process_payload(payload: DfuPayload) -> CommResponse {
    INPUT_SIG.signal(payload);

    let response = OUTPUT_SIG.wait().await;
    response
}

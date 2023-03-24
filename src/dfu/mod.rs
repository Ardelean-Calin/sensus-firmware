use embassy_boot_nrf::FirmwareUpdater;
use nrf_softdevice::Flash;

use crate::config::load_sensus_config;

// This FW version should be
// static FW_VERSION: AtomicU32 = AtomicU32::new(0x01);

#[embassy_executor::task]
pub async fn dfu_task(mut flash: Flash) {
    let mut updater = FirmwareUpdater::default();
    let mut cfg = load_sensus_config();
    defmt::info!("cfg: {:?}", cfg);

    // join(
    //     async {
    //         loop {
    //             match select(DFU_SIG_NEW_PAGE.wait(), DFU_SIG_DONE.wait()).await {
    //                 Either::First(_) => {
    //                     let page = GLOBAL_PAGE.lock().await;
    //                     // Flashes the received page.
    //                     updater
    //                         .write_firmware(
    //                             page.offset,
    //                             page.data.as_slice(),
    //                             &mut flash,
    //                             page.length(),
    //                         )
    //                         .await
    //                         .unwrap();
    //                     DFU_SIG_FLASHED.signal(true);
    //                 }
    //                 Either::Second(_) => {
    //                     defmt::info!("DFU Done! Resetting in 3 seconds...");
    //                     Timer::after(Duration::from_secs(3)).await;
    //                     // Mark the firmware as updated and reset!
    //                     let mut magic = [0; 4];
    //                     updater.mark_updated(&mut flash, &mut magic).await.unwrap();
    //                     // Reset microcontroller.
    //                     cortex_m::peripheral::SCB::sys_reset();
    //                 }
    //             }
    //         }
    //     },
    //     state_machines::dfu::run(),
    // )
    // .await;
}

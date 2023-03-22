use embassy_boot_nrf::FirmwareUpdater;
use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Timer};
use nrf_softdevice::Flash;

use crate::coroutines;
use crate::drivers;
use crate::globals::{DFU_SIG_DONE, DFU_SIG_FLASHED, DFU_SIG_NEW_PAGE, GLOBAL_PAGE};
use crate::state_machines;

#[embassy_executor::task]
pub async fn packet_manager_task() {
    coroutines::packet_builder::run().await;
}

#[embassy_executor::task]
pub async fn soil_task(per: drivers::probe::types::ProbePeripherals) {
    state_machines::probe::run(per).await;
}

#[embassy_executor::task]
pub async fn onboard_task(per: drivers::onboard::types::OnboardPeripherals) {
    state_machines::onboard::run(per).await;
}

#[embassy_executor::task]
pub async fn comm_task() {
    state_machines::comm::run().await;
}

#[embassy_executor::task]
pub async fn dfu_task(mut flash: Flash) {
    let mut updater = FirmwareUpdater::default();
    join(
        async {
            loop {
                match select(DFU_SIG_NEW_PAGE.wait(), DFU_SIG_DONE.wait()).await {
                    Either::First(_) => {
                        let page = GLOBAL_PAGE.lock().await;
                        // Flashes the received page.
                        updater
                            .write_firmware(
                                page.offset,
                                page.data.as_slice(),
                                &mut flash,
                                page.length(),
                            )
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
        },
        state_machines::dfu::run(),
    )
    .await;
}

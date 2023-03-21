use embassy_boot_nrf::FirmwareUpdater;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use nrf_softdevice::Flash;

use crate::{
    coroutines, drivers,
    state_machines::{self, dfu::types::Page},
};

// pub mod app;
// pub mod dfu_task;
// pub mod sensors;

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

pub struct DfuPage {
    pub dfu_done: bool,
    pub page: Page,
}
pub static FLASH_BUS: Signal<ThreadModeRawMutex, Page> = Signal::new();
pub static DFU_DONE: Signal<ThreadModeRawMutex, bool> = Signal::new();
#[embassy_executor::task]
pub async fn dfu_task(mut flash: Flash) {
    let mut updater = FirmwareUpdater::default();
    loop {
        match select(FLASH_BUS.wait(), DFU_DONE.wait()).await {
            Either::First(page) => {
                // Flashes the received page.
                updater
                    .write_firmware(page.offset, page.data.as_slice(), &mut flash, page.length())
                    .await
                    .unwrap();
            }
            Either::Second(_done) => {
                defmt::info!("DFU Done! Resetting...");
                // Mark the firmware as updated and reset!
                let mut magic = [0; 4];
                updater.mark_updated(&mut flash, &mut magic).await.unwrap();
                // Reset microcontroller.
                cortex_m::peripheral::SCB::sys_reset();
            }
        }
    }
}

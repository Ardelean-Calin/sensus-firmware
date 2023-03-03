use nrf_softdevice::Flash;

use crate::{coroutines, drivers, state_machines};

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
pub async fn dfu_task(flash: Flash) {
    state_machines::dfu::run(flash).await;
}

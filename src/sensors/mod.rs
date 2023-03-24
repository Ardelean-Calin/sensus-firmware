pub mod types;

// Private mods
mod drivers;
mod state_machines;

#[embassy_executor::task]
pub async fn soil_task(per: types::ProbePeripherals) {
    state_machines::probe::run(per).await;
}

#[embassy_executor::task]
pub async fn onboard_task(per: types::OnboardPeripherals) {
    state_machines::onboard::run(per).await;
}

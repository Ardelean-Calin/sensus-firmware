use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};

use self::types::SensorDataFiltered;

pub mod types;

// Private mods
mod drivers;
mod state_machines;

pub static LATEST_SENSOR_DATA: Mutex<ThreadModeRawMutex, Option<SensorDataFiltered>> =
    Mutex::new(None);

#[embassy_executor::task]
pub async fn soil_task(per: types::ProbePeripherals) {
    state_machines::probe::run(per).await;
}

#[embassy_executor::task]
pub async fn onboard_task(per: types::OnboardPeripherals) {
    state_machines::onboard::run(per).await;
}

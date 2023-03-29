use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex, pubsub::PubSubChannel};

use self::types::SensorDataFiltered;

pub mod types;

// Private mods
mod drivers;
mod state_machines;

pub static LATEST_SENSOR_DATA: Mutex<ThreadModeRawMutex, Option<SensorDataFiltered>> =
    Mutex::new(None);

static RESTART_SM: PubSubChannel<ThreadModeRawMutex, bool, 1, 2, 1> = PubSubChannel::new();

#[embassy_executor::task]
pub async fn soil_task(mut per: types::ProbePeripherals) {
    let mut flag = RESTART_SM.dyn_subscriber().unwrap();
    loop {
        select(
            flag.next_message_pure(),
            state_machines::probe::run(&mut per),
        )
        .await;
    }
}

#[embassy_executor::task]
pub async fn onboard_task(mut per: types::OnboardPeripherals) {
    let mut flag = RESTART_SM.dyn_subscriber().unwrap();
    loop {
        select(
            flag.next_message_pure(),
            state_machines::onboard::run(&mut per),
        )
        .await;
    }
}

/// Restarts all state machines. Make sure the startup state of each state machine
/// is properly written so as to reset the communication.
pub async fn restart_state_machines() {
    RESTART_SM.dyn_immediate_publisher().publish_immediate(true);
}

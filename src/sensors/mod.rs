use core::sync::atomic::AtomicU32;

use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex, signal::Signal};

use self::types::SensorDataRaw;

pub mod types;

// Private mods
mod drivers;
mod state_machines;

pub static PROBE_SAMPLE_PERIOD: AtomicU32 = AtomicU32::new(u32::MAX);
pub static ONBOARD_SAMPLE_PERIOD: AtomicU32 = AtomicU32::new(u32::MAX);
pub static LATEST_SENSOR_DATA: Mutex<ThreadModeRawMutex, Option<SensorDataRaw>> = Mutex::new(None);

/// Restarts both state machines when written to. Why not a Signal?
/// Because a signal would not notify both tasks, and only one would be restarted.
static RESTART_SM_ONBOARD: Signal<ThreadModeRawMutex, bool> = Signal::new();
static RESTART_SM_PROBE: Signal<ThreadModeRawMutex, bool> = Signal::new();

#[embassy_executor::task]
pub async fn soil_task(mut per: types::ProbePeripherals) {
    loop {
        select(
            RESTART_SM_PROBE.wait(),
            state_machines::probe::run(&mut per),
        )
        .await;
    }
}

#[embassy_executor::task]
pub async fn onboard_task(mut per: types::OnboardPeripherals) {
    loop {
        select(
            RESTART_SM_ONBOARD.wait(),
            state_machines::onboard::run(&mut per),
        )
        .await;
    }
}

/// Restarts all state machines. Make sure the startup state of each state machine
/// is properly written so as to reset the communication.
pub fn restart_state_machines() {
    RESTART_SM_ONBOARD.signal(true);
    RESTART_SM_PROBE.signal(true);
}

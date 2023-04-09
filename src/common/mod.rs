use crate::{ble, sensors};

pub mod types;

pub fn restart_state_machines() {
    sensors::restart_state_machines();
    // ble::restart_state_machine().await;
}

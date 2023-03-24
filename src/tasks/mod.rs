use crate::coroutines;
use crate::state_machines;

#[embassy_executor::task]
pub async fn packet_manager_task() {
    coroutines::packet_builder::run().await;
}

#[embassy_executor::task]
pub async fn comm_task() {
    state_machines::comm::run().await;
}

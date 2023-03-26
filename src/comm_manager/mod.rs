pub mod types;
use crate::globals::{RX_BUS, TX_BUS};
use types::{CommResponse, ResponseTypeErr};

/// This is the main Communication loop. It handles everything communication-related.
/// Data comes in via a subscriber and gets sent away via a publisher.
async fn comm_mgr_loop() {
    let mut data_rx = RX_BUS
        .dyn_subscriber()
        .expect("Failed to acquire subscriber.");
    let data_tx = TX_BUS
        .dyn_publisher()
        .expect("Failed to acquire publisher.");

    loop {
        match data_rx.next_message_pure().await {
            Ok(packet) => {
                let response: CommResponse = match packet.payload {
                    crate::types::CommPacketType::DfuPacket(payload) => {
                        // Feed to DFU state machine for processing.
                        crate::dfu::process_payload(payload).await
                    }
                    crate::types::CommPacketType::ConfigPacket(payload) => {
                        crate::config::process_payload(payload).await
                    }
                };

                // Response can be OK or NOK, depending on how the state machine processed the received payload.
                data_tx.publish(response).await;
            }
            Err(err) => {
                data_tx
                    .publish(CommResponse::NOK(ResponseTypeErr::Packet(err)))
                    .await
            }
        }
    }
}

#[embassy_executor::task]
pub async fn comm_task() {
    comm_mgr_loop().await;
}

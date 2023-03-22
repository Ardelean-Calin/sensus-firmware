mod types;

use crate::globals::{RX_BUS, TX_BUS};
use crate::types::{CommResponse, ResponseTypeErr};

/// This is the main Communication State Machine. It handles everything communication-related.
/// Data comes in via a subscriber and gets sent away via a publisher.
pub async fn run() {
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
                        crate::state_machines::dfu::process_payload(payload).await
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

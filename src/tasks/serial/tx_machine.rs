use defmt::info;
use embassy_nrf::{peripherals::UARTE0, uarte::UarteTx};
use postcard::to_slice_cobs;

use super::{CommPacket, UartError, TX_PACKET_CHANNEL};

async fn send_packet(tx: &mut UarteTx<'_, UARTE0>, packet: CommPacket) -> Result<(), UartError> {
    let mut buf = [0u8; 200];
    let tx_buf = to_slice_cobs(&packet, &mut buf).unwrap();

    tx.write(tx_buf).await.map_err(|_| UartError::TxError)?;

    Ok(())
}

pub async fn tx_state_machine(mut tx: UarteTx<'_, UARTE0>) {
    // I think I should have a state machine in each task.
    // For now, just this.
    loop {
        let packet = TX_PACKET_CHANNEL.recv().await;
        // info!("Sending packet with ID: {:?}", packet.id);
        send_packet(&mut tx, packet).await.unwrap();
    }
}

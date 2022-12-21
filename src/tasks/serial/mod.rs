use core::ops::DerefMut;

use defmt::info;
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::interrupt;
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::peripherals;
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::uarte;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;
use futures::future::join;
use futures::pin_mut;
use heapless::Vec;
use postcard::{from_bytes_cobs, to_slice_cobs};

use crate::types::CommError;
use crate::types::CommPacket;
use crate::RX_CHANNEL;
use crate::TX_CHANNEL;

#[derive(Debug)]
enum UartError {
    GenericRxError,
    RxBufferFull,
    DecodeError,
    TxError,
}

/// Reads until a 0x00 is found.
async fn read_cobs_frame(rx: &mut UarteRx<'_, UARTE0>) -> Result<Vec<u8, 200>, UartError> {
    let mut buf = [0u8; 1];
    let mut cobs_frame: Vec<u8, 200> = Vec::new();
    loop {
        rx.read(&mut buf)
            .await
            .map_err(|_| UartError::GenericRxError)?;
        cobs_frame
            .push(buf[0])
            .map_err(|_| UartError::GenericRxError)?;

        if buf[0] == 0x00 {
            // Done.
            return Ok(cobs_frame);
        }
        if cobs_frame.is_full() {
            return Err(UartError::RxBufferFull);
        }
    }
}

/// Waits for a COBS-encoded packet on UART and tries to transform it into a CommPacket.
async fn recv_packet(rx: &mut UarteRx<'_, UARTE0>) -> Result<CommPacket, CommError> {
    let mut raw_data = read_cobs_frame(rx)
        .await
        .map_err(|_| CommError::PhysError)?;

    let rx_data = raw_data.deref_mut();
    let packet: CommPacket = from_bytes_cobs(rx_data).map_err(|_| CommError::MalformedPacket)?;
    Ok(packet)
}

/// Sends a COBS-encoded packet over UART.
async fn send_packet(tx: &mut UarteTx<'_, UARTE0>, packet: CommPacket) -> Result<(), UartError> {
    let mut buf = [0u8; 200];
    let tx_buf = to_slice_cobs(&packet, &mut buf).unwrap();

    tx.write(tx_buf).await.map_err(|_| UartError::TxError)?;

    Ok(())
}

pub async fn rx_task(mut rx: UarteRx<'_, UARTE0>) {
    loop {
        let packet = recv_packet(&mut rx).await;
        RX_CHANNEL.immediate_publisher().publish_immediate(packet);
    }
}

pub async fn tx_task(mut tx: UarteTx<'_, UARTE0>) {
    let mut subscriber = TX_CHANNEL.subscriber().unwrap();
    loop {
        let packet_to_send = subscriber.next_message_pure().await;
        let _ = send_packet(&mut tx, packet_to_send).await;
    }
}

pub async fn serial_task(
    instance: &mut peripherals::UARTE0,
    pin_tx: &mut AnyPin,
    pin_rx: &mut AnyPin,
) {
    info!("UART task started!");
    // UART-related
    let uart_irq = interrupt::take!(UARTE0_UART0);
    uart_irq.set_priority(interrupt::Priority::P7);
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;

    let uart = uarte::Uarte::new(instance, uart_irq, pin_rx, pin_tx, config);
    let (tx, rx) = uart.split();

    let rx_fut = rx_task(rx);
    let tx_fut = tx_task(tx);
    pin_mut!(rx_fut);
    pin_mut!(tx_fut);

    join(rx_fut, tx_fut).await;
}

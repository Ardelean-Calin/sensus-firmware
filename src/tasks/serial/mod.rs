use core::ops::DerefMut;

use defmt::info;
use defmt::warn;
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::interrupt;
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::peripherals;
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::uarte;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;
use embassy_time::Duration;
use embassy_time::Timer;
use futures::future::join;
use futures::future::select;
use futures::pin_mut;
use heapless::Vec;
use postcard::{from_bytes_cobs, to_slice_cobs};

use crate::types::CommError;
use crate::types::CommPacket;
use crate::PacketDispatcher;
use crate::CTRL_CHANNEL;
use crate::DISPATCHER;
use crate::RX_CHANNEL;

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
///
/// * `rx` - Mutable reference to a UarteRx peripheral.
/// * `timeout` - Duration after which a timeout error will be reported.
async fn recv_packet(
    rx: &mut UarteRx<'_, UARTE0>,
    timeout: Duration,
) -> Result<CommPacket, CommError> {
    let timeout_fut = Timer::after(timeout);
    pin_mut!(timeout_fut);
    let rx_fut = read_cobs_frame(rx);
    pin_mut!(rx_fut);

    let mut raw_data = match select(rx_fut, timeout_fut).await {
        futures::future::Either::Left((data_result, _)) => {
            data_result.map_err(|_| CommError::PhysError)
        }
        futures::future::Either::Right(_) => {
            warn!("UART RX timeout!");
            Err(CommError::Timeout)
        }
    }?;

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
    let (mut tx, mut rx) = uart.split();

    loop {
        match DISPATCHER.await_command().await {
            crate::DispatcherCommand::Receive(timeout) => {
                info!("Waiting for receiving...");
                let packet = recv_packet(&mut rx, timeout).await;
                RX_CHANNEL.immediate_publisher().publish_immediate(packet);
            }
            crate::DispatcherCommand::Send(packet) => {
                let _ = send_packet(&mut tx, packet).await;
            }
        }
    }
}

use core::ops::DerefMut;

use defmt::warn;
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;
use embassy_time::Duration;
use embassy_time::Timer;
use futures::future::select;
use futures::pin_mut;
use heapless::Vec;
use postcard::{from_bytes_cobs, to_slice_cobs};

use crate::types::CommError;
use crate::types::CommPacket;

pub mod tasks;
pub(crate) use tasks::serial_task;

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

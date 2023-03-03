use defmt::info;
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;
use heapless::Vec;
use postcard::to_slice_cobs;

use crate::types::Error;
use crate::types::Packet;
use crate::types::RawPacket;

pub mod tasks;

#[derive(Debug)]
enum UartError {
    GenericRxError,
    RxBufferFull,
    DecodeError,
    TxError,
}

/// Reads until a 0x00 is found.
async fn read_cobs_frame(rx: &mut UarteRx<'_, UARTE0>) -> Result<Vec<u8, 256>, Error> {
    let mut buf = [0u8; 1];
    let mut cobs_frame: Vec<u8, 256> = Vec::new();
    loop {
        rx.read(&mut buf).await.map_err(|_| Error::UartRxError)?;
        cobs_frame.push(buf[0]).map_err(|_| Error::UartBufferFull)?;
        if buf[0] == 0x00 {
            // Got end-of-frame.
            return Ok(cobs_frame);
        }
    }
}

/// Waits for a COBS-encoded packet on UART and tries to transform it into a CommPacket.
///
/// * `rx` - Mutable reference to a UarteRx peripheral.
/// * `timeout` - TODO Duration after which a timeout error will be reported.
async fn recv_packet(rx: &mut UarteRx<'_, UARTE0>) -> Result<RawPacket, Error> {
    let mut raw_data = read_cobs_frame(rx).await?;

    let size = cobs::decode_in_place(&mut raw_data).map_err(|_| Error::CobsDecodeError)?;

    // Also checks CRC
    let packet = Packet::from_slice(&raw_data[..size])?;

    Ok(packet.raw)
}

/// Sends a COBS-encoded packet over UART.
async fn send_packet(tx: &mut UarteTx<'_, UARTE0>, packet: RawPacket) -> Result<(), Error> {
    let mut buf = [0u8; 32];
    let tx_buf = to_slice_cobs(&packet, &mut buf).expect("COBS encoding error.");

    info!("Sending uart packet...");
    tx.write(tx_buf).await.map_err(|_| Error::UartTxError)?;

    Ok(())
}

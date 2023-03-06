use defmt::trace;
use embassy_nrf::uarte;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;
use heapless::Vec;
use postcard::to_slice_cobs;

use crate::types::Error;
use crate::types::Packet;
use crate::types::RawPacket;

pub mod tasks;

/// Initializes the UART peripheral with our default config.
fn serial_init<'d, T>(
    instance: &'d mut T,
    pin_tx: &'d mut embassy_nrf::gpio::AnyPin,
    pin_rx: &'d mut embassy_nrf::gpio::AnyPin,
    uart_irq: &'d mut impl embassy_nrf::Peripheral<P = T::Interrupt>,
) -> (UarteTx<'d, T>, UarteRx<'d, T>)
where
    T: embassy_nrf::uarte::Instance,
{
    // UART-related
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD460800;

    let uart = uarte::Uarte::new(instance, uart_irq, pin_rx, pin_tx, config);

    // Return the two Rx and Tx instances
    uart.split()
}

/// Reads until a 0x00 is found.
async fn read_cobs_frame<T>(rx: &mut UarteRx<'_, T>) -> Result<Vec<u8, 256>, Error>
where
    T: embassy_nrf::uarte::Instance,
{
    let mut buf = [0u8; 1];
    let mut cobs_frame: Vec<u8, 256> = Vec::new();
    loop {
        rx.read(&mut buf).await.map_err(|_| Error::UartRx)?;
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
async fn recv_packet<T>(rx: &mut UarteRx<'_, T>) -> Result<RawPacket, Error>
where
    T: embassy_nrf::uarte::Instance,
{
    let mut raw_data = read_cobs_frame(rx).await?;

    let size = cobs::decode_in_place(&mut raw_data).map_err(|_| Error::CobsDecodeError)?;

    // Also checks CRC
    let packet = Packet::from_slice(&raw_data[..size])?;

    Ok(packet.raw)
}

/// Sends a COBS-encoded packet over UART.
async fn send_packet<T>(tx: &mut UarteTx<'_, T>, packet: RawPacket) -> Result<(), Error>
where
    T: embassy_nrf::uarte::Instance,
{
    let mut buf = [0u8; 32];
    let tx_buf = to_slice_cobs(&packet, &mut buf).expect("COBS encoding error.");

    trace!("Sending uart packet...");
    tx.write(tx_buf).await.map_err(|_| Error::UartTx)?;

    Ok(())
}

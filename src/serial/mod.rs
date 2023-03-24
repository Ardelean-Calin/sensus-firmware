use crc::{Crc, CRC_16_GSM};
use embassy_nrf::uarte;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;
use heapless::Vec;
use postcard::from_bytes_cobs;
use postcard::to_slice_cobs;
use postcard::to_vec;

use crate::types::CommPacket;
use crate::types::CommPacketType;
use crate::types::CommResponse;
use crate::types::PacketError;
use crate::types::UartError;

pub mod tasks;

pub const CRC_GSM: Crc<u16> = Crc::<u16>::new(&CRC_16_GSM);

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
async fn read_cobs_frame<T>(rx: &mut UarteRx<'_, T>) -> Result<Vec<u8, 256>, UartError>
where
    T: embassy_nrf::uarte::Instance,
{
    let mut buf = [0u8; 1];
    let mut cobs_frame: Vec<u8, 256> = Vec::new();
    loop {
        rx.read(&mut buf).await.map_err(|_| UartError::UartRx)?;
        cobs_frame
            .push(buf[0])
            .map_err(|_| UartError::UartBufferFull)?;
        if buf[0] == 0x00 {
            // Got end-of-frame.
            return Ok(cobs_frame);
        }
    }
}

fn calculate_checksum(content: &CommPacketType) -> Result<u16, PacketError> {
    let serialized: Vec<u8, 64> = to_vec(content).map_err(|_| PacketError::PacketCRC)?;
    let crc = CRC_GSM.checksum(serialized.as_slice());
    Ok(crc)
}

/// Waits for a COBS-encoded packet on UART and tries to transform it into a CommPacket.
///
/// * `rx` - Mutable reference to a UarteRx peripheral.
/// * `timeout` - TODO Duration after which a timeout error will be reported.
async fn recv_packet<T>(rx: &mut UarteRx<'_, T>) -> Result<CommPacket, PacketError>
where
    T: embassy_nrf::uarte::Instance,
{
    let mut raw_data = read_cobs_frame(rx)
        .await
        .map_err(|_| PacketError::PhysError)?;

    let packet: CommPacket =
        from_bytes_cobs(&mut raw_data).map_err(|_| PacketError::DeserializationError)?;

    // Extract the checksum and check if it's a fine checksum
    let checksum = packet.crc;
    let actual_checksum = calculate_checksum(&packet.payload)?;
    if checksum != actual_checksum {
        defmt::error!("Checksum error");
        return Err(PacketError::PacketCRC);
    }

    Ok(packet)
}

/// Sends a COBS-encoded packet over UART.
async fn send_response<T>(tx: &mut UarteTx<'_, T>, response: CommResponse) -> Result<(), UartError>
where
    T: embassy_nrf::uarte::Instance,
{
    let mut buf = [0u8; 32];
    let tx_buf = to_slice_cobs(&response, &mut buf).expect("COBS encoding error.");

    tx.write(tx_buf).await.map_err(|_| UartError::UartTx)?;

    Ok(())
}

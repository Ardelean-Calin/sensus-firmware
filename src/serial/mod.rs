mod types;

use crc::{Crc, CRC_16_GSM};
use embassy_nrf::bind_interrupts;
use embassy_nrf::interrupt::Binding;
use embassy_nrf::peripherals;
use embassy_nrf::uarte;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;
use heapless::Vec;
use postcard::from_bytes_cobs;
use postcard::to_slice_cobs;
use postcard::to_vec;

use crate::comm_manager::types::CommPacket;
use crate::comm_manager::types::CommPacketType;
use crate::comm_manager::types::CommResponse;
use crate::comm_manager::types::PacketError;

use types::UartError;

pub mod tasks;

pub const CRC_GSM: Crc<u16> = Crc::<u16>::new(&CRC_16_GSM);

bind_interrupts!(struct UartIrqs {
    UARTE0_UART0 => uarte::InterruptHandler<peripherals::UARTE0>;
});

/// Initializes the UART peripheral with our default config.
fn serial_init<'d, T>(
    instance: &'d mut T,
    pin_tx: &'d mut embassy_nrf::gpio::AnyPin,
    pin_rx: &'d mut embassy_nrf::gpio::AnyPin,
) -> (UarteTx<'d, T>, UarteRx<'d, T>)
where
    T: embassy_nrf::uarte::Instance,
    UartIrqs: Binding<
        <T as embassy_nrf::uarte::Instance>::Interrupt,
        embassy_nrf::uarte::InterruptHandler<T>,
    >,
{
    // UART-related
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD460800;

    let uart = uarte::Uarte::new(instance, UartIrqs, pin_rx, pin_tx, config);

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
    // TODO. The 256 byte limit should not be hard-coded. It should depend on the size of the structure
    let serialized: Vec<u8, 256> = to_vec(content).map_err(|_| PacketError::PacketCRC)?;
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
    let mut buf = [0u8; 64]; // 64 bytes should be enough to encode any reply of ours.
    let tx_buf = to_slice_cobs(&response, &mut buf).expect("COBS encoding error.");

    tx.write(tx_buf).await.map_err(|_| UartError::UartTx)?;

    Ok(())
}

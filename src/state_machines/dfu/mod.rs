pub mod types;

use defmt::error;
use defmt::info;

use defmt::warn;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_sync::pubsub::DynPublisher;
use embassy_sync::pubsub::DynSubscriber;
use embassy_time::with_timeout;
use embassy_time::Duration;
use nrf_softdevice::Flash;

use crate::types::Error;
use crate::types::RawPacket;
use crate::types::RX_BUS;
use crate::types::TX_BUS;

use self::types::DfuState;
use self::types::DfuStateMachine;
use self::types::Page;

/// Flashes a page to the DFU partition. Note, this function does not alter the active partition, only the DFU partition.
///
/// * `page` - The page to be flashed. 4096 bytes in size.
/// * `offset` - Page offset in the DFU partition. Should go from 0 to binary size.
/// * `updater` - A FirmwareUpdater instance.
/// * `flash` - AsyncNorFlash-compatible flash driver.
async fn flash_page(page: &Page, offset: usize, updater: &mut FirmwareUpdater, flash: &mut Flash) {
    // self.page_buffer.resize(4096, 0x00u8).unwrap();
    // Flashes the received page.
    updater
        .write_firmware(offset, page.data.as_slice(), flash, page.length())
        .await
        .unwrap();
}

async fn tick<'a>(
    sm: &'a mut DfuStateMachine,
    flash: &'a mut Flash,
    rx_subscriber: &'a mut DynSubscriber<'static, Result<RawPacket, Error>>,
    tx_publisher: &'a DynPublisher<'static, RawPacket>,
    updater: &mut FirmwareUpdater,
) -> Result<(), Error> {
    match sm.state {
        DfuState::Idle => {
            // Wait for start of DFU
            if let Ok(RawPacket::RecvDfuStart(dfu_header)) = rx_subscriber.next_message_pure().await
            {
                info!("Got the following binary size:");
                info!("  binary size: {:#04x}", dfu_header.binary_size);
                sm.binary_size = dfu_header.binary_size as usize;
                // Send OK message back.
                tx_publisher.publish(RawPacket::RespOK).await;
                // Wait for the first frame.
                sm.state = DfuState::NextFrame;
            }
        }
        DfuState::NextFrame => {
            with_timeout(Duration::from_millis(100), async {
                let res = match rx_subscriber.next_message_pure().await {
                    Ok(RawPacket::RecvDfuBlock(block_data)) => {
                        info!("Received frame {:?}", block_data.counter);
                        if block_data.counter == sm.frame_counter {
                            sm.page
                                .data
                                .extend_from_slice(&block_data.data)
                                .expect("Page full. Is the block size a divisor of 4096?");

                            if sm.page.is_full() {
                                sm.state = DfuState::FlashPage;
                            } else {
                                tx_publisher.publish(RawPacket::RespOK).await;
                            }
                            sm.frame_counter += 1;

                            Ok(())
                        } else {
                            info!("{:?}\t{:?}", block_data.counter, sm.frame_counter);
                            // Communication error. Aborting DFU.
                            Err(Error::DfuCounterError)
                        }
                    }
                    // This packet was not for us or an error, ignore. We will timeout if this repeats.
                    _ => Ok(()),
                };

                res
            })
            .await
            .map_err(|_| Error::DfuTimeout)
            .flatten()?;
        }
        DfuState::FlashPage => {
            flash_page(&sm.page, sm.page_offset, updater, flash).await;
            sm.page.clear();
            sm.page_offset += 4096;

            if sm.page_offset >= sm.binary_size {
                // DFU Done.
                sm.state = DfuState::Done;
            } else {
                tx_publisher.publish(RawPacket::RespOK).await;
                sm.state = DfuState::NextFrame;
            }
        }
        DfuState::Done => {
            // Notify user then restart system.
            warn!("DFU Done. Resetting...");
            tx_publisher.publish(RawPacket::RespDfuDone).await;
            // Mark the firmware as updated and reset!
            let mut magic = [0; 4];
            updater.mark_updated(flash, &mut magic).await.unwrap();
            // Reset microcontroller.
            cortex_m::peripheral::SCB::sys_reset();

            // I should not get here.
        }
    };

    Ok(())
}

pub(crate) async fn run(mut flash: Flash) {
    let mut updater = FirmwareUpdater::default();
    let mut rx_subscriber = RX_BUS
        .dyn_subscriber()
        .expect("Failed to acquire subscriber.");
    let tx_publisher = TX_BUS
        .dyn_publisher()
        .expect("Failed to acquire publisher.");

    let mut sm = DfuStateMachine::new();
    let mut error_counter: usize = 0;
    loop {
        let result = tick(
            &mut sm,
            &mut flash,
            &mut rx_subscriber,
            &tx_publisher,
            &mut updater,
        )
        .await;

        match result {
            Ok(_) => {
                error_counter = 0;
            }
            Err(e) => match e {
                Error::DfuPacketDecode(err) => error!("Error decoding packet. {:?}", err),
                Error::DfuPacketCRC | Error::DfuTimeout => {
                    // CRC incorrect or timeout. Retry.
                    error_counter += 1;
                }
                Error::DfuCounterError => {
                    // Non-recoverable error...
                    tx_publisher.publish_immediate(RawPacket::RespDfuFailed);
                    error_counter = usize::MAX;
                }
                _ => error!("Unexpected error in DFU module: {:?}", e),
            },
        };

        // Depending on how many errors we had, we do different stuff.
        match error_counter {
            0 => {}
            1..=3 => {
                tx_publisher.publish_immediate(RawPacket::RespNOK);
            }
            _ => {
                error!("DFU Error. Aborting DFU.");
                sm = DfuStateMachine::new();
            }
        };
    }
}

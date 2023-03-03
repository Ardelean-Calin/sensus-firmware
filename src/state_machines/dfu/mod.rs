pub mod types;

use defmt::error;
use defmt::info;

use defmt::warn;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_time::with_timeout;
use embassy_time::Duration;
use heapless::Vec;
use nrf_softdevice::Flash;

use crate::state_machines::dfu::types::DfuError;
use crate::types::RawPacket;
use crate::types::RX_BUS;
use crate::types::TX_BUS;

use self::types::DfuState;
use self::types::DfuStateMachine;

#[derive(Clone, Default)]
struct Page {
    data: Vec<u8, 4096>,
}

impl Page {
    fn empty() -> Self {
        Self { data: Vec::new() }
    }

    fn length(&self) -> usize {
        self.data.len()
    }

    fn is_full(&self) -> bool {
        self.data.is_full()
    }

    fn clear(&mut self) {
        self.data.clear();
    }
}

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

pub(crate) async fn run(mut flash: Flash) {
    let mut page = Page::empty();
    let mut updater = FirmwareUpdater::default();
    let mut rx_subscriber = RX_BUS.subscriber().expect("Failed to acquire subscriber.");
    let tx_publisher = TX_BUS.publisher().expect("Failed to acquire publisher.");

    let mut sm = DfuStateMachine::new();
    loop {
        match sm.state {
            DfuState::Idle => {
                // Wait for start of DFU
                if let Ok(RawPacket::RecvDfuStart(dfu_header)) =
                    rx_subscriber.next_message_pure().await
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
                let res = with_timeout(Duration::from_millis(100), async {
                    if let Ok(RawPacket::RecvDfuBlock(block_data)) =
                        rx_subscriber.next_message_pure().await
                    {
                        info!("Received frame {:?}", block_data.counter);
                        if block_data.counter == sm.frame_counter {
                            page.data
                                .extend_from_slice(&block_data.data)
                                .expect("Page full. Is the block size a divisor of 4096?");

                            if page.is_full() {
                                sm.state = DfuState::FlashPage;
                            } else {
                                tx_publisher.publish(RawPacket::RespOK).await;
                            }
                            sm.frame_counter += 1;
                        } else {
                            info!("{:?}\t{:?}", block_data.counter, sm.frame_counter);
                            // Communication error. Aborting DFU.
                            sm.state = DfuState::Error(DfuError::FrameCounterError)
                        }
                    } else {
                        // In case of error, send NOK, causing frame to repeat.
                        warn!("Error receiving DFU block. Retrying... 1");
                        tx_publisher.publish(RawPacket::RespNOK).await;
                    }
                })
                .await;

                // In case of timeout receiving next frame, ask for a frame repetition.
                match res {
                    Ok(_) => {}
                    Err(_) => {
                        warn!("Timeout error. Retrying... 1");
                        tx_publisher.publish(RawPacket::RespNOK).await;
                    }
                }
            }
            DfuState::FlashPage => {
                flash_page(&page, sm.page_offset, &mut updater, &mut flash).await;
                page.clear();
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
                updater.mark_updated(&mut flash, &mut magic).await.unwrap();
                cortex_m::peripheral::SCB::sys_reset();

                // I should not get here.
            }
            DfuState::Error(_err) => {
                error!("Error during DFU. Frame counter out of sync.");
                // Reset the State Machine.
                sm = DfuStateMachine::new();
            }
        }
    }
}

pub mod types;

use defmt::error;
use defmt::info;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_time::with_timeout;
use embassy_time::Duration;
use embassy_time::Timer;

use super::types::DfuError;
use super::types::DfuPayload;
use super::types::Page;

use crate::comm_manager::types::DfuResponse;
use crate::FIRMWARE_VERSION;
use crate::FLASH_DRIVER;

use self::types::DfuSmState;
use self::types::DfuStateMachine;

use super::INPUT_SIG;
use super::OUTPUT_SIG;

impl DfuStateMachine {
    fn new() -> Self {
        DfuStateMachine {
            frame_counter: 0,
            binary_size: 0,
            state: DfuSmState::Idle,
        }
    }
}

/// Runs the DFU State Machine in an infinite loop.
pub async fn run() {
    let mut sm = DfuStateMachine::new();
    let mut updater = FirmwareUpdater::default();
    let mut page = Page::new();
    loop {
        match sm.state {
            DfuSmState::Idle => {
                match INPUT_SIG.wait().await {
                    DfuPayload::Header(h) => {
                        info!("Got the following binary size:");
                        info!("  binary size: {:#04x}", h.binary_size);
                        // Reset the global page buffer when receiving a new start-of-dfu.
                        page.reset();

                        sm.binary_size = h.binary_size as usize;
                        // Wait for the first frame.
                        sm.state = DfuSmState::WaitBlock;
                        // Send an OK.
                        OUTPUT_SIG.signal(Ok(DfuResponse::NextBlock));
                    }
                    DfuPayload::RequestFwVersion => {
                        sm = DfuStateMachine::new();
                        let mut magic = [0; 4];

                        // If we got queried for the firmware version, we are clearly booted and at least the DFU
                        // seems to be working.
                        let mut f = FLASH_DRIVER.lock().await;
                        let flash_ref = f.as_mut().unwrap();
                        let _ = updater.mark_booted(flash_ref, &mut magic).await;

                        OUTPUT_SIG.signal(Ok(DfuResponse::FirmwareVersion(FIRMWARE_VERSION)))
                    }
                    _ => {
                        sm.state = DfuSmState::Error(DfuError::StateMachineError);
                    }
                };
            }
            DfuSmState::WaitBlock => {
                // If we wait for a block, then we have a 1000 millisecond-long "active" section.
                // This is so we don't remain stuck in DFU mode in case of no communication.
                match with_timeout(Duration::from_millis(1000), INPUT_SIG.wait()).await {
                    Ok(payload) => {
                        if let DfuPayload::Block(b) = payload {
                            if b.counter == sm.frame_counter {
                                page.data
                                    .extend_from_slice(&b.data)
                                    .expect("Page full. Is the block size a divisor of 4096?");
                                sm.frame_counter += 1;

                                if page.is_full() {
                                    sm.state = DfuSmState::FlashPage;
                                } else {
                                    // Do not change state and request the next frame.
                                    OUTPUT_SIG.signal(Ok(DfuResponse::NextBlock));
                                }
                            } else {
                                error!("{:?}\t{:?}", b.counter, sm.frame_counter);
                                sm.state = DfuSmState::Error(DfuError::CounterError);
                            }
                        } else {
                            sm.state = DfuSmState::Error(DfuError::UnexpectedFrame);
                        }
                    }
                    Err(_t) => {
                        // DFU Timeout error!
                        sm.state = DfuSmState::Error(DfuError::TimeoutError);
                    }
                }
            }
            DfuSmState::FlashPage => {
                let mut f = FLASH_DRIVER.lock().await;
                let flash_ref = f.as_mut().unwrap();
                // Flashes the filled page.
                updater
                    .write_firmware(page.offset, page.data.as_slice(), flash_ref, page.length())
                    .await
                    .unwrap();

                // Increments offset with 4096 and clears data.
                page.increment_page();
                if page.offset >= sm.binary_size {
                    // DFU Done.
                    sm.state = DfuSmState::Done;
                } else {
                    OUTPUT_SIG.signal(Ok(DfuResponse::NextBlock));
                    sm.state = DfuSmState::WaitBlock;
                }
            }
            DfuSmState::Done => {
                OUTPUT_SIG.signal(Ok(DfuResponse::DfuDone));
                // Will cause a reset.
                info!("DFU Done! Resetting in 3 seconds...");
                Timer::after(Duration::from_secs(3)).await;
                // Mark the firmware as updated and reset!
                let mut f = FLASH_DRIVER.lock().await;
                let flash_ref = f.as_mut().unwrap();
                let mut magic = [0; 4];
                updater.mark_updated(flash_ref, &mut magic).await.unwrap();
                // Reset microcontroller.
                cortex_m::peripheral::SCB::sys_reset();
            }
            DfuSmState::Error(e) => {
                error!("DFU Error: {:?}", e);
                OUTPUT_SIG.signal(Err(e));
                sm = DfuStateMachine::new();
                // Just for not flooding in case of CTRL-C
                Timer::after(Duration::from_millis(100)).await;
            }
        }
    }
}

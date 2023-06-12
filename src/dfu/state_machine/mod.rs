pub mod types;

use defmt::error;
use defmt::info;
use defmt::warn;
use embassy_boot_nrf::AlignedBuffer;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_sync::pubsub::DynPublisher;
use embassy_time::with_timeout;
use embassy_time::Duration;
use embassy_time::Timer;

use super::types::DfuError;
use super::types::DfuPayload;
use super::types::Page;

use crate::comm_manager::types::CommResponse;
use crate::comm_manager::types::DfuResponse;
use crate::globals::TX_BUS;
use crate::FIRMWARE_VERSION;
use crate::FLASH_DRIVER;

use self::types::DfuSmState;
use self::types::DfuStateMachine;

use super::PAYLOAD_PROVIDER;

const RETRY_COUNT: usize = 3;

impl DfuStateMachine {
    fn new() -> Self {
        DfuStateMachine {
            current_block: 0,
            total_no_blocks: 0,
            binary_size: 0,
            state: DfuSmState::Waiting,
        }
    }
}

async fn send_response_ok(queue: &DynPublisher<'_, CommResponse>, response: DfuResponse) {
    queue
        .publish(crate::comm_manager::types::CommResponse::Ok(
            crate::comm_manager::types::ResponseTypeOk::Dfu(response),
        ))
        .await;
}

async fn send_response_err(queue: &DynPublisher<'_, CommResponse>, response: DfuError) {
    queue
        .publish(crate::comm_manager::types::CommResponse::Err(
            crate::comm_manager::types::ResponseTypeErr::Dfu(response),
        ))
        .await;
}

/// Runs the DFU State Machine in an infinite loop.
pub async fn run() -> ! {
    let mut sm = DfuStateMachine::new();
    let mut updater = FirmwareUpdater::default();
    let mut page = Page::new();
    let mut retry_counter = 0;
    let data_tx = TX_BUS
        .dyn_publisher()
        .expect("Failed to acquire publisher.");
    loop {
        match sm.state {
            DfuSmState::Waiting => {
                match PAYLOAD_PROVIDER.wait().await {
                    DfuPayload::StartDfu(header) => {
                        info!("Got the following DFU Header:");
                        info!("  binary size: {:#04x}", header.binary_size);
                        info!("  no_of_blocks: {:#04}", header.no_blocks);
                        // Reset the global page buffer when receiving a new start-of-dfu.
                        page.reset();

                        sm.binary_size = header.binary_size as usize;
                        sm.total_no_blocks = header.no_blocks;
                        // Wait for the first frame.
                        sm.state = DfuSmState::RequestBlock;
                    }
                    DfuPayload::RequestFwVersion => {
                        sm = DfuStateMachine::new();
                        send_response_ok(&data_tx, DfuResponse::FirmwareVersion(FIRMWARE_VERSION))
                            .await;
                    }
                    _ => {
                        sm.state = DfuSmState::Error(DfuError::StateMachineError);
                    }
                };
            }
            DfuSmState::RequestBlock => {
                // This state times out after three attempts to request a block.
                let res = with_timeout(Duration::from_millis(100), async {
                    send_response_ok(
                        &data_tx,
                        DfuResponse::RequestBlock(sm.current_block.to_le_bytes()),
                    )
                    .await;
                    if let DfuPayload::Block(block) = PAYLOAD_PROVIDER.wait().await {
                        retry_counter = 0;
                        sm.state = DfuSmState::ProcessBlock(block);
                    };
                })
                .await;

                if res.is_err() {
                    retry_counter += 1;
                }

                if retry_counter >= RETRY_COUNT {
                    sm.state = DfuSmState::Error(DfuError::TimeoutError);
                }
            }
            DfuSmState::ProcessBlock(block) => {
                // Process the block, and at the end increment the block index.
                page.data
                    .extend_from_slice(&block.data)
                    .expect("Page full. Is the block size a divisor of 4096?");

                if page.is_full() {
                    let mut f = FLASH_DRIVER.lock().await;
                    let flash_ref = defmt::unwrap!(f.as_mut());
                    // Flashes the filled page.
                    updater
                        .write_firmware(page.offset, page.data.as_slice(), flash_ref, page.length())
                        .await
                        .unwrap();

                    // Increments offset with 4096 and clears data.
                    page.data.clear();
                    page.offset += 4096;
                }

                sm.current_block += 1;
                if sm.current_block == sm.total_no_blocks {
                    sm.state = DfuSmState::Done;
                } else {
                    sm.state = DfuSmState::RequestBlock;
                }
            }
            DfuSmState::Done => {
                send_response_ok(&data_tx, DfuResponse::DfuDone).await;
                // Will cause a reset.
                info!("DFU Done! Resetting...");
                Timer::after(Duration::from_secs(1)).await;
                // Mark the firmware as updated and reset!
                let mut f = FLASH_DRIVER.lock().await;
                let flash_ref = defmt::unwrap!(f.as_mut());
                let mut magic = AlignedBuffer([0u8; 4]);
                updater
                    .mark_updated(flash_ref, magic.as_mut())
                    .await
                    .unwrap();
                // Reset microcontroller.
                cortex_m::peripheral::SCB::sys_reset();
            }
            DfuSmState::Error(e) => {
                match e {
                    DfuError::StateMachineError => {
                        error!("DFU State Machine error. Maybe counter not ok?")
                    }
                    DfuError::TimeoutError => warn!("DFU Timeout. Resetting state machine."),
                }
                send_response_err(&data_tx, e).await;
                sm = DfuStateMachine::new();
            }
        }
    }
}

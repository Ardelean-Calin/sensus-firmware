pub mod types;

use defmt::error;
use defmt::info;
use defmt::trace;
use embassy_time::with_timeout;
use embassy_time::Duration;
use embassy_time::Timer;

use super::types::DfuError;
use super::types::DfuPayload;

use crate::globals::DFU_SIG_DONE;
use crate::globals::{DFU_SIG_FLASHED, DFU_SIG_NEW_PAGE, GLOBAL_PAGE};
use crate::types::CommResponse;
use crate::types::DfuOkType;
use crate::types::ResponseTypeErr;
use crate::types::ResponseTypeOk;

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

    async fn process_dfu_payload(&mut self, payload: DfuPayload) {
        if let DfuPayload::Block(b) = payload {
            trace!("Received frame {:?}", b.counter);
            if b.counter == self.frame_counter {
                let mut page = GLOBAL_PAGE.lock().await;
                page.data
                    .extend_from_slice(&b.data)
                    .expect("Page full. Is the block size a divisor of 4096?");

                if page.is_full() {
                    // We need to free the mutex.
                    core::mem::drop(page);

                    DFU_SIG_NEW_PAGE.signal(true);
                    DFU_SIG_FLASHED.wait().await;

                    let mut page = GLOBAL_PAGE.lock().await;
                    page.clear_data();
                    page.offset += 4096;

                    if page.offset >= self.binary_size {
                        // DFU Done.
                        self.state = DfuSmState::Done;
                        // continue;
                    } else {
                        self.state = DfuSmState::WaitBlock;
                    }
                }

                self.frame_counter += 1;
                OUTPUT_SIG.signal(CommResponse::OK(ResponseTypeOk::Dfu(DfuOkType::NextFrame)));
            } else {
                error!("{:?}\t{:?}", b.counter, self.frame_counter);
                self.state = DfuSmState::Error(DfuError::CounterError);
            }
        }
    }
}

pub async fn run() {
    let mut sm = DfuStateMachine::new();
    loop {
        match sm.state {
            DfuSmState::Idle => {
                match INPUT_SIG.wait().await {
                    DfuPayload::Header(h) => {
                        info!("Got the following binary size:");
                        info!("  binary size: {:#04x}", h.binary_size);
                        sm.binary_size = h.binary_size as usize;
                        // Wait for the first frame.
                        sm.state = DfuSmState::WaitBlock;
                        // Send an OK.
                        OUTPUT_SIG
                            .signal(CommResponse::OK(ResponseTypeOk::Dfu(DfuOkType::NextFrame)));
                    }
                    DfuPayload::RequestFwVersion => {
                        sm.state = DfuSmState::Idle;
                        OUTPUT_SIG.signal(CommResponse::OK(ResponseTypeOk::Dfu(
                            DfuOkType::FirmwareVersion([0xAA, 0xBB, 0xCC, 0xDD, 0xCA, 0xFE]),
                        )))
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
                    Ok(payload) => sm.process_dfu_payload(payload).await,
                    Err(_t) => {
                        // DFU Timeout error!
                        sm.state = DfuSmState::Error(DfuError::TimeoutError);
                    }
                }
            }
            DfuSmState::Done => {
                info!("DFU DONE!");
                OUTPUT_SIG.signal(CommResponse::OK(ResponseTypeOk::Dfu(DfuOkType::DfuDone)));
                // Will cause a reset.
                DFU_SIG_DONE.signal(true);
                sm.state = DfuSmState::Idle;
            }
            DfuSmState::Error(e) => {
                error!("DFU Error: {:?}", e);
                sm.state = DfuSmState::Idle;
                OUTPUT_SIG.signal(CommResponse::NOK(ResponseTypeErr::Dfu(e)));
                // Just for not flooding in case of CTRL-C
                Timer::after(Duration::from_millis(100)).await;
            }
        }
    }
}

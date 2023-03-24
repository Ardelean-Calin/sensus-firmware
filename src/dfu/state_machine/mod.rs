pub mod types;

use defmt::error;
use defmt::info;
use defmt::trace;

use crate::globals::DFU_SIG_DONE;
use crate::globals::{DFU_SIG_FLASHED, DFU_SIG_NEW_PAGE, GLOBAL_PAGE};
use crate::types::CommResponse;
use crate::types::DfuError;
use crate::types::DfuOkType;
use crate::types::DfuPayload;
use crate::types::ResponseTypeErr;
use crate::types::ResponseTypeOk;

use self::types::DfuState;
use self::types::DfuStateMachine;

use super::INPUT_SIG;
use super::OUTPUT_SIG;

pub async fn run() {
    let mut sm = DfuStateMachine::new();
    loop {
        match sm.state {
            DfuState::Idle => {
                match INPUT_SIG.wait().await {
                    DfuPayload::Header(h) => {
                        info!("Got the following binary size:");
                        info!("  binary size: {:#04x}", h.binary_size);
                        sm.binary_size = h.binary_size as usize;
                        // Wait for the first frame.
                        sm.state = DfuState::WaitBlock;
                        // Send an OK.
                        OUTPUT_SIG
                            .signal(CommResponse::OK(ResponseTypeOk::Dfu(DfuOkType::NextFrame)));
                    }
                    DfuPayload::RequestFwVersion => {
                        sm.state = DfuState::Idle;
                        OUTPUT_SIG.signal(CommResponse::OK(ResponseTypeOk::Dfu(
                            DfuOkType::FirmwareVersion([0xAA, 0xBB, 0xCC, 0xDD, 0xCA, 0xFE]),
                        )))
                    }
                    _ => {
                        sm.state = DfuState::Error(DfuError::StateMachineError);
                    }
                };
            }
            DfuState::WaitBlock => {
                if let DfuPayload::Block(b) = INPUT_SIG.wait().await {
                    trace!("Received frame {:?}", b.counter);
                    if b.counter == sm.frame_counter {
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

                            if page.offset >= sm.binary_size {
                                // DFU Done.
                                sm.state = DfuState::Done;
                                continue;
                            } else {
                                sm.state = DfuState::WaitBlock;
                            }
                        }

                        sm.frame_counter += 1;
                        OUTPUT_SIG
                            .signal(CommResponse::OK(ResponseTypeOk::Dfu(DfuOkType::NextFrame)));
                    } else {
                        error!("{:?}\t{:?}", b.counter, sm.frame_counter);
                        sm.state = DfuState::Error(DfuError::CounterError);
                    }
                }
            }
            DfuState::Done => {
                info!("DFU DONE!");
                OUTPUT_SIG.signal(CommResponse::OK(ResponseTypeOk::Dfu(DfuOkType::DfuDone)));
                // Will cause a reset.
                DFU_SIG_DONE.signal(true);
                sm.state = DfuState::Idle;
            }
            DfuState::Error(e) => {
                sm.state = DfuState::Idle;
                OUTPUT_SIG.signal(CommResponse::NOK(ResponseTypeErr::Dfu(e)));
            }
        }
    }
}

pub mod types;

use defmt::error;
use defmt::info;

use defmt::trace;
use defmt::warn;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::pubsub::DynPublisher;
use embassy_sync::pubsub::DynSubscriber;
use embassy_sync::signal::Signal;
use embassy_time::with_timeout;
use embassy_time::Duration;
use embassy_time::Timer;
use nrf_softdevice::Flash;

use crate::tasks::FLASH_BUS;
use crate::types::DfuHeader;
use crate::types::Error;
use crate::types::RawPacket;

use self::types::DfuState;
use self::types::DfuStateMachine;
use self::types::Page;

impl DfuStateMachine {
    pub fn new() -> Self {
        DfuStateMachine {
            frame_counter: 0,
            binary_size: 0,
            page: Page::empty(),
            state: DfuState::Idle,
        }
    }

    pub fn init(&mut self, header: DfuHeader) {
        info!("Got the following binary size:");
        info!("  binary size: {:#04x}", header.binary_size);
        self.binary_size = header.binary_size as usize;
        // Wait for the first frame.
        self.state = DfuState::NextFrame;
    }

    pub async fn tick(&mut self, packet: RawPacket) -> Result<DfuState, Error> {
        match self.state {
            DfuState::Idle => {
                error!("DFU StateMachine should not tick while IDLE.");
                return Err(Error::DfuStateMachineError);
            }
            DfuState::NextFrame => {
                if let RawPacket::RecvDfuBlock(block_data) = packet {
                    trace!("Received frame {:?}", block_data.counter);
                    if block_data.counter == self.frame_counter {
                        self.page
                            .data
                            .extend_from_slice(&block_data.data)
                            .expect("Page full. Is the block size a divisor of 4096?");

                        if self.page.is_full() {
                            FLASH_BUS.signal(self.page.clone());
                            Timer::after(Duration::from_millis(250)).await;
                            self.page.clear_data();
                            self.page.offset += 4096;

                            if self.page.offset >= self.binary_size {
                                // DFU Done.
                                self.state = DfuState::Done;
                            } else {
                                self.state = DfuState::NextFrame;
                            }
                        }

                        self.frame_counter += 1;
                    } else {
                        error!("{:?}\t{:?}", block_data.counter, self.frame_counter);
                        // Communication error. Aborting DFU.
                        return Err(Error::DfuCounterError);
                    }
                } else {
                    // Got unexpected frame.
                    return Err(Error::DfuUnexpectedFrame);
                }
            }
            DfuState::Done => {
                error!("Should not have gotten here.");
            }
        };

        Ok(self.state.clone())
    }
}

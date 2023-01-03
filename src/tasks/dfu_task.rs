use defmt::{info, warn, Format};
use embassy_boot_nrf::FirmwareUpdater;
use embassy_time::Duration;
use heapless::Vec;
use nrf_softdevice::Flash;

use crate::{
    types::{CommError, CommPacket, PacketID},
    DISPATCHER,
};

#[derive(Format)]
enum DfuState {
    Idle,
    AwaitNoPages,
    AwaitNextPage,
    WaitFrames,
}

struct DfuStateMachine {
    current_state: DfuState,
    flash: Flash,
    page_buffer: Vec<u8, 4096>,
    no_of_pages: u8,
    no_of_frames: u8,
    page_offset: u32,
    updater: FirmwareUpdater,
}

impl DfuStateMachine {
    fn new(flash: Flash) -> DfuStateMachine {
        let page_buffer: Vec<u8, 4096> = Vec::new();
        let current_state = DfuState::Idle;
        let no_of_pages = 0u8;
        let no_of_frames = 0u8;
        let page_offset = 0u32;

        // Firmware update related
        let updater = FirmwareUpdater::default();

        DfuStateMachine {
            current_state,
            page_buffer,
            no_of_pages,
            no_of_frames,
            page_offset,
            updater,
            flash,
        }
    }

    async fn run(&mut self) -> Result<(), CommError> {
        info!("Current state: {:?}", self.current_state);
        match self.current_state {
            DfuState::Idle => {
                let packet = DISPATCHER.receive_with_timeout(None).await?;
                if packet.is(PacketID::DFU_START) {
                    info!("DFU started...");
                    self.current_state = DfuState::AwaitNoPages;
                    // First, request the number of remaining pages.
                    self.req_remaining_no_pages().await;
                };
                Ok(())
            }
            DfuState::AwaitNoPages => {
                let packet = DISPATCHER
                    .receive_with_timeout(Some(Duration::from_millis(100)))
                    .await?;

                if packet.is(PacketID::DFU_NO_PAGES) {
                    // This is the number of remaining pages.
                    self.no_of_pages = packet.data[0];
                    self.req_next_page().await;
                    self.current_state = DfuState::AwaitNextPage;
                };
                Ok(())
            }
            DfuState::AwaitNextPage => {
                let packet = DISPATCHER
                    .receive_with_timeout(Some(Duration::from_millis(100)))
                    .await?;

                if packet.is(PacketID::DFU_NO_FRAMES) {
                    self.no_of_frames = packet.data[0];
                    // If we still have to process more frames.
                    self.req_next_frame().await;
                    self.current_state = DfuState::WaitFrames;
                };
                Ok(())
            }
            DfuState::WaitFrames => {
                let packet = DISPATCHER
                    .receive_with_timeout(Some(Duration::from_millis(100)))
                    .await?;

                if packet.is(PacketID::DFU_FRAME) {
                    self.page_buffer.extend(packet.data);

                    self.no_of_frames -= 1;
                    if self.no_of_frames == 0 {
                        // We finished receiving all the frames for this page.
                        self.flash_page().await;
                    } else {
                        // Request the next frame.
                        self.req_next_frame().await;
                        self.current_state = DfuState::WaitFrames; // remain unchanged
                    }
                };
                Ok(())
            }
        }
    }

    async fn send_packet(&self, id: PacketID) {
        let packet = CommPacket {
            id,
            data: Vec::new(),
        };
        DISPATCHER.send_packet(packet).await;
    }

    async fn req_remaining_no_pages(&self) {
        self.send_packet(PacketID::REQ_NO_PAGES).await;
    }

    /// Requests the next page to be processed or the first page if it's the first one.
    async fn req_next_page(&self) {
        self.send_packet(PacketID::REQ_NEXT_PAGE).await;
    }

    async fn req_next_frame(&self) {
        self.send_packet(PacketID::REQ_NEXT_FRAME).await;
    }

    async fn mark_dfu_done(&self) {
        self.send_packet(PacketID::DFU_DONE).await;
    }

    async fn flash_page(&mut self) {
        self.page_buffer.resize(4096, 0x00u8).unwrap();
        // Flashes the received page.
        self.updater
            .write_firmware(
                self.page_offset as usize,
                &self.page_buffer,
                &mut self.flash,
                4096,
            )
            .await
            .unwrap();
        // Decrements the page number.
        self.page_buffer.clear();
        self.page_offset += 4096;
        // Then goes back to requesting the next page.
        self.no_of_pages -= 1;
        if self.no_of_pages != 0 {
            self.req_next_page().await;
            self.current_state = DfuState::AwaitNextPage;
        } else {
            self.dfu_done().await;
        }
    }

    async fn dfu_done(&mut self) {
        info!("DFU Done! Resetting...");
        self.mark_dfu_done().await;
        // Mark the firmware as updated and reset!
        let mut magic = [0; 4];
        self.updater
            .mark_updated(&mut self.flash, &mut magic)
            .await
            .unwrap();
        cortex_m::peripheral::SCB::sys_reset();
    }
}

#[embassy_executor::task]
pub async fn dfu_task(flash: Flash) {
    info!("DFU task started.");
    let mut state_machine = DfuStateMachine::new(flash);
    let mut err_cnt = 0u8;

    loop {
        let result = state_machine.run().await;
        match result {
            Ok(_) => {
                err_cnt = 0;
            }
            Err(_) => {
                err_cnt += 1;
                warn!("Error while DFU. Error counter: {:?}", err_cnt);
                DISPATCHER.send_packet(CommPacket::retry()).await;
            }
        };
    }
}

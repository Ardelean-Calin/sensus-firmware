use defmt::{info, Format};
use embassy_boot_nrf::FirmwareUpdater;
use heapless::Vec;
use nrf_softdevice::Flash;

use crate::{
    types::{CommPacket, PacketID},
    RX_CHANNEL, TX_CHANNEL,
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

    fn send_packet(&self, id: PacketID) {
        let packet = CommPacket {
            id,
            data: Vec::new(),
        };
        TX_CHANNEL.immediate_publisher().publish_immediate(packet);
    }

    fn req_remaining_no_pages(&self) {
        self.send_packet(PacketID::REQ_NO_PAGES);
    }

    /// Requests the next page to be processed or the first page if it's the first one.
    fn req_next_page(&self) {
        self.send_packet(PacketID::REQ_NEXT_PAGE);
    }

    fn req_next_frame(&self) {
        self.send_packet(PacketID::REQ_NEXT_FRAME);
    }

    fn mark_dfu_done(&self) {
        self.send_packet(PacketID::DFU_DONE);
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
            self.req_next_page();
            self.current_state = DfuState::AwaitNextPage;
        } else {
            self.dfu_done().await;
        }
    }

    async fn dfu_done(&mut self) {
        info!("DFU Done! Resetting...");
        self.mark_dfu_done();
        // Mark the firmware as updated and reset!
        let mut magic = [0; 4];
        self.updater
            .mark_updated(&mut self.flash, &mut magic)
            .await
            .unwrap();
        cortex_m::peripheral::SCB::sys_reset();
    }

    /// Ticks the state machine.
    async fn tick(&mut self, packet: CommPacket) {
        match self.current_state {
            DfuState::Idle => {
                if packet.is(PacketID::DFU_START) {
                    info!("DFU started...");
                    self.current_state = DfuState::AwaitNoPages;
                    // First, request the number of remaining pages.
                    self.req_remaining_no_pages();
                }
            }
            DfuState::AwaitNoPages => {
                if packet.is(PacketID::DFU_NO_PAGES) {
                    // This is the number of remaining pages.
                    self.no_of_pages = packet.data[0];
                    self.req_next_page();
                    self.current_state = DfuState::AwaitNextPage;
                }
            }
            DfuState::AwaitNextPage => {
                if packet.is(PacketID::DFU_NO_FRAMES) {
                    self.no_of_frames = packet.data[0];
                    // If we still have to process more frames.
                    self.req_next_frame();
                    self.current_state = DfuState::WaitFrames;
                }
            }
            DfuState::WaitFrames => {
                if packet.is(PacketID::DFU_FRAME) {
                    self.page_buffer.extend(packet.data);

                    self.no_of_frames -= 1;
                    if self.no_of_frames == 0 {
                        // We finished receiving all the frames for this page.
                        self.flash_page().await;
                    } else {
                        // Request the next frame.
                        self.req_next_frame();
                        self.current_state = DfuState::WaitFrames; // remain unchanged
                    }
                }
            }
        }
    }
}

#[embassy_executor::task]
pub async fn dfu_task(flash: Flash) {
    info!("DFU task started.");
    let mut sub = RX_CHANNEL.subscriber().unwrap();
    let mut state_machine = DfuStateMachine::new(flash);
    loop {
        let packet = sub.next_message_pure().await;
        match packet {
            Ok(comm_packet) => state_machine.tick(comm_packet).await,
            Err(_) => {
                // Send a packet retry request.
                TX_CHANNEL
                    .immediate_publisher()
                    .publish_immediate(CommPacket::retry());
            }
        }
    }
}

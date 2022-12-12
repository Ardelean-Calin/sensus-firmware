use core::ops::DerefMut;

use defmt::{info, Format};
use embassy_boot_nrf::FirmwareUpdater;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_nrf::{
    nvmc::Nvmc,
    peripherals::{NVMC, UARTE0},
    uarte::UarteRx,
};
use heapless::Vec;
use postcard::from_bytes_cobs;

use super::{CommPacket, PacketID, UartError, TX_PACKET_CHANNEL};

enum State {
    Idle,
    StreamingData,
    AwaitNoPages,
    NextPage,
    AwaitNoFrames,
    NextFrame,
    WaitFrame,
    FlashPage,
    DfuDone,
}

/// Reads until a 0x00 is found.
async fn read_cobs_frame(rx: &mut UarteRx<'_, UARTE0>) -> Result<Vec<u8, 200>, UartError> {
    let mut buf = [0u8; 1];
    let mut cobs_frame: Vec<u8, 200> = Vec::new();
    loop {
        rx.read(&mut buf).await.unwrap();
        cobs_frame.push(buf[0]).unwrap();
        if buf[0] == 0x00 {
            // Done.
            return Ok(cobs_frame);
        }
        if cobs_frame.is_full() {
            return Err(UartError::RxBufferFull);
        }
    }
}

/// Waits for a COBS-encoded packet on UART and tries to transform it into a CommPacket.
async fn recv_packet(rx: &mut UarteRx<'_, UARTE0>) -> Result<CommPacket, UartError> {
    loop {
        let mut raw_data = read_cobs_frame(rx).await?;
        let rx_data = raw_data.deref_mut();
        match from_bytes_cobs(rx_data).map_err(|_| UartError::DecodeError) {
            Ok(packet) => return Ok(packet),
            Err(_) => {
                send_response(PacketID::REQ_RETRY).await;
            }
        }
    }
}

/// Sends a COBS-encoded response over UART.
async fn send_response(response: PacketID) {
    let packet = CommPacket {
        id: response,
        data: Vec::new(),
    };
    TX_PACKET_CHANNEL.send(packet).await;
}

pub async fn rx_state_machine(mut rx: UarteRx<'_, UARTE0>, flash: &mut nrf_softdevice::Flash) {
    let mut page_buffer: Vec<u8, 4096> = Vec::new();
    let mut current_state = State::Idle;
    let mut no_of_pages = 0u8;
    let mut no_of_frames = 0u8;
    let mut page_offset = 0u32;
    // Firmware update related
    let mut updater = FirmwareUpdater::default();

    // TODO: Lots of problems with this state machine...I am not handling errors properly, for once.
    loop {
        match current_state {
            State::Idle => {
                // Wait for a packet
                let packet = recv_packet(&mut rx).await.unwrap();
                match packet.id {
                    PacketID::STREAM_START => {
                        // TODO: Send a signal to the TX state machine. It should know
                        // to also switch to streaming.
                        current_state = State::StreamingData;
                    }
                    PacketID::DFU_START => {
                        send_response(PacketID::REQ_NO_PAGES).await;
                        current_state = State::AwaitNoPages;
                        info!("Received DFU start. Wating number of pages.")
                    }
                    e => {
                        panic!(
                            "Only DFU Start and STREAM START are valid commands here. {:?}",
                            e
                        )
                    }
                }
            }
            State::AwaitNoPages => {
                let packet = recv_packet(&mut rx).await.unwrap();
                if let PacketID::DFU_NO_PAGES = packet.id {
                    no_of_pages = packet.data[0];
                    current_state = State::NextPage;
                    info!(
                        "Received number of pages: {:?} Waiting for first page.",
                        &no_of_pages
                    );
                } else {
                    panic!("Invalid packet ID: {:?}", packet.id);
                }
            }
            State::NextPage => {
                // Process page. Sends a command to the PC, which in turn returns the frame number.
                if no_of_pages == 0 {
                    current_state = State::DfuDone;
                } else {
                    // Ask for the next page and the number of frames in said page.
                    send_response(PacketID::REQ_NEXT_PAGE).await;
                    current_state = State::AwaitNoFrames;
                }
            }
            State::AwaitNoFrames => {
                // We then wait to receive the page frame number.
                let packet = recv_packet(&mut rx).await.unwrap();
                if let PacketID::DFU_NO_FRAMES = packet.id {
                    no_of_frames = packet.data[0];
                    current_state = State::NextFrame;
                    info!("Number of frames: {:?}", no_of_frames);
                } else {
                    panic!("Invalid packet ID: {:?}", packet.id);
                }
            }
            State::NextFrame => {
                // A frame was requested
                if no_of_frames == 0 {
                    // We finished receiving all the frames for this page.
                    current_state = State::FlashPage;
                } else {
                    // Request the next frame.
                    send_response(PacketID::REQ_NEXT_FRAME).await;
                    current_state = State::WaitFrame;
                }
            }
            State::WaitFrame => {
                let packet = recv_packet(&mut rx).await.unwrap();
                if let PacketID::DFU_FRAME = packet.id {
                    // Store the received frame in the data vector.
                    page_buffer.extend(packet.data);

                    no_of_frames -= 1;
                    current_state = State::NextFrame;
                } else {
                    panic!("Invalid packet ID: {:?}", packet.id);
                }
            }
            State::FlashPage => {
                page_buffer.resize(4096, 0x00u8).unwrap();
                // Flashes the received page.
                updater
                    .write_firmware(page_offset as usize, &page_buffer, flash, 4096)
                    .await
                    .unwrap();
                // Decrements the page number.
                page_buffer.clear();
                no_of_pages -= 1;
                page_offset += 4096;
                // Then goes back to requesting the next page.
                current_state = State::NextPage;
            }
            State::DfuDone => {
                info!("DFU Done! Resetting...");
                send_response(PacketID::DFU_DONE).await;
                // Mark the firmware as updated and reset!
                let mut magic = [0; 4];
                updater.mark_updated(flash, &mut magic).await.unwrap();
                cortex_m::peripheral::SCB::sys_reset();
            }
            State::StreamingData => {
                // Wait for a packet
                let packet = recv_packet(&mut rx).await.unwrap();
                if let PacketID::STREAM_STOP = packet.id {
                    // TODO: Send a signal to the TX state machine. It should know
                    // to also switch off streaming.
                    current_state = State::Idle;
                }
            }
        }
    }
}

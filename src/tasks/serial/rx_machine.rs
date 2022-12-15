use core::ops::DerefMut;

use defmt::{info, warn};
use embassy_boot_nrf::FirmwareUpdater;
use embassy_nrf::{peripherals::UARTE0, uarte::UarteRx};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use futures::{future::select, pin_mut};
use heapless::Vec;
use postcard::from_bytes_cobs;

use super::{CommPacket, PacketID, UartError, TX_PACKET_CHANNEL};

enum State {
    Start,
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

pub async fn dfu_sm(mut rx: &mut UarteRx<'_, UARTE0>, flash: &mut nrf_softdevice::Flash) {
    let mut page_buffer: Vec<u8, 4096> = Vec::new();
    let mut current_state = State::Start;
    let mut no_of_pages = 0u8;
    let mut no_of_frames = 0u8;
    let mut page_offset = 0u32;
    // Firmware update related
    let mut updater = FirmwareUpdater::default();
    loop {
        TIMEOUT_CHANNEL.send(1).await;
        match current_state {
            State::Start => {
                send_response(PacketID::REQ_NO_PAGES).await;
                // Create a timeout...
                current_state = State::AwaitNoPages;
                info!("Received DFU start. Wating number of pages.")
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
                    current_state = State::Start;
                }
            }
        }
    }
}

static TIMEOUT_CHANNEL: Channel<ThreadModeRawMutex, u8, 1> = Channel::new();

async fn dfu_sm_timout() {
    loop {
        let timeout_fut = Timer::after(Duration::from_millis(1000)); // I need a longer timeout because flashing will raise a DropBomb error if not...
        let new_activity_fut = TIMEOUT_CHANNEL.recv();

        pin_mut!(timeout_fut);
        pin_mut!(new_activity_fut);

        match select(timeout_fut, new_activity_fut).await {
            futures::future::Either::Left(_) => return,
            futures::future::Either::Right(_) => {}
        }
    }
}

pub async fn rx_state_machine(mut rx: UarteRx<'_, UARTE0>, flash: &mut nrf_softdevice::Flash) {
    // TODO: Lots of problems with this state machine...I am not handling errors properly, for once.
    loop {
        // Wait for a packet
        let packet = recv_packet(&mut rx).await.unwrap();
        match packet.id {
            PacketID::STREAM_START => {
                // TODO: Send a signal to the TX state machine. It should know
                // to also switch to streaming.
            }
            PacketID::DFU_START => {
                let dfu_fut = dfu_sm(&mut rx, flash);
                pin_mut!(dfu_fut);
                // Timeout the DFU after 30 seconds...
                let dfu_timeout_fut = dfu_sm_timout();
                pin_mut!(dfu_timeout_fut);

                match select(dfu_timeout_fut, dfu_fut).await {
                    futures::future::Either::Left(_) => {
                        warn!("A timeout occurred while DFU-ing...");
                    }
                    futures::future::Either::Right(_) => info!("DFU successful! Restarting..."),
                }
            }
            e => {
                panic!(
                    "Only DFU Start and STREAM START are valid commands here. {:?}",
                    e
                )
            }
        }
    }
}

use defmt::info;
use defmt::Format;
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::interrupt;
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::peripherals;
use embassy_nrf::uarte;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use futures::future::join;
use futures::pin_mut;
use serde::Deserialize;
use serde::Serialize;

mod rx_machine;
mod tx_machine;

#[allow(non_camel_case_types)]
#[derive(Format, Serialize, Deserialize, Clone, Debug)]
enum PacketID {
    STREAM_START = 0x31,   // Starts data streaming via UART
    STREAM_STOP = 0x32,    // Stops data streaming via UART
    DFU_START = 0x33,      // Represents the start of a dfu operation
    REQ_NO_PAGES = 0x34,   // Represents a request for the number of pages
    DFU_NO_PAGES = 0x35,   // The received number of pages.
    REQ_NEXT_PAGE = 0x36, // Indicates to the updated to prepare the next page. Updater will send the number of transfers required for this page.
    DFU_NO_FRAMES = 0x37, // The number of frames in the requested page
    REQ_NEXT_FRAME = 0x38, // Uploader, please give me the next 128-byte frame.
    DFU_FRAME = 0x39,     // This is how we represent a DFU frame.
    DFU_DONE = 0x3A,      // Sent by us to mark that the DFU is done.
    REQ_RETRY = 0xFE,     // Retry sending the last frame.
    ERROR = 0xFF,         // Represents an error
}

#[derive(Serialize, Deserialize, Clone)]
struct CommPacket {
    id: PacketID,
    data: heapless::Vec<u8, 128>,
}

#[derive(Debug)]
enum UartError {
    RxBufferFull,
    DecodeError,
    TxError,
}

static TX_PACKET_CHANNEL: Channel<ThreadModeRawMutex, CommPacket, 1> = Channel::new();

pub async fn serial_task(
    instance: &mut peripherals::UARTE0,
    pin_tx: &mut AnyPin,
    pin_rx: &mut AnyPin,
    nvmc: &mut embassy_nrf::peripherals::NVMC,
) {
    info!("UART task started!");
    // UART-related
    let uart_irq = interrupt::take!(UARTE0_UART0);
    uart_irq.set_priority(interrupt::Priority::P7);
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;

    let uart = uarte::Uarte::new(instance, uart_irq, pin_rx, pin_tx, config);
    let (tx, rx) = uart.split();

    let rx_fut = rx_machine::rx_state_machine(rx, nvmc);
    let tx_fut = tx_machine::tx_state_machine(tx);
    pin_mut!(rx_fut);
    pin_mut!(tx_fut);

    join(rx_fut, tx_fut).await;
}

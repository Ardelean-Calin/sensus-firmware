//! Contains a coroutine that keeps sending received data via UART to a connected PC.

use defmt::info;
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::interrupt;
use embassy_nrf::peripherals;
use embassy_nrf::uarte;

use crate::app::SENSOR_DATA_BUS;

pub async fn serial_pusher(
    instance: &mut peripherals::UARTE0,
    pin_tx: &mut AnyPin,
    pin_rx: &mut AnyPin,
) {
    let mut subscriber = SENSOR_DATA_BUS.subscriber().unwrap();
    // UART-related
    let uart_irq = interrupt::take!(UARTE0_UART0);
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;

    info!("UART task started!");
    let mut uart = uarte::Uarte::new(instance, uart_irq, pin_rx, pin_tx, config);
    let mut buf = [0u8; 32]; // 32 bytes should be plenty for storing the encoded messages.
    loop {
        let data_packet = subscriber.next_message_pure().await;
        let used = postcard::to_slice_cobs(&data_packet.to_bytes_array(), &mut buf).unwrap();
        uart.write(used).await.unwrap();
    }
}

use defmt::info;

use embassy_nrf::gpio::AnyPin;
use embassy_nrf::interrupt;
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::peripherals;
use embassy_nrf::uarte;

use crate::DISPATCHER;
use crate::PLUGGED_DETECT;
use crate::RX_CHANNEL;

use super::{recv_packet, send_packet};

#[embassy_executor::task]
pub async fn serial_task(
    mut instance: peripherals::UARTE0,
    mut pin_tx: AnyPin,
    mut pin_rx: AnyPin,
) {
    info!("serial task created.");
    // Configure UART
    let mut uart_irq = interrupt::take!(UARTE0_UART0);
    uart_irq.set_priority(interrupt::Priority::P7);

    run_while_plugged_in!(PLUGGED_DETECT, async {
        defmt::warn!("UART task started!");
        // UART-related
        let mut config = uarte::Config::default();
        config.parity = uarte::Parity::EXCLUDED;
        config.baudrate = uarte::Baudrate::BAUD115200;

        let uart = uarte::Uarte::new(
            &mut instance,
            &mut uart_irq,
            &mut pin_rx,
            &mut pin_tx,
            config,
        );
        let (mut tx, mut rx) = uart.split();

        loop {
            match DISPATCHER.await_command().await {
                crate::DispatcherCommand::Receive(timeout) => {
                    info!("Waiting for receiving...");
                    let packet = recv_packet(&mut rx, timeout).await;
                    RX_CHANNEL.immediate_publisher().publish_immediate(packet);
                }
                crate::DispatcherCommand::Send(packet) => {
                    let _ = send_packet(&mut tx, packet).await;
                }
            }
        }
    })
    .await
}

use defmt::{error, info};

use crate::drivers::serial::serial_init;
use crate::globals::{RX_BUS, TX_BUS};
use crate::PLUGGED_DETECT;
use embassy_futures::join::join;
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::interrupt;
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::peripherals;
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::uarte::UarteRx;
use embassy_nrf::uarte::UarteTx;

use super::{recv_packet, send_response};

async fn uart_rx_task(rx: &mut UarteRx<'_, UARTE0>) {
    loop {
        let raw_packet_res = recv_packet(rx).await;
        RX_BUS
            .immediate_publisher()
            .publish_immediate(raw_packet_res);
    }
}

async fn uart_tx_task(tx: &mut UarteTx<'_, UARTE0>) {
    let mut subscriber = TX_BUS
        .subscriber()
        .expect("Error registering subscriber for TX_BUS.");
    loop {
        let packet = subscriber.next_message().await;
        match packet {
            embassy_sync::pubsub::WaitResult::Lagged(x) => {
                error!("Missed {:?} messages.", x);
            }
            embassy_sync::pubsub::WaitResult::Message(raw) => {
                // info!("Sending packet: {:?}", raw);
                send_response(tx, raw)
                    .await
                    .expect("Failed to send packet.");
            }
        }
    }
}

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

        let (mut tx, mut rx) = serial_init(&mut instance, &mut pin_tx, &mut pin_rx, &mut uart_irq);

        loop {
            join(uart_rx_task(&mut rx), uart_tx_task(&mut tx)).await;
        }
    })
    .await
}

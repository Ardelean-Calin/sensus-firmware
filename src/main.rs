#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]

#[path = "tasks/app.rs"]
mod app;

mod ble;

#[path = "../common.rs"]
mod common;

use embassy_executor::Spawner;
use nrf52832_pac as pac;

// Reconfigure UICR to enable reset pin if required (resets if changed).
pub fn configure_reset_pin() {
    let uicr = unsafe { &*pac::UICR::ptr() };
    let nvmc = unsafe { &*pac::NVMC::ptr() };

    #[cfg(feature = "nrf52840")]
    const RESET_PIN: u8 = 18;
    #[cfg(feature = "nrf52832")]
    const RESET_PIN: u8 = 21;

    // Sequence copied from Nordic SDK components/toolchain/system_nrf52.c
    if uicr.pselreset[0].read().connect().is_disconnected()
        || uicr.pselreset[1].read().connect().is_disconnected()
    {
        nvmc.config.write(|w| w.wen().wen());
        while nvmc.ready.read().ready().is_busy() {}

        for i in 0..=1 {
            uicr.pselreset[i].write(|w| {
                unsafe {
                    w.pin().bits(RESET_PIN);
                } // should be 21 for 52832

                #[cfg(feature = "nrf52840")]
                w.port().clear_bit(); // not present on 52832

                w.connect().connected();
                w
            });
            while nvmc.ready.read().ready().is_busy() {}
        }

        nvmc.config.write(|w| w.wen().ren());
        while nvmc.ready.read().ready().is_busy() {}

        cortex_m::peripheral::SCB::sys_reset();
    }
}

/// Reconfigure NFC pins to be regular GPIO pins (resets if changed).
/// It's a simple bit flag on LSb of the UICR register.
pub fn configure_nfc_pins_as_gpio() {
    let uicr = unsafe { &*pac::UICR::ptr() };
    let nvmc = unsafe { &*pac::NVMC::ptr() };

    // Sequence copied from Nordic SDK components/toolchain/system_nrf52.c line 173
    if uicr.nfcpins.read().protect().is_nfc() {
        nvmc.config.write(|w| w.wen().wen());
        while nvmc.ready.read().ready().is_busy() {}

        uicr.nfcpins.write(|w| w.protect().disabled());
        while nvmc.ready.read().ready().is_busy() {}

        nvmc.config.write(|w| w.wen().ren());
        while nvmc.ready.read().ready().is_busy() {}

        cortex_m::peripheral::SCB::sys_reset();
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Configure NFC pins as gpio.
    // configure_nfc_pins_as_gpio();
    // Configure Pin 21 as reset pin (for now)
    configure_reset_pin();

    // Enable the softdevice.
    let (sd, server) = ble::configure_ble();
    spawner.must_spawn(ble::softdevice_task(sd));

    // Main application task.
    let mut config = embassy_nrf::config::Config::default();
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P7;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
    config.lfclk_source = embassy_nrf::config::LfclkSource::InternalRC;

    // Peripherals config
    let p = embassy_nrf::init(config);
    spawner.must_spawn(app::application_task(p));

    // Should await forever.
    ble::run_ble_application(sd, server).await;
}

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(async_fn_in_trait)]

#[path = "tasks/app.rs"]
mod app;

mod ble;
mod error;
mod prelude;

use embassy_boot_nrf::FirmwareUpdater;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_executor::Spawner;
use embassy_nrf::nvmc::Nvmc;
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
    // Configure Pin 21 as reset pin. Only this pin can be used according
    // to datasheet.
    configure_reset_pin();

    // If we got here, it means that this application works. We indicate that to the bootloader.
    // TODO: I can make this more complex and, for example, only mark OK if I got a BLE connection...
    // As per tutorial, I need to make sure that:
    // Your application is running correctly before marking itself as successfully booted.
    // Doing this too early could cause your application to be stuck with the new faulty firmware.
    // For IoT connected devices, there is an additional trick: make sure you can connect to
    // the required services (such as the firmware update service) before marking the firmware
    // as successfully booted.
    // let mut updater = FirmwareUpdater::default();
    // updater.mark_booted(&mut flash).await;

    // Main application task.
    let mut config = embassy_nrf::config::Config::default();
    // NOTE: Do not enable Xtal. It is used by the S112. The SoftDevice powers the crystal
    //       on only when it needs it in order to transmit something. Then turns it off.
    //       If I enable it here, the crystal will always be on, drawing a significant
    //       amount of power!
    // config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
    config.lfclk_source = embassy_nrf::config::LfclkSource::InternalRC;
    let mut p = embassy_nrf::init(config);

    // let nvmc = Nvmc::new(&mut p.NVMC);
    // let mut flash = BlockingAsync::new(nvmc);
    // let mut updater = FirmwareUpdater::default();
    // let mut magic = [0; 4];
    // updater.mark_booted(&mut flash, &mut magic).await;

    // Enable the softdevice.
    let (sd, server) = ble::configure_ble();
    spawner.must_spawn(ble::softdevice_task(sd));

    // #[cfg(not(debug_assertions))]
    // if let Some(msg) = get_panic_message_bytes() {
    //    How about if I have some panic message, I turn on the red LED if plugged in?
    //     board.uart.write(msg);
    // }

    let flash = nrf_softdevice::Flash::take(sd);
    spawner.must_spawn(app::application_task(p, flash));

    // Should await forever.
    ble::run_ble_application(sd, &server).await;
}

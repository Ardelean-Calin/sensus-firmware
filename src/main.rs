#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(async_fn_in_trait)]

mod ble;
mod drivers;
mod error;
mod prelude;
mod tasks;
mod types;
use types::*;

mod custom_executor;

use cortex_m_rt::entry;
use defmt::info;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_executor::Spawner;
use embassy_nrf::{
    gpio::{AnyPin, Pin},
    nvmc::Nvmc,
    peripherals, saadc,
    wdt::Watchdog,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex, pubsub::PubSubChannel};
use futures::StreamExt;
use nrf52832_pac as pac;
use static_cell::StaticCell;

/// TODO: Maybe add an Error Channel, used to report errors from different modules?
/// We use channels to transmit data packets between modules.
/// A pub-sub transmit channel with 3 queue items, 2 subscribers and 1 publisher.
/// Sends data to the user
static TX_CHANNEL: PubSubChannel<ThreadModeRawMutex, CommPacket, 3, 2, 1> = PubSubChannel::new();
/// A pub-sub receive channel with 3 queue items, 2 subscribers and 1 publisher.
/// Received data from the user
static RX_CHANNEL: PubSubChannel<ThreadModeRawMutex, Result<CommPacket, CommError>, 3, 2, 1> =
    PubSubChannel::new();
/// DFU ongoing. Used to make sure I don't send data while DFU is taking place somewhere else.
static DFU_ONGOING: Mutex<ThreadModeRawMutex, u8> = Mutex::new(0);

/// I need to create a custom executor to patch some very specific hardware bugs found on nRF52832.
static EXECUTOR: StaticCell<custom_executor::Executor> = StaticCell::new();

struct RgbPins {
    pin_red: AnyPin,
    pin_green: AnyPin,
    pin_blue: AnyPin,
}

pub struct LowPowerPeripherals {
    pin_sda: AnyPin,
    pin_scl: AnyPin,
    pin_probe_en: AnyPin,
    pin_probe_detect: AnyPin,
    pin_adc: saadc::AnyInput,
    pin_freq_in: AnyPin,
    saadc: peripherals::SAADC,
    twim: peripherals::TWISPI0,
    gpiote_ch: peripherals::GPIOTE_CH0,
    ppi_ch: peripherals::PPI_CH0,
}

pub struct HighPowerPeripherals {
    pin_chg_detect: AnyPin,
    pin_plug_detect: AnyPin,
    pins_rgb: RgbPins,
    pwm_rgb: peripherals::PWM0,
    uart: peripherals::UARTE0,
    pin_uart_tx: AnyPin,
    pin_uart_rx: AnyPin,
}

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

#[embassy_executor::task]
async fn main_task() {
    let spawner = Spawner::for_current_executor().await;
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
    let nvmc = Nvmc::new(&mut p.NVMC);
    let mut flash = BlockingAsync::new(nvmc);
    let mut updater = FirmwareUpdater::default();
    let mut magic = [0; 4];
    updater.mark_booted(&mut flash, &mut magic).await;

    // Enable the softdevice.
    let (sd, server) = ble::configure_ble();
    spawner.must_spawn(ble::softdevice_task(sd));

    // // #[cfg(not(debug_assertions))]
    // // if let Some(msg) = get_panic_message_bytes() {
    // //    How about if I have some panic message, I turn on the red LED if plugged in?
    // //     board.uart.write(msg);
    // // }

    let flash = nrf_softdevice::Flash::take(sd);
    let lp_peripherals = LowPowerPeripherals {
        pin_sda: p.P0_14.degrade(),
        pin_scl: p.P0_15.degrade(),
        pin_probe_en: p.P0_06.degrade(),
        pin_probe_detect: p.P0_20.degrade(),
        pin_adc: p.P0_03.into(),
        pin_freq_in: p.P0_19.degrade(),
        saadc: p.SAADC,
        twim: p.TWISPI0,
        gpiote_ch: p.GPIOTE_CH0,
        ppi_ch: p.PPI_CH0,
    };

    let hp_peripherals = HighPowerPeripherals {
        pin_chg_detect: p.P0_29.degrade(),
        pin_plug_detect: p.P0_31.degrade(),
        pins_rgb: RgbPins {
            pin_red: p.P0_22.degrade(),
            pin_green: p.P0_23.degrade(),
            pin_blue: p.P0_24.degrade(),
        },
        pwm_rgb: p.PWM0,
        uart: p.UARTE0,
        pin_uart_tx: p.P0_26.degrade(),
        pin_uart_rx: p.P0_25.degrade(),
    };

    spawner.must_spawn(tasks::app::application_task(lp_peripherals, hp_peripherals));
    spawner.must_spawn(tasks::dfu_task::dfu_task(flash));
    spawner.must_spawn(watchdog_task(p.WDT));

    // Should await forever.
    ble::run_ble_application(sd, &server).await;
}

#[embassy_executor::task]
async fn watchdog_task(wdt: peripherals::WDT) {
    let mut wdt_config = embassy_nrf::wdt::Config::default();
    wdt_config.timeout_ticks = 32768 * 3; // 3 seconds
    wdt_config.run_during_sleep = true;
    wdt_config.run_during_debug_halt = false; // false so that we can see the panic message in debug mode.

    let (_wdt, [mut handle]) = match Watchdog::try_new(wdt, wdt_config) {
        Ok(x) => x,
        Err(_) => {
            info!("Watchdog already active with wrong config, waiting for it to timeout...");
            loop {}
        }
    };

    // Feed the watchdog every 1.5 second. If something happens, the watchdog will reset our microcontroller.
    let mut ticker = embassy_time::Ticker::every(embassy_time::Duration::from_millis(1500));
    loop {
        handle.pet();
        ticker.next().await;
    }
}

#[entry]
fn main() -> ! {
    info!("Booted successfully!");

    let executor = EXECUTOR.init(custom_executor::Executor::new());
    executor.run(|spawner| spawner.must_spawn(main_task()));
}

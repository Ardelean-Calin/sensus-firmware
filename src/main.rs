#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(result_flattening)]
#![feature(once_cell)]

// Needs to come before everything.
mod prelude;

mod ble;
mod comm_manager;
mod common;
mod config_manager;
mod dfu;
mod globals;
mod power_manager;
mod rgb;
mod sensors;
mod serial;
mod types;

mod custom_executor;

use cortex_m_rt::entry;
use defmt::info;
use embassy_executor::Spawner;
use embassy_nrf::{
    gpio::Pin, gpiote::Channel, peripherals, ppi::ConfigurableChannel, wdt::Watchdog,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use nrf_softdevice::Softdevice;
use static_cell::StaticCell;

// Static not const since static variables have a fixed location in memory.
static FIRMWARE_VERSION: &str = "1.0.0-rc";

/// Global access to a flash
static FLASH_DRIVER: Mutex<ThreadModeRawMutex, Option<nrf_softdevice::Flash>> = Mutex::new(None);

/// I need to create a custom executor to patch some very specific hardware bugs found on nRF52832.
static EXECUTOR: StaticCell<custom_executor::Executor> = StaticCell::new();

#[embassy_executor::task]
async fn main_task() {
    let spawner = Spawner::for_current_executor().await;
    // NOTE: You can configure the reset pin as gpio by using the cargo feature "reset-pin-as-gpio".
    //       Same can be done for the NFC pins: "nfc-pins-as-gpio"

    // Main application task.
    let mut config = embassy_nrf::config::Config::default();
    // NOTE: Do not enable Xtal. It is used by the S112. The SoftDevice powers the crystal
    //       on only when it needs it in order to transmit something. Then turns it off.
    //       If I enable it here, the crystal will always be on, drawing a significant
    //       amount of power!
    // config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P7;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P7;
    config.lfclk_source = embassy_nrf::config::LfclkSource::ExternalXtal;
    let p = embassy_nrf::init(config);

    // Print out FW Version (for debugging purposes)
    info!("Current FW version: {:?}", FIRMWARE_VERSION);
    // Enable the softdevice.
    let sd = ble::configure_ble();
    // And get the flash controller
    let flash = nrf_softdevice::Flash::take(sd);

    // Store the Flash in memory.
    let mut f = FLASH_DRIVER.lock().await;
    f.replace(flash);
    core::mem::drop(f);

    // After we initialized the Flash driver, we can load the config from Flash.
    config_manager::init().expect("Error initializing config manager.");

    // Spawn all the used tasks.
    // TODO: Only spawn the tasks AFTER configuration was loaded from nonvolatile memory.
    spawner.must_spawn(watchdog_task(p.WDT)); // This has to be the first one.

    spawner.must_spawn(softdevice_task(sd));
    spawner.must_spawn(power_manager::pwr_detect_task(p.P0_04.degrade()));

    // Onboard sensor aquisition task.
    let onboard_per = sensors::types::OnboardPeripherals {
        pin_sda: p.P0_06.degrade(),       // SDA
        pin_scl: p.P0_08.degrade(),       // SCL
        pin_interrupt: p.P0_07.degrade(), // INT
        instance_twim: p.TWISPI0,         // used I2C interface
        instance_saadc: p.SAADC,          // used SAADC
    };
    spawner.must_spawn(sensors::onboard_task(onboard_per));

    // Soil sensor aquisition task.
    let probe_per = sensors::types::ProbePeripherals {
        pin_probe_detect: p.P0_20.degrade(),     // probe_detect
        pin_probe_enable: p.P0_11.degrade(),     // probe_enable
        pin_probe_sda: p.P0_14.degrade(),        // SDA
        pin_probe_scl: p.P0_15.degrade(),        // SCL
        pin_probe_freq: p.P0_19.degrade(),       // probe_freq
        instance_twim: p.TWISPI1,                // used I2C interface
        instance_gpiote: p.GPIOTE_CH0.degrade(), // GPIOTE channel
        instance_ppi: p.PPI_CH0.degrade(),       // PPI channel
    };
    spawner.must_spawn(sensors::soil_task(probe_per));
    spawner.must_spawn(ble::payload_manager::payload_mgr_task());
    spawner.must_spawn(ble::ble_task());

    // This "task" can run all the time, since we want DFU to be available via Bluetooth, as
    // well.
    spawner.must_spawn(dfu::dfu_task());
    spawner.must_spawn(comm_manager::comm_task());

    // Will handle UART DFU and data logging over UART.
    spawner.must_spawn(serial::tasks::serial_task(
        p.UARTE0,
        p.P0_03.degrade(),
        p.P0_02.degrade(),
    ));

    spawner.must_spawn(rgb::rgb_task());
    // spawner.must_spawn(drivers::rgb::rgb_task(
    //     p.PWM0,
    //     p.P0_28.degrade(),
    //     p.P0_26.degrade(),
    //     p.P0_27.degrade(),
    // ));
    // TODO: move these tasks
    // spawner.must_spawn(drivers::rgb::tasks::rgb_task(
    //     p.PWM0,
    //     p.P0_28.degrade(),
    //     p.P0_26.degrade(),
    //     p.P0_27.degrade(),
    // ));

    // Should await forever.
    ble::coroutines::advertisment_loop(sd).await;
}

#[embassy_executor::task]
async fn watchdog_task(wdt: peripherals::WDT) {
    let mut wdt_config = embassy_nrf::wdt::Config::default();
    wdt_config.timeout_ticks = 32768 * 5; // 5 seconds
    wdt_config.run_during_sleep = true;
    wdt_config.run_during_debug_halt = false; // false so that we can see the panic message in debug mode.

    let (_wdt, [mut handle]) = match Watchdog::try_new(wdt, wdt_config) {
        Ok(x) => x,
        Err(_) => {
            panic!("Watchdog already active with wrong config, waiting for it to timeout...");
        }
    };

    // Feed the watchdog every 1.5 second. If something happens, the watchdog will reset our microcontroller.
    let mut ticker = embassy_time::Ticker::every(embassy_time::Duration::from_millis(1500));
    loop {
        handle.pet();
        ticker.next().await;
    }
}

#[embassy_executor::task]
pub async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[entry]
fn main() -> ! {
    info!("Booted successfully!");

    let executor = EXECUTOR.init(custom_executor::Executor::new());
    executor.run(|spawner| spawner.must_spawn(main_task()));
}

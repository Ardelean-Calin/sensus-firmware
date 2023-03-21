#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(result_flattening)]

// Needs to come before everything.
mod prelude;

mod common;
mod coroutines;
mod drivers;
mod error;
mod serial;
mod state_machines;
mod tasks;
mod types;

use embassy_futures::join::join;
use nrf_softdevice::Softdevice;

mod custom_executor;

use cortex_m_rt::entry;
use defmt::info;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_executor::Spawner;
use embassy_nrf::{
    gpio::{AnyPin, Input, Pin, Pull},
    gpiote::Channel,
    interrupt::{self, InterruptExt},
    peripherals,
    ppi::ConfigurableChannel,
    wdt::Watchdog,
};
use embassy_sync::{
    blocking_mutex::raw::ThreadModeRawMutex, pubsub::PubSubChannel, signal::Signal,
};
use futures::StreamExt;
use nrf52832_pac as pac;
use state_machines::dfu::types::Page;
use static_cell::StaticCell;

/// I need to create a custom executor to patch some very specific hardware bugs found on nRF52832.
static EXECUTOR: StaticCell<custom_executor::Executor> = StaticCell::new();

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

    // Enable the softdevice.
    let (sd, server) = drivers::ble::configure_ble();
    info!("My address: {:?}", nrf_softdevice::ble::get_address(sd));
    // And get the flash controller
    let mut flash = nrf_softdevice::Flash::take(sd);
    let mut updater = FirmwareUpdater::default();
    let mut magic = [0; 4];

    // TODO: Move to another place. Somewhere where if we got here, it is sure that the firmware is working.
    let _ = updater.mark_booted(&mut flash, &mut magic).await;

    // Spawn all the used tasks.
    // TODO: Only spawn the tasks AFTER configuration was loaded from nonvolatile memory.
    spawner.must_spawn(watchdog_task(p.WDT)); // This has to be the first one.

    spawner.must_spawn(softdevice_task(sd));
    spawner.must_spawn(power_state_task(p.P0_04.degrade()));

    // Onboard sensor aquisition task.
    let adc_irq = interrupt::take!(SAADC);
    adc_irq.set_priority(interrupt::Priority::P7);
    let i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    i2c_irq.set_priority(interrupt::Priority::P7);
    let onboard_per = drivers::onboard::types::OnboardPeripherals {
        pin_sda: p.P0_06.degrade(),       // SDA
        pin_scl: p.P0_08.degrade(),       // SCL
        pin_interrupt: p.P0_07.degrade(), // INT
        instance_twim: p.TWISPI0,         // used I2C interface
        instance_saadc: p.SAADC,          // used SAADC
        adc_irq,                          // used SAADC interrupt
        i2c_irq,                          // used I2c interrupt
    };
    spawner.must_spawn(tasks::onboard_task(onboard_per));

    // Soil sensor aquisition task.
    let i2c_irq = interrupt::take!(SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1);
    i2c_irq.set_priority(interrupt::Priority::P7);
    let probe_per = drivers::probe::types::ProbePeripherals {
        pin_probe_detect: p.P0_20.degrade(),     // probe_detect
        pin_probe_enable: p.P0_11.degrade(),     // probe_enable
        pin_probe_sda: p.P0_14.degrade(),        // SDA
        pin_probe_scl: p.P0_15.degrade(),        // SCL
        pin_probe_freq: p.P0_19.degrade(),       // probe_freq
        instance_twim: p.TWISPI1,                // used I2C interface
        instance_gpiote: p.GPIOTE_CH0.degrade(), // GPIOTE channel
        instance_ppi: p.PPI_CH0.degrade(),       // PPI channel
        i2c_irq,                                 // I2C interrupt
    };
    spawner.must_spawn(tasks::soil_task(probe_per));
    spawner.must_spawn(tasks::packet_manager_task());

    // This "task" can run all the time, since we want DFU to be available via Bluetooth, as
    // well.
    spawner.must_spawn(tasks::dfu_task(flash));
    spawner.must_spawn(tasks::comm_task());

    // Will handle UART DFU and data logging over UART.
    spawner.must_spawn(serial::tasks::serial_task(
        p.UARTE0,
        p.P0_03.degrade(),
        p.P0_02.degrade(),
    ));
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
    join(
        state_machines::ble::gatt_spawner(sd, server),
        state_machines::ble::run(),
    )
    .await;
}

struct PowerDetect {
    plugged_in: PubSubChannel<ThreadModeRawMutex, bool, 1, 2, 1>,
    plugged_out: PubSubChannel<ThreadModeRawMutex, bool, 1, 2, 1>,
}

static PLUGGED_DETECT: PowerDetect = PowerDetect {
    plugged_in: PubSubChannel::new(),
    plugged_out: PubSubChannel::new(),
};

#[embassy_executor::task]
async fn power_state_task(monitor_pin: AnyPin) {
    let mut plugged_detect = Input::new(monitor_pin, Pull::Down);
    loop {
        plugged_detect.wait_for_high().await;
        info!("Plugged in");
        PLUGGED_DETECT
            .plugged_in
            .publisher()
            .unwrap()
            .publish(true)
            .await;
        plugged_detect.wait_for_low().await;
        info!("Plugged out");
        PLUGGED_DETECT
            .plugged_out
            .publisher()
            .unwrap()
            .publish(true)
            .await;
    }
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

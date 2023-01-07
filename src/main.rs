#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]

mod ble;
mod error;
mod prelude;
mod rgb;
mod sensors;
mod serial;
mod tasks;
mod types;

use async_guard::AsyncGuard;
use embassy_time::{Duration, Timer};
use types::*;

mod custom_executor;

use cortex_m_rt::entry;
use defmt::info;
use embassy_boot_nrf::FirmwareUpdater;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_executor::Spawner;
use embassy_nrf::{
    gpio::{AnyPin, Input, Pin, Pull},
    interrupt::{self, InterruptExt},
    nvmc::Nvmc,
    peripherals,
    wdt::Watchdog,
};
use embassy_sync::{
    blocking_mutex::raw::ThreadModeRawMutex, channel::Channel, pubsub::PubSubChannel,
};
use futures::StreamExt;
use nrf52832_pac as pac;
use static_cell::StaticCell;

/// A pub-sub receive channel with 3 queue items, 2 subscribers and 1 publisher.
/// Received data from the user
static RX_CHANNEL: PubSubChannel<ThreadModeRawMutex, Result<CommPacket, CommError>, 3, 2, 1> =
    PubSubChannel::new();
/// Send commands to different parts of the program.
static CTRL_CHANNEL: PubSubChannel<ThreadModeRawMutex, DispatcherCommand, 5, 3, 1> =
    PubSubChannel::new();

static DISPATCHER: PacketDispatcher = PacketDispatcher {
    rx_channel: &RX_CHANNEL,
    ctrl_channel: &CTRL_CHANNEL,
};

static RGB_ROUTER: Channel<ThreadModeRawMutex, rgb::RGBTransition, 1> = Channel::new();

// This asynchronous guard monitors the state of the GPIO pin indicating
static POWER_DETECT: AsyncGuard = AsyncGuard::new();

#[derive(Clone)]
enum DispatcherCommand {
    Receive(Duration),
    Send(CommPacket),
}

struct PacketDispatcher {
    rx_channel: &'static PubSubChannel<ThreadModeRawMutex, Result<CommPacket, CommError>, 3, 2, 1>,
    ctrl_channel: &'static PubSubChannel<ThreadModeRawMutex, DispatcherCommand, 5, 3, 1>,
}

impl PacketDispatcher {
    async fn send_packet(&self, packet: CommPacket) {
        self.ctrl_channel
            .publisher()
            .unwrap()
            .publish(DispatcherCommand::Send(packet))
            .await;
    }

    async fn receive_with_timeout(
        &self,
        timeout: Option<Duration>,
    ) -> Result<CommPacket, CommError> {
        info!("Sent receive command");
        // Publish the command, then wait.
        self.ctrl_channel
            .publisher()
            .unwrap()
            .publish(DispatcherCommand::Receive(timeout.unwrap_or(Duration::MAX)))
            .await;

        let mut rx_channel = self.rx_channel.subscriber().unwrap();
        let packet = rx_channel.next_message_pure().await;

        packet
    }

    async fn await_command(&self) -> DispatcherCommand {
        info!("Awaiting command...");
        let command = self.ctrl_channel.subscriber().unwrap().next_message().await;
        match command {
            embassy_sync::pubsub::WaitResult::Lagged(_) => panic!("AAAAAAH"),
            embassy_sync::pubsub::WaitResult::Message(command) => info!("Received a command"),
        }

        DispatcherCommand::Receive(Duration::MAX)
        // command
    }
}

/// I need to create a custom executor to patch some very specific hardware bugs found on nRF52832.
static EXECUTOR: StaticCell<custom_executor::Executor> = StaticCell::new();

pub struct ApplicationPeripherals {
    pin_sda: AnyPin,
    pin_scl: AnyPin,
    pin_probe_en: AnyPin,
    pin_probe_detect: AnyPin,
    pin_freq_in: AnyPin,
    saadc: peripherals::SAADC,
    twim: peripherals::TWISPI0,
    gpiote_ch: peripherals::GPIOTE_CH0,
    ppi_ch: peripherals::PPI_CH0,
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

async fn my_task(params: u32) {
    let mut i: u32 = params;
    defmt::info!("Hey there!");
    loop {
        i += 1;
        Timer::after(Duration::from_millis(10)).await;
        defmt::info!("{:?}", i);
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
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
    config.lfclk_source = embassy_nrf::config::LfclkSource::InternalRC;
    let mut p = embassy_nrf::init(config);
    let nvmc = Nvmc::new(&mut p.NVMC);
    let mut flash = BlockingAsync::new(nvmc);
    let mut updater = FirmwareUpdater::default();
    let mut magic = [0; 4];
    // TODO: Move to another place. Somewhere where if we got here, it is sure that the firmware is working.
    let _ = updater.mark_booted(&mut flash, &mut magic).await;

    // These peripherals are always used.
    let app_peripherals = ApplicationPeripherals {
        pin_sda: p.P0_14.degrade(),
        pin_scl: p.P0_15.degrade(),
        pin_probe_en: p.P0_11.degrade(),
        pin_probe_detect: p.P0_20.degrade(),
        pin_freq_in: p.P0_19.degrade(),
        saadc: p.SAADC,
        twim: p.TWISPI0,
        gpiote_ch: p.GPIOTE_CH0,
        ppi_ch: p.PPI_CH0,
    };
    // Configure UART
    let uart_irq = interrupt::take!(UARTE0_UART0);
    uart_irq.set_priority(interrupt::Priority::P7);

    // Enable the softdevice.
    let (sd, server) = ble::configure_ble();
    // And get the flash controller
    let flash = nrf_softdevice::Flash::take(sd);

    // Spawn all the used tasks.
    spawner.must_spawn(watchdog_task(p.WDT)); // This has to be the first one.
    spawner.must_spawn(ble::softdevice_task(sd));
    spawner.must_spawn(serial::tasks::serial_task(
        p.UARTE0,
        p.P0_03.degrade(),
        p.P0_02.degrade(),
        uart_irq,
    ));
    spawner.must_spawn(tasks::app::application_task(app_peripherals));
    spawner.must_spawn(tasks::dfu_task::dfu_task(flash));
    spawner.must_spawn(rgb::tasks::heartbeat_task());
    spawner.must_spawn(rgb::tasks::rgb_task(
        p.PWM0,
        p.P0_28.degrade(),
        p.P0_26.degrade(),
        p.P0_27.degrade(),
    ));
    spawner.must_spawn(power_state_task(p.P0_04.degrade()));

    // Should await forever.
    ble::run_ble_application(sd, &server).await;
}

#[embassy_executor::task]
async fn power_state_task(monitor_pin: AnyPin) {
    let mut plugged_detect = Input::new(monitor_pin, Pull::Down);
    loop {
        plugged_detect.wait_for_high().await;
        POWER_DETECT.ready(true);
        plugged_detect.wait_for_low().await;
        POWER_DETECT.ready(false);
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

#[entry]
fn main() -> ! {
    info!("Booted successfully!");

    let executor = EXECUTOR.init(custom_executor::Executor::new());
    executor.run(|spawner| spawner.must_spawn(main_task()));
}

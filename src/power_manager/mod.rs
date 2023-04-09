mod macros;
mod types;

use core::sync::atomic::AtomicBool;

use defmt::unwrap;
use embassy_futures::select::select;
use embassy_nrf::gpio::{AnyPin, Input, Pull};
use embassy_sync::{
    blocking_mutex::raw::ThreadModeRawMutex, pubsub::PubSubChannel, signal::Signal,
};

use types::PowerDetect;

use crate::{
    common,
    config_manager::SENSUS_CONFIG,
    sensors::{ONBOARD_SAMPLE_PERIOD, PROBE_SAMPLE_PERIOD},
};

// Used by other parts in our program.
pub static PLUGGED_IN_FLAG: AtomicBool = AtomicBool::new(false);
pub static PLUGGED_DETECT: PowerDetect = PowerDetect {
    plugged_in: PubSubChannel::new(),
    plugged_out: PubSubChannel::new(),
};

/// This flag synchronizes
static PLUGGED_SIG: Signal<ThreadModeRawMutex, bool> = Signal::new();
/// Executes whenever the power state changes.
async fn power_hook() {
    let plugged_in_pub = unwrap!(PLUGGED_DETECT.plugged_in.publisher());
    let plugged_out_pub = unwrap!(PLUGGED_DETECT.plugged_out.publisher());
    loop {
        let plugged_in = PLUGGED_SIG.wait().await;
        let mutex = SENSUS_CONFIG.lock().await;
        let config = mutex.clone().unwrap_or_default();
        match plugged_in {
            true => {
                plugged_in_pub.publish(true).await;
                ONBOARD_SAMPLE_PERIOD.store(
                    config.sampling_period.onboard_sdt_plugged_ms,
                    core::sync::atomic::Ordering::Relaxed,
                );
                PROBE_SAMPLE_PERIOD.store(
                    config.sampling_period.probe_sdt_plugged_ms,
                    core::sync::atomic::Ordering::Relaxed,
                );
            }
            false => {
                plugged_out_pub.publish(true).await;
                ONBOARD_SAMPLE_PERIOD.store(
                    config.sampling_period.onboard_sdt_battery_ms,
                    core::sync::atomic::Ordering::Relaxed,
                );
                PROBE_SAMPLE_PERIOD.store(
                    config.sampling_period.probe_sdt_battery_ms,
                    core::sync::atomic::Ordering::Relaxed,
                );
            }
        }

        PLUGGED_IN_FLAG.store(plugged_in, core::sync::atomic::Ordering::Relaxed);
        // Reset state machines
        common::restart_state_machines();
    }
}

#[embassy_executor::task]
pub async fn power_state_task(monitor_pin: AnyPin) {
    let mut plugged_detect = Input::new(monitor_pin, Pull::None);
    select(power_hook(), async {
        loop {
            defmt::info!("Plugged out");
            PLUGGED_SIG.signal(false);
            plugged_detect.wait_for_high().await;
            defmt::info!("Plugged in");
            PLUGGED_SIG.signal(true);
            plugged_detect.wait_for_low().await;
        }
    })
    .await;
}

mod macros;
mod types;

use core::sync::atomic::AtomicBool;

use embassy_nrf::gpio::{AnyPin, Input, Pull};
use embassy_sync::pubsub::PubSubChannel;

use types::PowerDetect;

// Used by other parts in our program.
pub static PLUGGED_IN_FLAG: AtomicBool = AtomicBool::new(false);
pub static PLUGGED_DETECT: PowerDetect = PowerDetect {
    plugged_in: PubSubChannel::new(),
    plugged_out: PubSubChannel::new(),
};

#[embassy_executor::task]
pub async fn power_state_task(monitor_pin: AnyPin) {
    let mut plugged_detect = Input::new(monitor_pin, Pull::None);
    loop {
        plugged_detect.wait_for_high().await;
        defmt::info!("Plugged in");
        PLUGGED_IN_FLAG.store(true, core::sync::atomic::Ordering::Relaxed);
        PLUGGED_DETECT
            .plugged_in
            .publisher()
            .unwrap()
            .publish(true)
            .await;
        plugged_detect.wait_for_low().await;
        defmt::info!("Plugged out");
        PLUGGED_IN_FLAG.store(false, core::sync::atomic::Ordering::Relaxed);
        PLUGGED_DETECT
            .plugged_out
            .publisher()
            .unwrap()
            .publish(true)
            .await;
    }
}

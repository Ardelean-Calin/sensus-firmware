mod macros;
mod types;

use core::{
    cell::RefCell,
    future::poll_fn,
    sync::atomic::{AtomicBool, Ordering},
    task::{Poll, Waker},
};

use embassy_futures::select::select;
use embassy_nrf::gpio::{AnyPin, Input, Pull};
use embassy_sync::{
    blocking_mutex::{
        raw::{RawMutex, ThreadModeRawMutex},
        Mutex,
    },
    signal::Signal,
    waitqueue::MultiWakerRegistration,
};

use crate::{
    common,
    config_manager::SENSUS_CONFIG,
    sensors::{ONBOARD_SAMPLE_PERIOD, PROBE_SAMPLE_PERIOD},
};

/// This structure is a `MultiWaker`. Using a multiwaker with capacity N, I can
/// await and wake-up N different futures. If I had used an AtomicWaker, every call to
/// register() would have overwritten the previous waker, so I wouldn't have been able
/// to, for example, wait for the device to go into high-power mode more than once.
pub struct MultiWaker<M: RawMutex, const N: usize> {
    inner: Mutex<M, RefCell<MultiWakerRegistration<N>>>,
}

impl<M: RawMutex, const N: usize> MultiWaker<M, N> {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::const_new(M::INIT, RefCell::new(MultiWakerRegistration::new())),
        }
    }

    pub fn register(&self, waker: &Waker) {
        self.inner.lock(|inner| {
            let mut wakers = inner.borrow_mut();
            match wakers.register(waker) {
                Ok(_) => {}
                Err(_) => {
                    wakers.wake();
                    wakers.register(waker).unwrap();
                }
            };
        });
    }

    pub fn wake(&self) {
        self.inner.lock(|inner| {
            let mut wakers = inner.borrow_mut();
            wakers.wake();
        });
    }
}

/// I am using MultiWakerRegistration to wake multiple async tasks.
pub static POWER_WAKER: MultiWaker<ThreadModeRawMutex, 2> = MultiWaker::new();
// Used by other parts in our program.
pub static PLUGGED_IN_FLAG: AtomicBool = AtomicBool::new(false);

/// This future completes when Sensus goes into high-power mode (plugged in)
pub async fn wait_for_hp() {
    poll_fn(move |cx| {
        POWER_WAKER.register(cx.waker());
        if PLUGGED_IN_FLAG.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// This future completes when Sensus goes into low-power mode (unplugged)
pub async fn wait_for_lp() {
    poll_fn(move |cx| {
        POWER_WAKER.register(cx.waker());
        if !PLUGGED_IN_FLAG.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// This flag synchronizes the power_hook with the main power detection task.
static PLUGGED_SIG: Signal<ThreadModeRawMutex, bool> = Signal::new();
/// Executes whenever the power state changes.
async fn power_hook() {
    loop {
        let plugged_in = PLUGGED_SIG.wait().await;
        let mutex = SENSUS_CONFIG.lock().await;
        let config = mutex.clone().unwrap_or_default();
        match plugged_in {
            true => {
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
        POWER_WAKER.wake(); // Wake any async task waiting for a power state change.

        // Reset state machines
        common::restart_state_machines();
    }
}

#[embassy_executor::task]
pub async fn pwr_detect_task(monitor_pin: AnyPin) {
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

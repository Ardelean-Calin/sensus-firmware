#![macro_use]

use defmt_rtt as _; // global logger
use embassy_nrf as _; // time driver

#[cfg(not(debug_assertions))]
use panic_persist as _; // panic handler that logs error into RAM, then soft-resets
#[cfg(debug_assertions)]
use panic_probe as _;

macro_rules! run_while_guard {
    ($guard:expr, $task:expr) => {{
        async move {
            loop {
                let task = $task;
                let guard_enter = ($guard).is_true();
                let guard_leave = ($guard).is_false();
                futures::pin_mut!(task);
                futures::pin_mut!(guard_enter);
                futures::pin_mut!(guard_leave);
                // Wait for the guard to enter our context
                guard_enter.await;
                // Once guard has entered our context, wait for it to go out of scope
                futures::future::select(guard_leave, task).await;
            }
        }
    }};
}

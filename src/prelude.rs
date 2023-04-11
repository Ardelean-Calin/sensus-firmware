#![macro_use]

use defmt_rtt as _; // global logger
use embassy_nrf as _; // time driver

#[cfg(debug_assertions)]
use panic_probe as _;
#[cfg(not(debug_assertions))]
use panic_reset as _; // panic handler that logs error into RAM, then soft-resets

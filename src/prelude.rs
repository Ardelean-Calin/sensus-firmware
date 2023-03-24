#![macro_use]

use defmt_rtt as _; // global logger
use embassy_nrf as _; // time driver

#[cfg(not(debug_assertions))]
use panic_persist as _; // panic handler that logs error into RAM, then soft-resets
#[cfg(debug_assertions)]
use panic_probe as _;

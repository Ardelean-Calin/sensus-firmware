use defmt::Format;
use embassy_nrf::{
    gpio::{Input, Pull},
    Peripherals,
};

#[derive(Format)]
pub struct Diagnostics {
    plugged_in: bool,
    charging: bool,
}

pub fn update_diagnostics(p: &mut Peripherals) -> Diagnostics {
    let plugged_in_pin = Input::new(&mut p.P0_31, Pull::Up);
    let charging_status_pin = Input::new(&mut p.P0_29, Pull::Up);

    Diagnostics {
        plugged_in: plugged_in_pin.is_low(),
        charging: charging_status_pin.is_low(),
    }
}

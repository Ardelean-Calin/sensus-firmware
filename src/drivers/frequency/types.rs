use embassy_nrf::{
    gpio::AnyPin,
    gpiote::{AnyChannel, InputChannel},
    ppi::{AnyConfigurableChannel, Ppi},
    timerv2::{CounterType, TimerType},
};

pub struct FrequencySensor<'a> {
    pub counter: embassy_nrf::timerv2::Timer<CounterType>,
    pub timer: embassy_nrf::timerv2::Timer<TimerType>,
    pub ppi_ch: Ppi<'a, AnyConfigurableChannel, 1, 1>,
    pub gpiote_ch: InputChannel<'a, AnyChannel, AnyPin>,
    pub freq_result: Option<u32>,
}

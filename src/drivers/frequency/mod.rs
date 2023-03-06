pub mod types;

use embassy_nrf::{
    gpio::AnyPin,
    gpiote::{AnyChannel, InputChannel},
    ppi::{AnyConfigurableChannel, Ppi},
    timerv2::{CounterType, TimerType},
};

use types::{FrequencySensor, FrequencySensorError};

impl<'a> FrequencySensor<'a> {
    pub fn new(
        counter: embassy_nrf::timerv2::Timer<CounterType>,
        timer: embassy_nrf::timerv2::Timer<TimerType>,
        ppi_ch: Ppi<'a, AnyConfigurableChannel, 1, 1>,
        gpiote_ch: InputChannel<'a, AnyChannel, AnyPin>,
    ) -> Self {
        Self {
            counter,
            timer,
            ppi_ch,
            gpiote_ch,
            freq_result: None,
        }
    }

    pub fn start_measuring(&mut self) {
        self.counter.clear();
        self.timer.clear();
        self.counter.start();
        self.timer.start();
    }
    pub fn stop_measuring(&mut self) {
        self.counter.stop();
        self.timer.stop();
        let cc = self.counter.cc(0).capture() as u64;
        let timer_val = self.timer.cc(0).capture() as u64;
        let freq = if timer_val != 0 {
            ((cc * 1_000_000) / timer_val) as u32
        } else {
            0
        };
        self.freq_result = Some(freq);
        // See https://infocenter.nordicsemi.com/pdf/nRF52832_Rev_2_Errata_v1.7.pdf
        // Errata No. 78
        // Increased current consumption when the timer has been running and the STOP task is used to stop it.
        self.timer.shutdown();
        self.counter.shutdown();
    }

    pub fn get_frequency(&self) -> Result<u32, FrequencySensorError> {
        self.freq_result.ok_or(FrequencySensorError::Unknown)
    }
}

use defmt::{info, Format};
use embassy_nrf::{
    gpio::{Input, Output, Pin},
    timerv2::{CounterType, Timer, TimerType},
};
use embassy_time::Duration;
use embedded_hal::blocking::i2c::*;
use tmp1x2::marker::mode::Continuous;

#[derive(Format, Clone, Default)]
pub struct ProbeData {
    pub soil_temperature: u16, // unit: 0.1K
    pub soil_moisture: u32,    // unit: Hz
}

#[derive(Debug)]
pub enum ProbeError {
    ProbeCommError,
    ProbeNotConnectedError,
}

pub struct SoilSensor<'a, T, IN, OUT>
where
    IN: Pin,
    OUT: Pin,
{
    timer: Timer<TimerType>,
    counter: Timer<CounterType>,
    probe_enable: Output<'a, OUT>,
    probe_detect: Input<'a, IN>,
    tmp112_sensor: tmp1x2::Tmp1x2<T, Continuous>,
}

impl<'a, T, IN, OUT, E> SoilSensor<'a, T, IN, OUT>
where
    T: Read<Error = E> + Write<Error = E> + WriteRead<Error = E>,
    IN: Pin,
    OUT: Pin,
    E: core::fmt::Debug,
{
    /// Constructor for a SoilSensor structure.
    pub fn new(
        timer: Timer<TimerType>,
        counter: Timer<CounterType>,
        i2c_tmp: T,
        probe_enable: Output<'a, OUT>,
        probe_detect: Input<'a, IN>,
    ) -> SoilSensor<'a, T, IN, OUT> {
        let tmp112_sensor = tmp1x2::Tmp1x2::new(i2c_tmp, tmp1x2::SlaveAddr::Default);
        SoilSensor {
            timer,
            counter,
            tmp112_sensor,
            probe_enable,
            probe_detect,
        }
    }

    /// Checks whether there's a probe connected to the PlantBuddy.
    fn check_connection(&self) -> Result<(), ProbeError> {
        if self.probe_detect.is_high() {
            Err(ProbeError::ProbeNotConnectedError)
        } else {
            Ok(())
        }
    }

    fn enable_probe(&mut self) {
        self.probe_enable.set_high();
    }

    fn disable_probe(&mut self) {
        self.probe_enable.set_low();
    }

    /// Triggers an asynchronous sampling of soil moisture and soil temperature and returns the result.
    /// TODO: Unfortunately, I take ownership of self and never return it back. I am not experienced enough to fix this for now.
    pub async fn sample(&mut self) -> Result<ProbeData, ProbeError> {
        // Check if probe is connected. Return error if it is not.
        // self.check_connection()?;
        info!("Enabling probe...");
        self.enable_probe();
        embassy_time::Timer::after(Duration::from_millis(2)).await; // 2ms to settle the power regulator

        // Split into two: sample the temperature & sample the moisture.
        let freq = self.sample_soil_water().await;
        let temp_milli_c = self.sample_soil_temp()?;
        self.disable_probe();
        info!("Disabled probe.");

        let probe_data = ProbeData {
            soil_moisture: freq,
            soil_temperature: ((temp_milli_c + 273150) / 100) as u16,
        };

        Ok(probe_data)
    }

    /// Measure soil temperature via a Tmp112 sensor mounted on the probe.
    /// Returns the soil temperature in millidegrees C
    fn sample_soil_temp(&mut self) -> Result<i32, ProbeError> {
        let soil_temp = self
            .tmp112_sensor
            .read_temperature()
            .map_err(|_| ProbeError::ProbeCommError)?;
        // Convert to millidegree C
        let soil_temp = (soil_temp * 1000.0) as i32;

        Ok(soil_temp)
    }

    /// Measure soil moisture using a 555-timer astable oscillator.
    async fn sample_soil_water(&self) -> u32 {
        // How frequency measurement works:
        // We bind the GPIO19 falling edge event to the counter's count up event.
        // In parallel, we start a normal 1MHz timer.
        // Then, after 100ms we get the value of CC in the counter, therefore giving us the number of events per 100ms
        self.counter.clear();
        self.timer.clear();
        self.counter.start();
        self.timer.start();
        // TODO: Replace with impl DelayUs
        embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
        self.counter.stop();
        self.timer.stop();
        let cc = self.counter.cc(0).capture() as u64;
        let timer_val = self.timer.cc(0).capture() as u64;
        let freq: u32;
        if timer_val != 0 {
            freq = ((cc * 1_000_000) / timer_val) as u32;
        } else {
            freq = 0;
        }
        // See https://infocenter.nordicsemi.com/pdf/nRF52832_Rev_2_Errata_v1.7.pdf
        // Errata No. 78
        // Increased current consumption when the timer has been running and the STOP task is used to stop it.
        self.timer.shutdown();
        self.counter.shutdown();

        freq
    }
}

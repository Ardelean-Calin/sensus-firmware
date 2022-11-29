use defmt::Format;
use embassy_nrf::{
    gpio::{Input, Pin},
    timerv2::{CounterType, Timer, TimerType},
};
use embedded_hal::blocking::i2c::*;

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

pub struct SoilSensor<'a, T, P>
where
    P: Pin,
{
    timer: Timer<TimerType>,
    counter: Timer<CounterType>,
    probe_detect: Input<'a, P>,
    i2c_tmp: T,
}

impl<'a, T, P, E> SoilSensor<'a, T, P>
where
    T: Read<Error = E> + Write<Error = E> + WriteRead<Error = E>,
    P: Pin,
    E: core::fmt::Debug,
{
    /// Constructor for a SoilSensor structure.
    pub fn new(
        timer: Timer<TimerType>,
        counter: Timer<CounterType>,
        i2c_tmp: T,
        probe_detect: Input<P>,
    ) -> SoilSensor<T, P> {
        SoilSensor {
            timer,
            counter,
            i2c_tmp,
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

    /// Triggers an asynchronous sampling of soil moisture and soil temperature and returns the result.
    /// TODO: Unfortunately, I take ownership of self and never return it back. I am not experienced enough to fix this for now.
    pub async fn sample(self) -> Result<ProbeData, ProbeError> {
        // Check if probe is connected.
        self.check_connection()?;
        // Split into two: sample the temperature & sample the moisture.
        let freq = self.sample_soil_water().await;
        let temp = self.sample_soil_temp()?;

        let probe_data = ProbeData {
            soil_moisture: freq,
            soil_temperature: ((temp + 273150) / 100) as u16,
        };

        Ok(probe_data)
    }

    /// Measure soil temperature via a Tmp112 sensor mounted on the probe.
    /// Returns the soil temperature in millidegrees C
    fn sample_soil_temp(self) -> Result<i32, ProbeError> {
        let mut tmp112_sensor = tmp1x2::Tmp1x2::new(self.i2c_tmp, tmp1x2::SlaveAddr::Default);
        let soil_temp = tmp112_sensor
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
        let freq: u32 = ((cc * 1_000_000) / timer_val) as u32;

        freq
    }
}

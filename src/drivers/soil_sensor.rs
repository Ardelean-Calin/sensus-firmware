use defmt::{info, Format};
use embassy_nrf::{
    gpio::{Input, Output, Pin},
    timerv2::{CounterType, Timer, TimerType},
};
use embassy_time::Duration;
use embassy_time::Timer as SoftwareTimer;
use embedded_hal::blocking::i2c::*;
use serde::{Deserialize, Serialize};
use tmp1x2::marker::mode::Continuous;

/* Constants */
static PROBE_STARTUP_TIME: Duration = Duration::from_millis(2);
static TMP_MAX_CONV_TIME: Duration = Duration::from_millis(35);

#[derive(Format, Clone, Default, Serialize, Deserialize)]
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
        self.check_connection()?;
        // info!("Enabling probe...");
        self.enable_probe();
        SoftwareTimer::after(PROBE_STARTUP_TIME).await; // 2ms to settle the power regulator

        // Start measuring the soil water content. While this completes, we measure the temperature.
        self.sample_soil_water_start();
        let temp_milli_c = self.sample_soil_temperature().await?;
        let freq = self.sample_soil_water_stop();
        self.disable_probe();
        // info!("Disabled probe.");
        // The whole measurement should have taken no more than 35ms.

        let probe_data = ProbeData {
            soil_moisture: freq,
            soil_temperature: ((temp_milli_c + 273150) / 100) as u16,
        };

        Ok(probe_data)
    }

    /// Measure soil temperature via a Tmp112 sensor mounted on the probe.
    /// Returns the soil temperature in millidegrees C
    /// NOTE: The maximum conversion time is 35ms
    async fn sample_soil_temperature(&mut self) -> Result<i32, ProbeError> {
        // Wait 35ms
        SoftwareTimer::after(TMP_MAX_CONV_TIME).await;
        let soil_temp = self
            .tmp112_sensor
            .read_temperature()
            .map_err(|_| ProbeError::ProbeCommError)?;
        // Convert to millidegree C
        let soil_temp = (soil_temp * 1000.0) as i32;

        Ok(soil_temp)
    }

    /// Starts a soil water content measurement. Since it takes 35ms for the temperature sensor to
    /// provide the first sample, anyway, we can use that time to measure the soil water content.
    fn sample_soil_water_start(&self) {
        self.counter.clear();
        self.timer.clear();
        self.counter.start();
        self.timer.start();
    }

    /// Measure soil moisture using a 555-timer astable oscillator.
    fn sample_soil_water_stop(&self) -> u32 {
        // How frequency measurement works:
        // We bind the GPIO19 falling edge event to the counter's count up event.
        // In parallel, we start a normal 1MHz timer.
        // Then, after 35ms we get the value of CC in the counter, therefore giving us the number of events per 35ms
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

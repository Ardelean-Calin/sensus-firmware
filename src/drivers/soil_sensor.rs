use embassy_nrf::timerv2::{CounterType, Timer, TimerType};
use embedded_hal::blocking::i2c::*;

pub struct SoilSensor<T> {
    timer: Timer<TimerType>,
    counter: Timer<CounterType>,
    i2c_tmp: T,
}

impl<T, E> SoilSensor<T>
where
    T: Read<Error = E> + Write<Error = E> + WriteRead<Error = E>,
    E: core::fmt::Debug,
{
    /// Constructor for a SoilSensor structure.
    pub fn new(timer: Timer<TimerType>, counter: Timer<CounterType>, i2c_tmp: T) -> SoilSensor<T> {
        SoilSensor {
            timer,
            counter,
            i2c_tmp,
        }
    }

    /// Triggers an asynchronous sampling of soil moisture and soil temperature and returns the result.
    /// TODO: Unfortunately, I take ownership of self and never return it back. I am not experienced enough to fix this for now.
    pub async fn sample(self) -> (u32, f32) {
        // Split into two: sample the temperature & sample the moisture.
        let freq = self.sample_soil_water().await;
        let temp = self.sample_soil_temp();

        (freq, temp)
    }

    /// Measure soil temperature via a Tmp112 sensor mounted on the probe.
    fn sample_soil_temp(self) -> f32 {
        let mut tmp112_sensor = tmp1x2::Tmp1x2::new(self.i2c_tmp, tmp1x2::SlaveAddr::Default);
        let soil_temp = tmp112_sensor.read_temperature().unwrap();
        // info!("Soil temperature: {:?}", soil_temp);
        soil_temp
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

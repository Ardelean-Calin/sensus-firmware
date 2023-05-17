pub mod types;

use core::ops::{Add, Mul, Sub};

use crate::sensors::drivers::frequency::types::FrequencySensor;
use crate::sensors::types::Error;
use crate::sensors::types::ProbePeripherals;
use crate::sensors::types::ProbeSample;
use embassy_nrf::bind_interrupts;
use embassy_nrf::peripherals;
use embassy_nrf::{
    gpio::{Input, Level, Output, Pull},
    gpiote::InputChannel,
    ppi::Ppi,
    timerv2, twim,
};
use embassy_time::{Duration, Timer};

use types::ProbeHardware;

// Necessary implementations to be able to filter the data.
impl Add for ProbeSample {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        ProbeSample {
            moisture: self.moisture + rhs.moisture,
            temperature: self.temperature + rhs.temperature,
            moisture_raw: self.moisture_raw + rhs.moisture_raw,
        }
    }
}

impl Sub for ProbeSample {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        ProbeSample {
            moisture: self.moisture - rhs.moisture,
            temperature: self.temperature - rhs.temperature,
            moisture_raw: self.moisture_raw - rhs.moisture_raw,
        }
    }
}

impl Mul<f32> for ProbeSample {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        ProbeSample {
            moisture: self.moisture * rhs,
            temperature: self.temperature * rhs,
            moisture_raw: self.moisture_raw * rhs,
        }
    }
}

bind_interrupts!(struct I2cIrqs {
    SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1 => twim::InterruptHandler<peripherals::TWISPI1>;
});

impl<'a> ProbeHardware<'a> {
    pub fn from_peripherals(per: &'a mut ProbePeripherals) -> Self {
        // Frequency measurement initialization
        let freq_cnter = timerv2::Timer::new(timerv2::TimerInstance::TIMER1)
            .into_counter()
            .with_bitmode(timerv2::Bitmode::B32);

        let freq_timer = timerv2::Timer::new(timerv2::TimerInstance::TIMER2)
            .into_timer()
            .with_bitmode(timerv2::Bitmode::B32)
            .with_frequency(timerv2::Frequency::F1MHz);

        let freq_in = InputChannel::new(
            &mut per.instance_gpiote,
            Input::new(&mut per.pin_probe_freq, embassy_nrf::gpio::Pull::Up),
            embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
        );

        let mut ppi_ch = Ppi::new_one_to_one(
            &mut per.instance_ppi,
            freq_in.event_in(),
            freq_cnter.task_count(),
        );
        ppi_ch.enable();
        let freq_sensor = FrequencySensor::new(freq_cnter, freq_timer, ppi_ch, freq_in);

        // Probe enable pin
        let output_probe_enable = Output::new(
            &mut per.pin_probe_enable,
            Level::Low,
            embassy_nrf::gpio::OutputDrive::Standard,
        );
        // Probe detect pin
        let input_probe_detect = Input::new(&mut per.pin_probe_detect, Pull::Up);

        // I2C initialization
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K100; // 100k seems to be best for low power consumption.

        let i2c_bus = twim::Twim::new(
            &mut per.instance_twim,
            I2cIrqs,
            &mut per.pin_probe_sda,
            &mut per.pin_probe_scl,
            i2c_config,
        );

        Self {
            input_probe_detect,
            output_probe_enable,
            i2c_bus,
            freq_sensor,
        }
    }
}

#[inline]
fn moisture_from_freq(freq: u32) -> f32 {
    // 1.6 MIL = 0%
    // 100k = 100%
    let result = (-100.0 * ((freq as f32) + 100e3)) / 1.5e6 + 100.0;

    // Clamp to 0-100%
    result.clamp(0.0, 100.0)
}

/* Constants */
static PROBE_STARTUP_TIME: Duration = Duration::from_millis(20);
static TMP_MAX_CONV_TIME: Duration = Duration::from_millis(35);

pub async fn sample_soil(mut hw: ProbeHardware<'_>) -> Result<ProbeSample, Error> {
    // Detect the presence of a probe before doing any other operation.
    if hw.input_probe_detect.get_level() == Level::High {
        return Err(Error::ProbeDisconnected);
    }

    let mut tmp112_sensor = tmp1x2::Tmp1x2::new(hw.i2c_bus, tmp1x2::SlaveAddr::Default);
    let mut enable_ctrl = hw.output_probe_enable;

    enable_ctrl.set_high();
    Timer::after(PROBE_STARTUP_TIME).await; // 2ms to settle the power regulator

    // Start frequency measurement and also measure temperature in the meantime.
    hw.freq_sensor.start_measuring();
    Timer::after(TMP_MAX_CONV_TIME).await; // Wait 35ms
    let temperature = tmp112_sensor
        .read_temperature()
        .map_err(|_| Error::ProbeI2cFailed)?;
    // Stop frequency measurement and get result.
    hw.freq_sensor.stop_measuring();
    let frequency = hw.freq_sensor.get_frequency()?;

    enable_ctrl.set_low();

    let moisture = moisture_from_freq(frequency);
    Ok(ProbeSample {
        moisture_raw: frequency as f32,
        moisture,
        temperature,
    })
}

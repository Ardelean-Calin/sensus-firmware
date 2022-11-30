use defmt::{info, unwrap, Format};
use embassy_nrf::{
    self,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    gpiote::InputChannel,
    interrupt,
    peripherals::{GPIOTE_CH0, P0_06, P0_19, P0_20, PPI_CH0, TWISPI0},
    ppi::Ppi,
    saadc::{self, Saadc},
    timerv2::{self, CounterType, TimerType},
    twim::{self, Twim},
    Peripherals,
};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::pubsub::PubSubChannel;
use embassy_time::{Duration, Instant, Timer};
use futures::future::join3;

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;
use battery_sensor::BatterySensor;

#[path = "../drivers/environment.rs"]
mod environment;
use environment::EnvironmentSensors;

#[path = "../drivers/soil_sensor.rs"]
mod soil_sensor;
use soil_sensor::SoilSensor;

use ltr303_async::{self, LTR303Result};
use shared_bus::{BusManager, NullMutex};
use shtc3_async::{self, SHTC3Result};

use self::soil_sensor::ProbeData;

// Sensor data transmission channel. Queue of 4. 1 publisher, 3 subscribers
pub static SENSOR_DATA_BUS: PubSubChannel<ThreadModeRawMutex, DataPacket, 4, 3, 1> =
    PubSubChannel::new();

// Constants
const MEAS_INTERVAL: Duration = Duration::from_secs(5);

// Data we get from main PCB:
//  2 bytes for battery voltage  => u16; unit: mV
//  2 bytes for air temperature  => u16; unit: 0.1 Kelvin
//  2 bytes for air humidity     => u16; unit: 0.01%
//  2 bytes for illuminance      => u16; unit: Lux
// Data we get from (optional) soil probe:
//  2 bytes for soil temperature => u16; unit: 0.1 Kelvin
//  4 bytes for soil moisture    => u32; unit: Hertz
//
// TODO:
//  1) We can encode soil moisture in percentages if we can find a way to directly map
//     frequency to %.
//  2) We can further "compress" the bytes. For example, temperature in Kelvin can be
//     expressed with 9 bits. 0-512
#[derive(Format, Clone)]
pub struct SensorData {
    pub battery_voltage: u32,
    pub sht_data: shtc3_async::SHTC3Result,
    pub ltr_data: ltr303_async::LTR303Result,
    pub soil_temperature: i32,
    pub soil_moisture: u32,
}

#[derive(Format, Clone)]

pub struct EnvironmentData {
    air_temperature: u16, // unit: 0.1K
    air_humidity: u16,    // unit: 0.1%
    illuminance: u16,     // unit: Lux
}

impl EnvironmentData {
    fn new(sht_data: SHTC3Result, ltr_data: LTR303Result) -> Self {
        EnvironmentData {
            air_temperature: ((sht_data.temperature.as_millidegrees_celsius() + 273150) / 100)
                as u16,
            air_humidity: (sht_data.humidity.as_millipercent() / 100) as u16,
            illuminance: ltr_data.lux,
        }
    }
}

// 14 bytes total.
#[derive(Format, Clone)]
pub struct DataPacket {
    pub battery_voltage: u16, // unit: mV
    pub env_data: EnvironmentData,
    pub probe_data: ProbeData,
}

impl DataPacket {
    pub fn to_bytes_array(&self) -> [u8; 14] {
        let mut arr = [0u8; 14];
        // Encode battery voltage
        arr[0] = self.battery_voltage.to_be_bytes()[0];
        arr[1] = self.battery_voltage.to_be_bytes()[1];
        // Encode air temperature
        arr[2] = self.env_data.air_temperature.to_be_bytes()[0];
        arr[3] = self.env_data.air_temperature.to_be_bytes()[1];
        // Encode air humidity
        arr[4] = self.env_data.air_humidity.to_be_bytes()[0];
        arr[5] = self.env_data.air_humidity.to_be_bytes()[1];
        // Encode solar illuminance
        arr[6] = self.env_data.illuminance.to_be_bytes()[0];
        arr[7] = self.env_data.illuminance.to_be_bytes()[1];
        // Probe data
        // Encode soil temperature
        arr[8] = self.probe_data.soil_temperature.to_be_bytes()[0];
        arr[9] = self.probe_data.soil_temperature.to_be_bytes()[1];
        // Encode soil moisture
        arr[10] = self.probe_data.soil_moisture.to_be_bytes()[0];
        arr[11] = self.probe_data.soil_moisture.to_be_bytes()[1];
        arr[12] = self.probe_data.soil_moisture.to_be_bytes()[2];
        arr[13] = self.probe_data.soil_moisture.to_be_bytes()[3];

        arr
    }
}

impl Default for SensorData {
    fn default() -> Self {
        Self {
            battery_voltage: Default::default(),
            sht_data: Default::default(),
            ltr_data: Default::default(),
            soil_temperature: Default::default(),
            soil_moisture: Default::default(),
        }
    }
}

// This struct shall contain all peripherals we use for data aquisition. Easy to track if something
// changes.
struct Hardware<'a> {
    // One enable pin for external sensors (frequency + tmp112)
    enable_pin: Output<'a, P0_06>,
    // One I2C bus for SHTC3 and LTR303-ALS, as well as TMP112.
    i2c_bus: BusManager<NullMutex<Twim<'a, TWISPI0>>>,
    // Two v2 timers for the frequency measurement as well as one PPI channel.
    freq_cnter: timerv2::Timer<CounterType>,
    freq_timer: timerv2::Timer<TimerType>,
    probe_detect: Input<'a, P0_20>,
    adc: Saadc<'a, 1>,
    // Private variables. Why? Because they get dropped if I don't store them here.
    _ppi_ch: Ppi<'a, PPI_CH0, 1, 1>,
    _freq_in: InputChannel<'a, GPIOTE_CH0, P0_19>,
}

impl<'a> Hardware<'a> {
    // Peripherals reference has a lifetime at least that of the hardware. Fixes "borrowed previous loop" errors.
    fn new<'p: 'a>(
        p: &'p mut Peripherals,
        adc_irq: &'p mut interrupt::SAADC,
        i2c_irq: &'p mut interrupt::SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0,
    ) -> Self {
        // Soil enable pin used by soil probe sensor.
        let mut sen = Output::new(&mut p.P0_06, Level::Low, OutputDrive::Standard);
        sen.set_low();

        // ADC initialization
        let mut config = saadc::Config::default();
        config.oversample = saadc::Oversample::OVER64X;
        let channel_cfg = saadc::ChannelConfig::single_ended(&mut p.P0_03);
        let saadc = saadc::Saadc::new(&mut p.SAADC, adc_irq, config, [channel_cfg]);

        // I2C initialization
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K400; // 400k seems to be best for low power consumption.

        let i2c_bus = Twim::new(
            &mut p.TWISPI0,
            i2c_irq,
            &mut p.P0_14,
            &mut p.P0_15,
            i2c_config,
        );
        // Create a bus manager to be able to share i2c buses easily.
        let i2c_bus = shared_bus::BusManagerSimple::new(i2c_bus);

        // Counter + Timer initialization
        let counter = timerv2::Timer::new(timerv2::TimerInstance::TIMER1)
            .into_counter()
            .with_bitmode(timerv2::Bitmode::B32);

        let my_timer = timerv2::Timer::new(timerv2::TimerInstance::TIMER2)
            .into_timer()
            .with_bitmode(timerv2::Bitmode::B32)
            .with_frequency(timerv2::Frequency::F1MHz);

        let freq_in = InputChannel::new(
            &mut p.GPIOTE_CH0,
            Input::new(&mut p.P0_19, embassy_nrf::gpio::Pull::Up),
            embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
        );

        let mut ppi_ch =
            Ppi::new_one_to_one(&mut p.PPI_CH0, freq_in.event_in(), counter.task_count());
        ppi_ch.enable();

        let probe_detect = Input::new(&mut p.P0_20, Pull::Up);

        // Create new struct. If I don't store ppi_ch and freq_in inside the struct, they will get dropped from
        // memory when I get here, causing Frequency Measurement to not work. Therefore I store them in private
        // fields.
        Self {
            enable_pin: sen,
            i2c_bus: i2c_bus,
            freq_cnter: counter,
            freq_timer: my_timer,
            probe_detect: probe_detect,
            adc: saadc,
            _ppi_ch: ppi_ch,
            _freq_in: freq_in,
        }
    }
}

struct Sensors {}
impl Sensors {
    fn new() -> Self {
        Self {}
    }

    async fn sample<'a>(&'a self, hw: Hardware<'a>) -> DataPacket {
        // Environement data: air temperature & humidity, ambient light.
        let mut env_sensors =
            EnvironmentSensors::new(hw.i2c_bus.acquire_i2c(), hw.i2c_bus.acquire_i2c());
        // Probe data: soil moisture & temperature.
        let mut probe_sensor = SoilSensor::new(
            hw.freq_timer,
            hw.freq_cnter,
            hw.i2c_bus.acquire_i2c(),
            hw.enable_pin,
            hw.probe_detect,
        );
        // Battery voltage sensor. TODO could also be battery status
        let mut batt_sensor = BatterySensor::new(hw.adc);

        // Sample everything at the same time to save processing time.
        let (environment_data, probe_data, batt_mv) = join3(
            env_sensors.sample(),
            probe_sensor.sample(),
            batt_sensor.sample_mv(),
        )
        .await;

        // I could have some type of field representing invalid data. InvalidData<LastData>. This way, in case
        // of an error I keep the last received value (or 0 if no value) and just wrap it inside InvalidData
        // to mark it as being non-valid.
        DataPacket {
            battery_voltage: batt_mv,
            env_data: environment_data,
            probe_data: probe_data.unwrap_or_default(),
        }
        // At the end, all our sensors are dropped since we own Hardware. So all peripherals found there
        // get dropped. That includes i2c, gpio, etc.
    }
}

#[embassy_executor::task]
pub async fn application_task(mut p: Peripherals) {
    // Used interrupts; Need to be declared only once otherwise we get a core panic.
    let mut adc_irq = interrupt::take!(SAADC);
    let mut i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    let data_publisher = unwrap!(SENSOR_DATA_BUS.publisher());

    // The application runs indefinitely.
    loop {
        let start_time = Instant::now();
        // Used hardware will get dropped at the end of the loop. Thus guaranteeing low power consumption.
        let used_hardware = Hardware::new(&mut p, &mut adc_irq, &mut i2c_irq);
        // Hardware is given to sensors, which then lets it drop out of scope
        let sensors = Sensors::new();
        let sensor_data = sensors.sample(used_hardware).await;
        info!("{:?}", sensor_data);

        // Publish the measured data.
        data_publisher.publish_immediate(sensor_data);

        // I also have diagnostic data. Stuff like "is the battery connected? Is it charging?"
        // let diag = Diagnostics::new();
        // let diag_data = diag.get_diag_data(&mut p); // I should be able to use a mutable reference here, since hardware went out of scope.

        // Wait 60s for the next measurement. TODO. Drop used_hardware before going to sleep. Either join or force drop.
        let sleep_duration = MEAS_INTERVAL - (Instant::now() - start_time);
        Timer::after(sleep_duration).await;
    }
}

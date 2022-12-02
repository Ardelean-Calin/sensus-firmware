use defmt::{info, unwrap, warn, Format};
use embassy_nrf::{
    self,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    gpiote::InputChannel,
    interrupt::{self, SAADC, SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0},
    peripherals::{GPIOTE_CH0, P0_06, P0_19, P0_20, PPI_CH0, TWISPI0},
    ppi::Ppi,
    saadc::{self, Saadc},
    timerv2::{self, CounterType, TimerType},
    twim::{self, Twim},
    Peripherals,
};
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, pubsub::Publisher};
use embassy_time::{Duration, Instant, Timer};

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;

#[path = "../drivers/environment.rs"]
mod environment;

#[path = "../drivers/soil_sensor.rs"]
mod soil_sensor;

#[path = "../drivers/rgbled.rs"]
mod rgbled;

mod sensors;

use ltr303_async::{self, LTR303Result};
use shared_bus::{BusManager, NullMutex};
use shtc3_async::{self, SHTC3Result};

use self::soil_sensor::ProbeData;

// Sensor data transmission channel. Queue of 4. 1 publisher, 3 subscribers
pub static SENSOR_DATA_BUS: PubSubChannel<ThreadModeRawMutex, DataPacket, 4, 3, 1> =
    PubSubChannel::new();

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
pub struct Hardware<'a> {
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

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Format)]
enum Operation {
    NONE = 0,
    RUN_DIAGNOSTICS,
    RUN_DATA_AQUISITION,
    RUN_SERIAL_COMM,
}

#[derive(Debug, Clone)]
struct SchedulerEntry {
    scheduled_operation: Operation,
    scheduled_time: Instant,
    free: bool, // Indicates whether this entry can be populated.
}

impl SchedulerEntry {
    fn new(op: Operation, time: Instant) -> Self {
        SchedulerEntry {
            scheduled_operation: op,
            scheduled_time: time,
            free: false,
        }
    }

    fn free(&mut self) {
        self.scheduled_time = Instant::MAX;
        self.free = true;
    }
}

impl Default for SchedulerEntry {
    fn default() -> Self {
        Self {
            scheduled_time: Instant::MAX,
            scheduled_operation: Operation::NONE,
            free: true,
        }
    }
}

struct PersistentData<'a> {
    adc_irq: SAADC,
    i2c_irq: SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0,
    data_publisher: Publisher<'a, ThreadModeRawMutex, DataPacket, 4, 3, 1>,
}
impl<'a> PersistentData<'a> {
    fn new() -> Self {
        // Used interrupts; Need to be declared only once otherwise we get a core panic.
        let adc_irq = interrupt::take!(SAADC);
        let i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
        let data_publisher = unwrap!(SENSOR_DATA_BUS.publisher());

        Self {
            adc_irq,
            i2c_irq,
            data_publisher,
        }
    }
}

#[derive(Debug, Format)]
enum SchedulerError {
    TableFull = 0,
}

struct Scheduler {
    p: Peripherals,
    current_time: Instant,
    table: [SchedulerEntry; 10],
    persistent_data: PersistentData<'static>,
}

impl Scheduler {
    fn new(p: Peripherals) -> Self {
        let table: [SchedulerEntry; 10] = Default::default();
        let current_time = Instant::now();

        Scheduler {
            p,
            table,
            current_time,
            persistent_data: PersistentData::new(),
        }
    }

    /**
     * This function searches for the first free table entry and places an operation there.
     *
     * If no free entries are found, it means our table is full, so the scheduler reports and error.
     */
    fn schedule(&mut self, op: Operation, time: Instant) -> Result<(), SchedulerError> {
        let mut found: bool = false;

        for i in 0..self.table.len() {
            let entry = &mut self.table[i];
            if entry.free == true {
                *entry = SchedulerEntry::new(op.clone(), time.clone());
                found = true;
                break;
            }
        }

        if found == false {
            Err(SchedulerError::TableFull)
        } else {
            Ok(())
        }
    }

    /**
     * This function runs the given operation.
     *
     * Also returns an error if we try to schedule an operation in the future but no space remains.
     */
    async fn run_operation(&mut self, op: &Operation) -> Result<(), SchedulerError> {
        match op {
            Operation::RUN_DIAGNOSTICS => {
                self.schedule(
                    Operation::RUN_DIAGNOSTICS,
                    self.current_time + Duration::from_millis(100),
                )?;
                // info!("Diagnostics");
            }
            Operation::RUN_DATA_AQUISITION => {
                self.schedule(
                    Operation::RUN_DATA_AQUISITION,
                    self.current_time + sensors::MEAS_INTERVAL,
                )?;
                info!("Data aquisition!");
                let hw = Hardware::new(
                    &mut self.p,
                    &mut self.persistent_data.adc_irq,
                    &mut self.persistent_data.i2c_irq,
                );
                let sensors = sensors::Sensors::new();
                let sensor_data = sensors.sample(hw).await;
                info!("{:?}", sensor_data);

                // Publish the measured data.
                self.persistent_data
                    .data_publisher
                    .publish_immediate(sensor_data);
            }
            Operation::RUN_SERIAL_COMM => {}
            Operation::NONE => {
                // do nothing
            }
        }

        Ok(())
    }

    fn init_table(&mut self) {
        self.table[0] = SchedulerEntry::new(Operation::RUN_DIAGNOSTICS, Instant::now());
        self.table[1] = SchedulerEntry::new(Operation::RUN_DATA_AQUISITION, Instant::now());
    }

    async fn run(&mut self) -> Result<(), SchedulerError> {
        info!("Running scheduler!");
        loop {
            let current_time = Instant::now();
            self.current_time = current_time.clone();

            for i in 0..self.table.len() {
                let entry = self.table[i].clone();
                // Since the table is sorted, I only need to do this until I find a time that doesn't belong here.
                if entry.scheduled_time <= current_time {
                    // info!("Should run operation");
                    self.run_operation(&entry.scheduled_operation).await?;
                    self.table[i].free();
                }
            }

            // Calculate sleep time so as to run the loop every 100ms
            let sleep_duration_option = Duration::from_millis(100)
                .checked_sub(Instant::now().checked_duration_since(current_time).unwrap());

            if let Some(sleep_duration) = sleep_duration_option {
                Timer::after(sleep_duration).await;
            } else {
                warn!("A task took longer than a timeslot of the Scheduler.");
            }
        }
    }
}

#[embassy_executor::task]
pub async fn application_task(p: Peripherals) {
    #[allow(unused_doc_comments)]
    /**
     * Cum ar fi sa separ task-urile in doua?
     * 1) Sensors task => Aduna date de la senzori, deinitializeaza perifericele cand termina cu ele
     * 2) Diagnostics task => Monitorizeaza diferite GPIO-uri, detecteaza daca incarcam, suntem plugged in, etc.
     *  2.a) BONUS! Daca suntem plugged in, pot rula inca un task/coroutine, si anume SerialCommTask
     *       Pot inclusiv face ceva de genul:
     *          select(plugged_in.is_low().await, serialCommTask().await)
     *       Astfel daca deconectez USB-C, ul, se distruge automat si serialCommTask
     *  
     *  NOTE: Nu ma pot baza pe intreruperi de GPIO, vad ca consuma prea mult curent. Va trebui sa am un task ciclic
     *        ex. 100ms, care verifica nivelul GPIO-urilor, il stocheaza, si apoi merge inapoi la somn.
     */

    // let rgbled = RGBLED::new_rgb(&mut p.PWM0, &mut p.P0_22, &mut p.P0_23, &mut p.P0_24);

    /// Ce trebuie sa se deinitializeze cand merg in sleep?
    ///  *  1) I2C bus-ul
    ///  *  2) HW timerele
    ///  *  3) ADC-ul
    ///  *
    ///  *  Ce nu trebuie sa se deinitializeze?
    ///  *  1) GPIO-ul ce detecteaza incarcarea.
    // TODO: I want different behaviors...
    //  if on battery => minimum power consumption.
    //  if plugged in => some additional drivers
    let mut scheduler = Scheduler::new(p);
    scheduler.init_table();
    // Should never return.
    unwrap!(scheduler.run().await);
}

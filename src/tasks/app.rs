use embassy_nrf::{
    self,
    gpio::{Input, Level, Output, OutputDrive, Pin},
    gpiote::InputChannel,
    peripherals::{P0_06, PPI_CH0, TWISPI0},
    ppi::Ppi,
    timerv2::{self, CounterType, TimerType},
    twim::Twim,
    Peripherals,
};
use embassy_time::{Duration, Instant, Timer};

// This struct shall contain all peripherals we use for data aquisition. Easy to track if something
// changes.
struct Hardware<'a> {
    // One enable pin for external sensors (frequency + tmp112)
    enable_pin: Output<'a, P0_06>,
    // One I2C bus for SHTC3 and LTR303-ALS, as well as TMP112.
    i2c_bus: Twim<'a, TWISPI0>,
    // Two v2 timers for the frequency measurement as well as one PPI channel.
    freq_cnter: timerv2::Timer<CounterType>,
    freq_timer: timerv2::Timer<TimerType>,
    ppi_ch: Ppi<'a, PPI_CH0, 1, 1>,
    // TODO: One SAADC for battery level measurement.
}

struct SensorData {
    battery_voltage: u32,
    sht_data: shtc3_async::Measurement,
    ltr_data: ltr303_async::Measurement,
    soil_temperature: f32,
    soil_humidity: u32,
}

struct Sensors {
    p: Peripherals,
}
impl Sensors {
    fn new(p: Peripherals) -> Self {
        Self { p }
    }

    async fn init(&mut self) {
        // First we create the resources
        let sen = Output::new(&mut self.p.P0_06, Level::Low, OutputDrive::Standard);

        // Hardware timer(s) used by the probe
        // Let's bind the GPIO19 falling edge event to the counter's count up event.
        // In parallel, we start a normal 1MHz timer.
        // Then, after 100ms we get the value of CC in the counter, therefore giving us the number of events per 100ms
        let freq_in = InputChannel::new(
            &mut self.p.GPIOTE_CH0,
            Input::new(&mut self.p.P0_19, embassy_nrf::gpio::Pull::Up),
            embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
        );

        let counter = timerv2::Timer::new(timerv2::TimerInstance::TIMER1)
            .into_counter()
            .with_bitmode(timerv2::Bitmode::B32);

        let my_timer = timerv2::Timer::new(timerv2::TimerInstance::TIMER2)
            .into_timer()
            .with_bitmode(timerv2::Bitmode::B32)
            .with_frequency(timerv2::Frequency::F1MHz);
    }
    async fn deinit(&self) {}
    async fn sample(&self) -> SensorData {
        SensorData {
            battery_voltage: todo!(),
            sht_data: todo!(),
            ltr_data: todo!(),
            soil_temperature: todo!(),
            soil_humidity: todo!(),
        }
    }

    // Trigges a data aquisition on all sensors. Waits for everything to finish
    // before sending data back and sleeping the peripherals.
    async fn aquire_and_sleep(&mut self) -> SensorData {
        self.init().await;
        let data = self.sample().await;
        self.deinit().await;

        return data;
    }
}

#[embassy_executor::task]
pub async fn application_task() {
    let mut config = embassy_nrf::config::Config::default();
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P2;

    // Peripherals config
    let p = embassy_nrf::init(config);
    let mut sensors = Sensors::new(p);

    // The application runs indefinitely.
    loop {
        let start_time = Instant::now();
        // Aquire a sample of all sensors.
        let sensor_data = sensors.aquire_and_sleep().await;
        // Wait 60s for the next measurement.
        let sleep_duration = Duration::from_secs(60) - (Instant::now() - start_time);
        Timer::after(sleep_duration).await;
    }
}

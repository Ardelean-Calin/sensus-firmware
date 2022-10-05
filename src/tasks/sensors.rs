#[path = "../../common.rs"]
mod common;

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;

use embassy_nrf::gpiote::InputChannel;
use embassy_nrf::interrupt::{
    SAADC, SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0, SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1,
};
use embassy_nrf::peripherals::{P0_06, TWISPI0, TWISPI1};
use embassy_nrf::ppi::Ppi;
// My own drivers.
use ltr303_async::Ltr303;
use shtc3_async::Shtc3;

use futures::future::join;

use defmt::{info, *};
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive};
use embassy_nrf::twim::{self, Twim};
use embassy_nrf::{interrupt, Peripherals};
use embassy_time::{Delay, Duration, Timer};
use tmp1x2::{self, Tmp1x2};

use battery_sensor::BatterySensor;
use shared_bus;

/// Periodicity of the i2c sensor measurement.
const SENSOR_PERIOD_MS: u16 = 5000;

/// When called, samples all the i2c sensors and then drops
/// the i2c bus to save power.
async fn i2c_sensors_sample(
    i2c_bus: Twim<'_, TWISPI0>,
) -> (shtc3_async::Measurement, ltr303_async::Measurement) {
    debug!("Sampling i2c sensors...");
    // Create the I2C bus which will be dropped after sampling is done!
    let bus = shared_bus::BusManagerSimple::new(i2c_bus);
    let proxy1 = bus.acquire_i2c();
    let proxy2 = bus.acquire_i2c();

    let mut sht_sensor = Shtc3::new(proxy1);
    let mut ltr_sensor = Ltr303::new(proxy2);
    let mut delay1 = Delay;
    let mut delay2 = Delay;

    let (result_sht, result_ltr) = join(
        sht_sensor.sample(&mut delay1),
        ltr_sensor.sample(&mut delay2),
    )
    .await;

    (result_sht.unwrap(), result_ltr.unwrap())
}

/// When called, samples the battery voltage and returns its value in millivolts.
async fn battery_sensor_sample<'a>(mut sensor: BatterySensor<'a>) -> u32 {
    sensor.init().await;
    let battery_voltage = sensor.sample_mv().await;
    battery_voltage
}

struct SensorData {
    battery_voltage: u32,
    sht_data: shtc3_async::Measurement,
    ltr_data: ltr303_async::Measurement,
    soil_temperature: f32,
    soil_humidity: f32,
}

struct Sensors<'a> {
    p: &'a mut Peripherals,
    onb_i2c_irq: SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0,
    ext_i2c_irq: SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1,
    adc_irq: SAADC,
}

impl<'a> Sensors<'a> {
    fn new(p: &'a mut Peripherals) -> Self {
        let onb_i2c_irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
        let ext_i2c_irq = interrupt::take!(SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1);
        let adc_irq = interrupt::take!(SAADC);

        Self {
            p,
            onb_i2c_irq,
            ext_i2c_irq,
            adc_irq,
        }
    }

    async fn sample(&mut self) -> SensorData {
        // TODO: This will only be necessary for the external sensors.
        let mut sen = Output::new(&mut self.p.P0_06, Level::Low, OutputDrive::Standard);
        sen.set_high();
        Timer::after(Duration::from_millis(2)).await;

        let battery_sensor =
            BatterySensor::new(&mut self.p.P0_29, &mut self.p.SAADC, &mut self.adc_irq);

        // i2c bus used by onboard sensors
        let mut config = twim::Config::default();
        config.frequency = twim::Frequency::K400; // 400k seems to be best for low power consumption.
        let onboard_i2c_bus = Twim::new(
            &mut self.p.TWISPI0,
            &mut self.onb_i2c_irq,
            &mut self.p.P0_08,
            &mut self.p.P0_09,
            config,
        );

        // i2c bus used by external probe sensors
        let mut config2 = twim::Config::default();
        config2.frequency = twim::Frequency::K400; // 400k seems to be best for low power consumption.
        let external_i2c_bus = Twim::new(
            &mut self.p.TWISPI1,
            &mut self.ext_i2c_irq,
            &mut self.p.P0_14,
            &mut self.p.P0_15,
            config2,
        );

        // NOTE: join3 doesn't seem to work!
        let ((bat_voltage, sht_data, ltr_data), (soil_temp, soil_water)) = join(
            Sensors::_onboard_sensors_sample(onboard_i2c_bus, battery_sensor),
            Sensors::_external_sensors_sample(external_i2c_bus),
        )
        .await;

        // TODO: This will only be necessary for the external sensors.
        sen.set_low();

        let data = SensorData {
            sht_data,
            ltr_data,
            battery_voltage: bat_voltage,
            soil_temperature: soil_temp,
            soil_humidity: soil_water,
        };

        data
    }

    async fn _onboard_sensors_sample(
        i2c_bus: Twim<'_, TWISPI0>,
        battery_sensor: BatterySensor<'_>,
    ) -> (u32, shtc3_async::Measurement, ltr303_async::Measurement) {
        let (batt_voltage, (sht_data, ltr_data)) = join(
            battery_sensor_sample(battery_sensor),
            i2c_sensors_sample(i2c_bus),
        )
        .await;

        (batt_voltage, sht_data, ltr_data)
    }

    async fn _external_sensors_sample(i2c_bus: Twim<'_, TWISPI1>) -> (f32, f32) {
        // Sensor 1: Temperature
        let mut tmp_sensor = Tmp1x2::new(i2c_bus, tmp1x2::SlaveAddr::Default);
        // 35ms is the maximum conversion time as specified in the datasheet.
        Timer::after(Duration::from_millis(35)).await;
        let ext_tmp = tmp_sensor.read_temperature().unwrap();

        info!("Soil temperature: {}", ext_tmp);

        // Sensor 2: Soil humidity
        (ext_tmp, 69f32)
    }
}

#[embassy_executor::task]
pub async fn sensors_task() {
    let mut config = embassy_nrf::config::Config::default();
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;

    // Peripherals config
    let mut p = embassy_nrf::init(config);

    // Hardware timer(s) used by the probe
    // Let's bind the GPIO19 falling edge event to the counter's count up event.
    // In parallel, we start a normal 1MHz timer.
    // Then, after 100ms we get the value of CC in the counter, therefore giving us the number of events per 100ms
    let freq_in = InputChannel::new(
        p.GPIOTE_CH0,
        Input::new(p.P0_19, embassy_nrf::gpio::Pull::Up),
        embassy_nrf::gpiote::InputChannelPolarity::HiToLo,
    );
    let mut counter = embassy_nrf::timer::Timer::new(p.TIMER0);
    let mut ppi = Ppi::new_one_to_one(p.PPI_CH0, freq_in.event_in(), counter.task_count());
    ppi.enable();
    counter.start();

    let mut sen = Output::new(p.P0_06, Level::Low, OutputDrive::Standard);
    sen.set_high();
    Timer::after(Duration::from_millis(2)).await;

    let mut prev = 0u32;
    loop {
        let cc = counter.cc(0).capture();
        info!("Freq: {}Hz\tCC: {}", cc - prev, cc);
        prev = cc;
        Timer::after(Duration::from_millis(1000)).await;
    }
    // timer.cc(0).write(value).

    // let mut sensors = Sensors::new(&mut p);

    // loop {
    //     let data = sensors.sample().await;

    //     info!("Battery voltage: {}mV", data.battery_voltage);
    //     info!("LTR measurement result: {}", data.ltr_data);
    //     info!("SHT measurement result: {}", data.sht_data);
    //     info!("Soil temperature: {}C", data.soil_temperature);
    //     info!("Soil water content: {}%", data.soil_humidity);

    //     // I know I don't include the measurement time here, but I don't really need to be super precise...
    //     Timer::after(Duration::from_millis(SENSOR_PERIOD_MS as u64)).await;
    // }
}

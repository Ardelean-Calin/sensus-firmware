#[path = "../../common.rs"]
mod common;

#[path = "../drivers/battery_sensor.rs"]
mod battery_sensor;

use defmt::{info, *};
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::saadc::Input;
use embassy_time::{Duration, Timer};
// use futures::future::join3;

use battery_sensor::BatterySensor;

struct Pins<T> {
    adc_in: T,
    i2c_sda: T,
    i2c_scl: T,
    sen: T,
}
struct Sensors<'a> {
    // pins: Pins,
    batt_sensor: Option<BatterySensor<'a>>,
}

impl<'a> Sensors<'a> {
    /// TODO: Maybe add a pins configuration to this constructor.
    /// Something along the lines of:
    ///
    /// Pins {
    ///     adc_in: P0_29,
    ///     i2c_sda: P0_08,
    ///     i2c_scl: P0_09,
    ///     sen: P0_06,
    /// }
    fn new() -> Self {
        Sensors { batt_sensor: None }
    }

    // Init all sensors
    pub async fn init(&mut self) {
        let p = embassy_nrf::init(Default::default());

        let adc_pin = p.P0_29.degrade_saadc();
        let batt = BatterySensor::new(adc_pin, p.SAADC);
        batt.calibrate().await;

        self.batt_sensor = Some(batt);
    }

    pub fn power_enable(&self) {}
    pub fn power_disable(&self) {}
}

// struct SensorData {
//     timestamp_s: u32, // Timestamp in seconds. Up to 136 years.
//     air_temp: u16,    // 100 millikelvin per unit <=> 0K to 6553K
//     air_relhum: u16,  // 100 millipercent per unit <=> 0.0% to 100.0%
//     irradiance: u16,  // Solar irradiance in Lux
//     soil_temp: u16,   // 100 millikelvin per unit <=> 0K to 6553K
//     soil_freq: u32,   // frequency for now, percentages in the future.
// }

// impl Default for SensorData {
//     fn default() -> Self {
//         Self {
//             timestamp_s: 0,
//             air_temp: 0,
//             air_relhum: 0,
//             irradiance: 0,
//             soil_temp: 0,
//             soil_freq: 0,
//         }
//     }
// }

#[embassy_executor::task]
pub async fn sensors_task() {
    let mut sensors = Sensors::new();
    sensors.init().await;
    let mut batt_sensor = unwrap!(sensors.batt_sensor);
    // let mut sen = Output::new(p.P0_06, Level::Low, OutputDrive::Standard);
    loop {
        // Enable power to the sensors.
        // sen.set_high();
        // 0 -> 3.3V in less than 500us. Measured on oscilloscope. So I will set 2 ms just to be sure.
        Timer::after(Duration::from_millis(2)).await;

        // let (battery_data, shtc3_data) = join!(battery_sample(), shtc3_sample()).await;
        // Wait for all data aquisition to finish and then deconstruct the data
        // let (shtc3_data, ltr303_data, soil_probe_data) =
        //     futures::future::join!(shtc3_sample, ltr303_sample, soil_probe_sample).await;

        batt_sensor.sample().await;
        // info!("sensors_task tick!");
        Timer::after(Duration::from_millis(500)).await;

        // Disable power to the sensors.
        // sen.set_low();
    }
}

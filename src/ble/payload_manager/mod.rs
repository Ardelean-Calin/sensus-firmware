use crate::sensors::types::SensorDataFiltered;
use crate::sensors::LATEST_SENSOR_DATA;

use embassy_futures::select::{select, Either};

use crate::ble::types::BTHomeAD;
use crate::globals::{BTHOME_QUEUE, ONBOARD_DATA_SIG, PROBE_DATA_SIG};

/// This loop receives data from different parts of the program and packs this data
/// into an BTHomeAD. Then it sends this payload to be processed.
async fn payload_mgr_loop() {
    let mut bthome_payload = BTHomeAD::default();

    let mut sensordata = SensorDataFiltered::default();

    // TODO.
    //
    // This "payload builder" can be a separate "filter" module that waits for incoming
    // probe or onboard data and updates all related structures: Latest Sensor Data, Advertisment
    // data, etc.
    loop {
        // Wait for either new onboard data or new probe data.
        bthome_payload = match select(ONBOARD_DATA_SIG.wait(), PROBE_DATA_SIG.wait()).await {
            Either::First(data) => {
                sensordata.feed_onboard(data);

                // Return a new advertisment payload.
                let filtered = sensordata.get_onboard();
                bthome_payload
                    .air_humidity((filtered.environment_data.humidity as u8).into())
                    .air_temperature(((filtered.environment_data.temperature * 10.0) as i16).into())
                    .illuminance(((filtered.environment_data.illuminance * 100.0) as u32).into())
                    .battery_level(((filtered.battery_level.value * 1000.0) as u16).into())
                    .battery_low((filtered.battery_level.value <= 2.4f32).into())
            }
            Either::Second(data) => {
                sensordata.feed_probe(data);
                let filtered = sensordata.get_probe();

                // Return a new advertisment payload.
                bthome_payload
                    .soil_temperature(((filtered.temperature * 10.0) as i16).into())
                    .soil_humidity((filtered.moisture as u8).into())
            }
        };
        // Replace the latest sensor data with the filtered one.
        LATEST_SENSOR_DATA
            .lock()
            .await
            .replace(sensordata.get_raw());

        // This call is debounced by the BLE state machine.
        BTHOME_QUEUE.send(bthome_payload.clone()).await;
    }
}

#[embassy_executor::task]
pub async fn payload_mgr_task() {
    payload_mgr_loop().await;
}

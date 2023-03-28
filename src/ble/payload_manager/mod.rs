use crate::power_manager::PLUGGED_IN_FLAG;
use crate::sensors::types::SensorDataFiltered;
use crate::sensors::LATEST_SENSOR_DATA;

use embassy_futures::select::{select, Either};
use embassy_time::Instant;

use crate::ble::types::AdvertismentPayload;
use crate::globals::{BLE_ADV_PKT_QUEUE, ONBOARD_DATA_SIG, PROBE_DATA_SIG};

/// This loop receives data from different parts of the program and packs this data
/// into an AdvertismentPayload. Then it sends this payload to be processed.
async fn payload_mgr_loop() {
    let mut adv_payload = AdvertismentPayload::default();

    let mut sensordata = SensorDataFiltered::default();

    // TODO.
    //
    // This "payload builder" can be a separate "filter" module that waits for incoming
    // probe or onboard data and updates all related structures: Latest Sensor Data, Advertisment
    // data, etc.
    loop {
        // Wait for either new onboard data or new probe data.
        adv_payload = match select(ONBOARD_DATA_SIG.wait(), PROBE_DATA_SIG.wait()).await {
            Either::First(data) => {
                sensordata.feed_onboard(data);

                // Return a new advertisment payload.
                adv_payload.with_onboard_data(sensordata.get_onboard())
            }
            Either::Second(data) => {
                sensordata.feed_probe(data);

                // Return a new advertisment payload.
                adv_payload.with_probe_data(sensordata.get_probe())
            }
        };
        // Replace the latest sensor data with the filtered one.
        LATEST_SENSOR_DATA.lock().await.replace(sensordata.clone());

        adv_payload = adv_payload.with_uptime(Instant::now());
        adv_payload = adv_payload
            .with_plugged_in(PLUGGED_IN_FLAG.load(core::sync::atomic::Ordering::Relaxed));
        // This call is debounced by the BLE state machine.
        BLE_ADV_PKT_QUEUE.send(adv_payload).await;
    }
}

#[embassy_executor::task]
pub async fn payload_mgr_task() {
    payload_mgr_loop().await;
}

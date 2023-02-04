pub mod types;

use embassy_sync::waitqueue::AtomicWaker;
use embassy_time::{Duration, Ticker};
use futures::StreamExt;

use crate::common::types::Filter;
use crate::drivers::ble::types::AdvertismentPayload;
use crate::drivers::onboard::{
    battery::types::BatteryLevel, environment::types::EnvironmentSample,
};
use crate::drivers::probe::types::ProbeSample;
use crate::state_machines::ble::BLE_ADV_PKT_QUEUE;
use crate::state_machines::onboard::ONBOARD_DATA;
use crate::state_machines::probe::PROBE_DATA;

pub static PKT_BLD_WAKER: AtomicWaker = AtomicWaker::new();

pub async fn run() {
    let mut adv_payload = AdvertismentPayload::default();

    let mut env_filter = Filter::<EnvironmentSample>::default();
    let mut bat_filter = Filter::<BatteryLevel>::new(0.181);
    let mut probe_filter = Filter::<ProbeSample>::default();

    let ble_pub = defmt::unwrap!(BLE_ADV_PKT_QUEUE.publisher());

    let mut ticker = Ticker::every(Duration::from_secs(1));

    loop {
        ticker.next().await;
        // let data = ONBOARD_DATA.lock().await;
        // if let Some(s) = *data {
        //     let env_data = env_filter.feed(s.environment_data);
        //     let bat_data = bat_filter.feed(s.battery_level);
        //     adv_payload = AdvertismentPayload {
        //         battery_level: Some(bat_data.value.into()),
        //         air_temperature: Some(env_data.temperature.into()),
        //         air_humidity: Some(env_data.humidity.into()),
        //         illuminance: Some(env_data.illuminance.into()),
        //         uptime: Some((s.current_time.as_secs() as f32).into()),
        //         ..adv_payload
        //     }
        // } else {
        //     // The SM has set our data to None, that means some error has taken place.
        //     adv_payload = AdvertismentPayload {
        //         battery_level: None,
        //         air_temperature: None,
        //         air_humidity: None,
        //         illuminance: None,
        //         uptime: None,
        //         ..adv_payload
        //     }
        // }

        // let data = PROBE_DATA.lock().await;
        // if let Some(s) = *data {
        //     let sample = probe_filter.feed(s);
        //     adv_payload = AdvertismentPayload {
        //         soil_temperature: Some(sample.temperature.into()),
        //         soil_humidity: Some(sample.moisture.into()),
        //         ..adv_payload
        //     }
        // } else {
        //     // The SM has set our data to None, that means some error has taken place.
        //     adv_payload = AdvertismentPayload {
        //         soil_temperature: None,
        //         soil_humidity: None,
        //         ..adv_payload
        //     }
        // }

        // TODO: Maybe do something in case of error.
        // let _ = ble_pub.try_publish(adv_payload);
        // ticker.next().await;
    }
}

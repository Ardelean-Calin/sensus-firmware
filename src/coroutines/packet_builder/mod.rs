pub mod types;
use types::OnboardFilter;

use embassy_futures::select::{select, Either};
use embassy_time::Instant;

use crate::common::types::Filter;
use crate::drivers::ble::types::AdvertismentPayload;
use crate::drivers::probe::types::ProbeSample;
use crate::globals::{BLE_ADV_PKT_QUEUE, ONBOARD_DATA_SIG, PROBE_DATA_SIG};

pub async fn run() {
    let mut adv_payload = AdvertismentPayload::default();

    let mut onboard_filter = OnboardFilter::default();
    let mut probe_filter = Filter::<ProbeSample>::default();

    loop {
        // Wait for either new onboard data or new probe data.
        adv_payload = match select(ONBOARD_DATA_SIG.wait(), PROBE_DATA_SIG.wait()).await {
            Either::First(data) => {
                let data = onboard_filter.feed(data);
                // Return a new advertisment payload.
                adv_payload.with_onboard_data(data)
            }
            Either::Second(data) => {
                let data = probe_filter.feed(data);
                // Return a new advertisment payload.
                adv_payload.with_probe_data(data)
            }
        };
        adv_payload = adv_payload.with_uptime(Instant::now());
        // This call is debounced by the BLE state machine.
        BLE_ADV_PKT_QUEUE.send(adv_payload).await;
    }
}

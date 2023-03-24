use crate::sensors::types::OnboardFilter;
use crate::sensors::types::ProbeFilter;

use embassy_futures::select::{select, Either};
use embassy_time::Instant;

use crate::ble::types::AdvertismentPayload;
use crate::globals::{BLE_ADV_PKT_QUEUE, ONBOARD_DATA_SIG, PROBE_DATA_SIG};
use crate::PLUGGED_IN_FLAG;

pub async fn run() {
    let mut adv_payload = AdvertismentPayload::default();

    let mut onboard_filter = OnboardFilter::default();
    let mut probe_filter = ProbeFilter::default();

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
        adv_payload = adv_payload
            .with_plugged_in(PLUGGED_IN_FLAG.load(core::sync::atomic::Ordering::Relaxed));
        // This call is debounced by the BLE state machine.
        BLE_ADV_PKT_QUEUE.send(adv_payload).await;
    }
}

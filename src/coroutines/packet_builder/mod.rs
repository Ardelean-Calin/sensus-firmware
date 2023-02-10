pub mod types;

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Instant;

use crate::common::types::Filter;
use crate::drivers::ble::types::AdvertismentPayload;
use crate::drivers::onboard::types::OnboardSample;
use crate::drivers::onboard::{
    battery::types::BatteryLevel, environment::types::EnvironmentSample,
};
use crate::drivers::probe::types::ProbeSample;
use crate::state_machines::ble::BLE_ADV_PKT_QUEUE;

pub static ONBOARD_DATA_SIG: Signal<ThreadModeRawMutex, OnboardSample> = Signal::new();
pub static PROBE_DATA_SIG: Signal<ThreadModeRawMutex, ProbeSample> = Signal::new();

pub async fn run() {
    let mut adv_payload = AdvertismentPayload::default();

    let mut env_filter = Filter::<EnvironmentSample>::default();
    let mut bat_filter = Filter::<BatteryLevel>::new(0.181);
    let mut probe_filter = Filter::<ProbeSample>::default();

    let ble_pub = defmt::unwrap!(BLE_ADV_PKT_QUEUE.publisher());

    loop {
        adv_payload = match select(ONBOARD_DATA_SIG.wait(), PROBE_DATA_SIG.wait()).await {
            Either::First(data) => adv_payload.with_onboard_data(data),
            Either::Second(data) => adv_payload.with_probe_data(data),
        };
        adv_payload = adv_payload.with_uptime(Instant::now());
        ble_pub.publish_immediate(adv_payload);
    }
}

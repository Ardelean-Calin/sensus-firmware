pub mod types;

use defmt::unwrap;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, pubsub::PubSubChannel};
use futures::{future::select, pin_mut};
use nrf_softdevice::Softdevice;

use crate::drivers::ble::{
    gatt,
    types::{AdvertismentData, AdvertismentPayload},
};
use types::{BleSM, BleSMState};

pub static BLE_ADV_PKT_QUEUE: PubSubChannel<ThreadModeRawMutex, AdvertismentPayload, 1, 1, 1> =
    PubSubChannel::new();

pub async fn run(sd: &'static Softdevice, server: gatt::Server) {
    let mut sm = BleSM::new();
    let mut subscriber = unwrap!(BLE_ADV_PKT_QUEUE.subscriber());
    let mut current_adv_data = AdvertismentData::default();
    loop {
        match sm.state {
            BleSMState::Advertising => {
                let adv_data_vec = current_adv_data.as_vec();
                let new_adv_pkt_fut = subscriber.next_message_pure();
                pin_mut!(new_adv_pkt_fut);
                let gatt_server_fut = gatt::run_gatt_server(sd, &server, adv_data_vec.as_slice());
                pin_mut!(gatt_server_fut);

                match select(new_adv_pkt_fut, gatt_server_fut).await {
                    futures::future::Either::Left((payload, _)) => {
                        // A venit un payload nou.
                        defmt::info!("Got new payload. Updating advertising data.");
                        current_adv_data = AdvertismentData::default().with_payload(payload);
                        sm = sm.with_state(BleSMState::Advertising);
                    }
                    futures::future::Either::Right((_res, _)) => {
                        defmt::error!("GATT error");
                        sm = sm.with_state(BleSMState::GattDisconnected);
                    }
                }
            }
            BleSMState::GattDisconnected => {
                defmt::error!("GATT server disconnected. ");
                sm = sm.with_state(BleSMState::Advertising);
            }
        }
    }
}

pub mod types;

use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel, signal::Signal};
use embassy_time::{with_timeout, Duration};
use nrf_softdevice::Softdevice;

use crate::drivers::ble::{
    gatt,
    types::{AdvertismentData, AdvertismentPayload},
};
use types::{BleSM, BleSMState};

pub static BLE_ADV_PKT_QUEUE: Channel<ThreadModeRawMutex, AdvertismentPayload, 1> = Channel::new();
/// Signals new advertising data between the two "threads".
static ADV_DATA: Signal<ThreadModeRawMutex, AdvertismentData> = Signal::new();

pub async fn gatt_spawner(sd: &'static Softdevice, server: gatt::Server) {
    let mut advdata = AdvertismentData::default();
    loop {
        let advdata_vec = advdata.as_vec();

        match select(
            ADV_DATA.wait(),
            gatt::run_gatt_server(sd, &server, advdata_vec),
        )
        .await
        {
            Either::First(newdata) => {
                advdata = newdata;
                defmt::info!("New Advdata: {:?}", advdata);
            }
            Either::Second(_e) => {
                defmt::info!("Gatt server terminated.");
            }
        }
    }
}

pub async fn run() {
    let mut sm = BleSM::new();
    let mut current_adv_data = AdvertismentData::default();
    let mut packet_id = 0x00u8;

    loop {
        match sm.state {
            BleSMState::WaitForAdvdata => {
                let payload = BLE_ADV_PKT_QUEUE.recv().await.with_packet_id(packet_id);
                current_adv_data = current_adv_data.with_payload(payload);
                sm = sm.with_state(BleSMState::Debounce);
            }
            // Debounce new received data so that I don't publish more often than every 250ms
            BleSMState::Debounce => {
                match with_timeout(Duration::from_millis(250), async {
                    let payload = BLE_ADV_PKT_QUEUE.recv().await;
                    current_adv_data.with_payload(payload)
                })
                .await
                {
                    Ok(newdata) => {
                        // New data came before timeout. Don't change anything
                        current_adv_data = newdata;
                    }
                    Err(_e) => {
                        // Timeout occured, so we debounced the received messages. We can go to the next state.
                        packet_id += 1;
                        sm = sm.with_state(BleSMState::Advertising);
                    }
                }
            }
            BleSMState::Advertising => {
                ADV_DATA.signal(current_adv_data);
                sm = sm.with_state(BleSMState::WaitForAdvdata);
            }
            BleSMState::GattDisconnected => {
                defmt::error!("GATT server disconnected. ");
                sm = sm.with_state(BleSMState::Advertising);
            }
        }
    }
}

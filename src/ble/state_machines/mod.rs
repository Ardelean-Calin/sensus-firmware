pub mod types;

use embassy_time::{with_timeout, Duration};

use crate::config_manager::SENSUS_CONFIG;
use crate::globals::BLE_ADV_PKT_QUEUE;
use types::{BleSM, BleSMState};

use crate::ble::types::AdvertismentData;
use crate::ble::ADV_DATA;

/// Runst the Bluetooth state machine. This state machine waits for new data to be published and publishes said data
/// via Extended Advertisments.
pub async fn run() {
    let mut sm = BleSM::new();
    let mut current_adv_data = AdvertismentData::default();
    let mut packet_id = 0x00u8;

    loop {
        match sm.state {
            BleSMState::Startup => {
                // For startup, we load the Advertisment name from config.
                let config = SENSUS_CONFIG.lock().await.clone().unwrap_or_default();
                let ble_name = config.name;
                current_adv_data.set_name(ble_name);
                sm = sm.with_state(BleSMState::WaitForAdvdata);
            }
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
                ADV_DATA.signal(current_adv_data.clone());
                sm = sm.with_state(BleSMState::WaitForAdvdata);
            }
            BleSMState::GattDisconnected => {
                defmt::error!("GATT server disconnected. ");
                sm = sm.with_state(BleSMState::Advertising);
            }
        }
    }
}

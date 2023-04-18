pub mod types;

use embassy_time::{with_timeout, Duration};

use crate::config_manager::SENSUS_CONFIG;
use crate::globals::BTHOME_QUEUE;
use types::{BleSM, BleSMState};

use crate::ble::types::AdvertismentData;
use crate::ble::ADV_DATA;

/// Runst the Bluetooth state machine. This state machine waits for new data to be published and publishes said data
/// via Extended Advertisments.
pub async fn run() {
    let mut sm = BleSM::new();
    let mut current_adv_data = AdvertismentData::default();

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
                let bthome_ad = BTHOME_QUEUE.recv().await;
                current_adv_data = current_adv_data.with_bthome(bthome_ad);
                sm = sm.with_state(BleSMState::Debounce);
            }
            // Debounce new received data so that I don't publish more often than every 250ms
            BleSMState::Debounce => {
                match with_timeout(Duration::from_millis(250), async {
                    let bthome_ad = BTHOME_QUEUE.recv().await;
                    current_adv_data.with_bthome(bthome_ad)
                })
                .await
                {
                    Ok(newdata) => {
                        // New data came before timeout. Don't change anything
                        current_adv_data = newdata;
                    }
                    Err(_e) => {
                        // Timeout occured, so we debounced the received messages. We can go to the next state.
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

use embassy_futures::select::{select, Either};
use heapless::Vec;
use nrf_softdevice::{
    ble::{
        peripheral::{self, AdvertiseError},
        TxPower,
    },
    Softdevice,
};

use crate::ble::types::AdvertismentData;
use crate::ble::ADV_DATA;

async fn run_advertisments<'a>(
    sd: &'static Softdevice,
    adv_data: Vec<u8, 64>,
) -> Result<(), AdvertiseError> {
    let config = nrf_softdevice::ble::peripheral::Config {
        interval: 1600, // equivalent to 1000ms
        tx_power: TxPower::Plus4dBm,
        ..Default::default()
    };

    let adv = peripheral::NonconnectableAdvertisement::ExtendedNonscannableUndirected {
        set_id: 0,
        adv_data: adv_data.as_slice(),
        anonymous: false,
    };
    // For now we will advertise as non-connectable.
    peripheral::advertise(sd, adv, &config).await
}

/// Starts the advertising loop. This loop watches for changes to ADV_DATA and publishes those new
/// changes via extended advertisments.
pub async fn advertisment_loop(sd: &'static Softdevice) {
    let mut advdata = AdvertismentData::default();
    loop {
        let advdata_vec = advdata.as_vec();

        match select(ADV_DATA.wait(), run_advertisments(sd, advdata_vec)).await {
            Either::First(newdata) => {
                advdata = newdata;
                // defmt::info!("New Advdata: {:?}", advdata);
            }
            Either::Second(_e) => {
                defmt::error!("Advertisment error.");
            }
        }
    }
}

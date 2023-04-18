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

async fn start_advertising<'a>(
    sd: &'static Softdevice,
    bthome_ad_element: Vec<u8, 31>,
    name_ad_element: Vec<u8, 31>,
) -> Result<(), AdvertiseError> {
    let config = nrf_softdevice::ble::peripheral::Config {
        interval: 1600, // equivalent to 1000ms
        tx_power: TxPower::Plus4dBm,
        ..Default::default()
    };

    let adv = peripheral::NonconnectableAdvertisement::ScannableUndirected {
        adv_data: bthome_ad_element.as_slice(), // The maximum size for Advertisment and Scan data is 31 bytes.
        scan_data: name_ad_element.as_slice(),
    };
    // TODO. In case I plan to return to extended advertising...
    // #[cfg(feature = "extended-advertising")]
    // let adv_data: Vec<u8, 62> = bthome_ad_element
    //     .into_iter()
    //     .chain(name_ad_element.into_iter())
    //     .collect();
    // #[cfg(feature = "extended-advertising")]
    // let adv = peripheral::NonconnectableAdvertisement::ExtendedNonscannableUndirected {
    //     set_id: 0,
    //     adv_data: adv_data.as_slice(),
    //     anonymous: false,
    // };
    // For now we will advertise as non-connectable.
    peripheral::advertise(sd, adv, &config).await
}

/// Starts the advertising loop. This loop watches for changes to ADV_DATA and publishes those new
/// changes via extended advertisments.
pub async fn advertisment_loop(sd: &'static Softdevice) {
    let mut advdata = AdvertismentData::default();
    loop {
        let bthome_ad = advdata.get_ad_bthome();
        // TODO. I need to somehow make it so that I detect at compile time AD elements longer than 31.
        defmt::trace!("BTHome AD length: {:?}", bthome_ad.len());
        let name_ad = advdata.get_ad_localname();

        match select(ADV_DATA.wait(), start_advertising(sd, bthome_ad, name_ad)).await {
            Either::First(newdata) => {
                advdata = newdata;
                defmt::trace!("New Advdata: {:?}", advdata);
            }
            Either::Second(_e) => {
                defmt::error!("Advertisment error.");
            }
        }
    }
}

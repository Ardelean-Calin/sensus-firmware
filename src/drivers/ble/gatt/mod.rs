use defmt::{info, unwrap};
use heapless::Vec;
use nrf_softdevice::{
    ble::{
        gatt_server::{self, RunError},
        peripheral, TxPower,
    },
    raw, Softdevice,
};

#[nrf_softdevice::gatt_service(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a600323d")]
pub struct MainService {
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a601323d", read, notify)]
    all_data: [u8; 14],
    // #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a605323d", read, notify)]
    // flags: u8, // Different flags. Charging, plugged in, etc.
}

#[nrf_softdevice::gatt_server]
pub struct Server {
    pub main: MainService,
    // control: ControlService,
}

pub async fn run_gatt_server<'a>(
    sd: &'static Softdevice,
    server: &'a Server,
    adv_data: Vec<u8, 64>,
) -> Result<(), RunError> {
    let config = nrf_softdevice::ble::peripheral::Config {
        interval: 1600, // equivalent to 1000ms
        tx_power: TxPower::Plus4dBm,
        ..Default::default()
    };

    let adv = peripheral::ConnectableAdvertisement::ExtendedNonscannableUndirected {
        set_id: 0,
        adv_data: adv_data.as_slice(),
    };
    let conn = unwrap!(peripheral::advertise_connectable(sd, adv, &config).await);
    info!("Advertising done! Got a connection, trying to negociate higher connection intervals.");
    let conn_params = raw::ble_gap_conn_params_t {
        min_conn_interval: 100, // 1.25ms units
        max_conn_interval: 400, // 1.25ms units
        slave_latency: 0,
        conn_sup_timeout: 400, // 4s
    };
    conn.set_conn_params(conn_params).unwrap();

    // Run the GATT server on the connection. This returns when the connection gets disconnected.
    gatt_server::run(&conn, server, |_e| {}).await
}

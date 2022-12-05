use defmt::{assert_eq, info, *};
use futures::pin_mut;

use nrf_softdevice::ble::gatt_server::SetValueError;
use nrf_softdevice::ble::{gatt_server, peripheral, Connection, TxPower};
use nrf_softdevice::{raw, Softdevice};
use raw::{sd_power_dcdc_mode_set, NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE};

use crate::app::{DataPacket, SENSOR_DATA_BUS};

use core::mem;

#[nrf_softdevice::gatt_service(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a600323d")]
pub struct MainService {
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a601323d", read, notify)]
    all_data: [u8; 14],
    // #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a605323d", read, notify)]
    // flags: u8, // Different flags. Charging, plugged in, etc.
}

#[nrf_softdevice::gatt_service(uuid = "3913a152-fffb-45af-95f6-9177d2005e39")]
struct ControlService {}

#[nrf_softdevice::gatt_server]
pub struct Server {
    pub main: MainService,
    // control: ControlService,
}

#[embassy_executor::task]
pub async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

fn update_main_char(
    server: &Server,
    conn: &Connection,
    data: &DataPacket,
) -> Result<(), SetValueError> {
    server.main.all_data_set(data.to_bytes_array())?;
    let _ = server.main.all_data_notify(conn, data.to_bytes_array());
    Ok(())
}

async fn update_gatt(server: &Server, connection: &Connection) {
    let mut data_subscriber = unwrap!(SENSOR_DATA_BUS.subscriber());
    loop {
        // Blocks until the application task (TODO change to "data aquisition task") publishes new data.
        let sensor_data = data_subscriber.next_message_pure().await;
        info!("BLE got new data: {:?}", sensor_data);
        update_main_char(server, connection, &sensor_data).unwrap();
    }
}

pub fn configure_ble<'a>() -> (&'a mut Softdevice, Server) {
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 0,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 1,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: raw::BLE_GATTS_ATTR_TAB_SIZE_DEFAULT.into(),
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: raw::BLE_GAP_ADV_SET_COUNT_DEFAULT as u8,
            periph_role_count: raw::BLE_GAP_ROLE_COUNT_PERIPH_DEFAULT as u8,
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: b"RustyBuddy" as *const u8 as _,
            current_len: 10,
            max_len: 10,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);
    // Enable DC/DC converter for the Softdevice.
    unsafe {
        let ret = sd_power_dcdc_mode_set(NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE as u8);
        assert_eq!(ret, 0, "Error when enabling DC/DC converter: {}", ret);
    }

    let server = unwrap!(Server::new(sd));

    (sd, server)
}

pub async fn run_ble_application(sd: &Softdevice, server: Server) {
    #[rustfmt::skip]
    let adv_data = &[
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        0x03, 0x03, 0x09, 0x18,
        0x0a, 0x09, b'R', b'u', b's', b't', b'y', b'B', b'u', b't', b't',
        // 0x0a, 0x09, b'H', b'e', b'l', b'l', b'o', b'R', b'u', b's', b't',
    ];
    #[rustfmt::skip]
    let scan_data = &[
        0x03, 0x03, 0x09, 0x18,
    ];

    loop {
        let mut config = peripheral::Config::default();
        // equivalent to 500ms
        config.interval = 800;
        config.tx_power = TxPower::Plus4dBm;

        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data,
            scan_data,
        };
        let conn = unwrap!(peripheral::advertise_connectable(sd, adv, &config).await);
        info!(
            "Advertising done! Got a connection, trying to negociate higher connection intervals."
        );
        let conn_params = raw::ble_gap_conn_params_t {
            min_conn_interval: 100, // 1.25ms units
            max_conn_interval: 400, // 1.25ms units
            slave_latency: 0,
            conn_sup_timeout: 400, // 4s
        };

        conn.set_conn_params(conn_params).unwrap();

        // Run the GATT server on the connection. This returns when the connection gets disconnected.
        let gatt_server_fut = gatt_server::run(&conn, &server, |_e| {});
        let gatt_update_fut = update_gatt(&server, &conn);

        pin_mut!(gatt_server_fut);
        pin_mut!(gatt_update_fut);

        let res = futures::future::select(gatt_server_fut, gatt_update_fut).await;

        match res {
            futures::future::Either::Left((gatt_res, _)) => {
                if let Err(e) = gatt_res {
                    info!("gatt_server run exited with error: {:?}", e);
                }
            }
            futures::future::Either::Right(_) => {}
        }
    }
}

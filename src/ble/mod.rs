use defmt::{assert_eq, info, *};
use futures::future::select;
use futures::pin_mut;

use nrf_softdevice::ble::gatt_server::SetValueError;
use nrf_softdevice::ble::{gatt_server, peripheral, Connection, TxPower};
use nrf_softdevice::{raw, Softdevice};
use raw::{sd_power_dcdc_mode_set, NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE};

use crate::tasks::app::SENSOR_DATA_BUS;
use crate::tasks::sensors::types::DataPacket;

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
        // update_main_char(server, connection, &sensor_data).unwrap();
    }
}

pub fn configure_ble<'a>() -> (&'a mut Softdevice, Server) {
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16, // Note: shorturl.at/jlvHO
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 1,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: raw::BLE_GATTS_ATTR_TAB_SIZE_DEFAULT,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: raw::BLE_GAP_ADV_SET_COUNT_DEFAULT as u8,
            periph_role_count: raw::BLE_GAP_ROLE_COUNT_PERIPH_DEFAULT as u8,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: b"RustyButt" as *const u8 as _,
            current_len: 9,
            max_len: 9,
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

async fn run_gatt_server<'a>(sd: &'static Softdevice, server: &'a Server, adv_data: &'a [u8]) {
    let mut config = peripheral::Config::default();
    // equivalent to 1000ms
    config.interval = 1600;
    config.tx_power = TxPower::Plus4dBm;

    let adv = peripheral::ConnectableAdvertisement::ExtendedNonscannableUndirected {
        set_id: 0,
        adv_data,
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
    let gatt_server_fut = gatt_server::run(&conn, server, |_e| {});
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

pub async fn run_ble_application(sd: &'static Softdevice, server: &Server) {
    #[rustfmt::skip]
    // Each line is an Advertising Data (AD) element.
    // The first byte in each line is the length of the AD element.
    // The second byte is the type of AD element as per: https://btprodspecificationrefs.blob.core.windows.net/assigned-numbers/Assigned%20Number%20Types/Generic%20Access%20Profile.pdf
    // The rest of the bytes is the payload.
    // Failing to have a payload with the exact size as the one specified in the first byte - 1 will lead to InvalidLength errors.
    let adv_data = &mut[
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8, // Flags
        0x19, 0x16, 0xD2, 0xFC, 0x40, // The BTHome AD element. Has a length of 25 bytes. 0xD2FC is the reserved UUID for BTHome
            // My actual data. placeholder for now. To be filled later
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 
            0xFF, 
        0x0A, 0x09, b'K', b'u', b's', b't', b'y', b'B', b'u', b't', b't',
    ];

    let mut data_subscriber = SENSOR_DATA_BUS.subscriber().unwrap();
    let mut data = data_subscriber.next_message_pure().await;
    loop {
        // Transform data in advertisment data
        adv_data[8..29].clone_from_slice(&bthome_format_data(&data));

        let new_data_fut = data_subscriber.next_message_pure();
        pin_mut!(new_data_fut);
        let run_gatt_server_fut = run_gatt_server(sd, server, adv_data);
        pin_mut!(run_gatt_server_fut);
        match select(new_data_fut, run_gatt_server_fut).await {
            futures::future::Either::Left((new_data, _)) => {
                data = new_data;
            }
            futures::future::Either::Right(_) => {
                // This future should never complete.
            }
        };
    }
}

fn bthome_format_data(data: &DataPacket) -> [u8; 21] {
    // How many bytes total? 3 per measurement + 4 for the soil humidity.
    // That means:
    //      3 battery voltage    +
    //      3 air temp           +
    //      3 air hum            +
    //      4 illuminance        +
    //      3 soil temperature   +
    //      5 soil moisture(count)
    //
    //   = 20 Bytes
    let mut buf = [0u8; 21];
    buf[0] = 0x0C; // BTHome Code for battery voltage in 1mV units
    buf[1..3].clone_from_slice(&data.battery_voltage.to_le_bytes());
    buf[3] = 0x02; // BTHome Code for temperature in 0.01 degree units
    buf[4..6].clone_from_slice(&data.env_data.get_air_temp().to_le_bytes());
    buf[6] = 0x03; // BTHome Code for humidity in 0.01 % units
    buf[7..9].clone_from_slice(&data.env_data.get_air_humidity().to_le_bytes());
    let illuminance: u32 = (data.env_data.get_illuminance() as u32) * 100;
    buf[9] = 0x05; // BTHome Code for illuminance in 0.01 lux units
    buf[10..13].clone_from_slice(&illuminance.to_le_bytes()[0..3]); // only 3 bytes are used
    buf[13] = 0x02; // Second temperature, probe temperature this time
    buf[14..16].clone_from_slice(&data.probe_data.soil_temperature.to_le_bytes());
    buf[16] = 0x3E;
    buf[17..21].clone_from_slice(&data.probe_data.soil_moisture.to_le_bytes());

    buf
}

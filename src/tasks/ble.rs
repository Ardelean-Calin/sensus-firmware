#[path = "../../common.rs"]
mod common;

use core::mem;

use defmt::{assert_eq, info, *};

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use futures::pin_mut;
use nrf_softdevice::ble::gatt_server::{NotifyValueError, SetValueError};
use nrf_softdevice::ble::{gatt_server, peripheral, Connection, TxPower};
use nrf_softdevice::{raw, Softdevice};
use raw::{sd_power_dcdc_mode_set, NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE};

use crate::app::SensorData;
use crate::SENSOR_DATA;

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[nrf_softdevice::gatt_service(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a600323d")]
struct MainService {
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a601323d", read, notify)]
    battery_level: u16,
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a602323d", read, notify)]
    temperature: i32,
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a603323d", read, notify)]
    humidity: u32,
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a604323d", read, notify)]
    illuminance: u16,
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a605323d", read, notify)]
    flags: u8, // Different flags. Charging, plugged in, etc.
}

#[nrf_softdevice::gatt_service(uuid = "08a6f86b-0bed-43fd-ad64-bc1c82003301")]
struct ProbeService {
    #[characteristic(uuid = "08a6f86b-0bed-43fd-ad64-bc1c82013301", read, notify)]
    frequency: u32,
    #[characteristic(uuid = "08a6f86b-0bed-43fd-ad64-bc1c82023301", read, notify)]
    temperature: f32,
}

#[nrf_softdevice::gatt_service(uuid = "3913a152-fffb-45af-95f6-9177d2005e39")]
struct ControlService {}

#[nrf_softdevice::gatt_server]
struct Server {
    main: MainService,
    probe: ProbeService,
    // control: ControlService,
}

fn update_main_char(
    server: &Server,
    conn: &Connection,
    data: &SensorData,
) -> Result<(), SetValueError> {
    server
        .main
        .battery_level_notify(conn, data.battery_voltage as u16)
        .or_else(|_| server.main.battery_level_set(data.battery_voltage as u16))?;
    server
        .main
        .humidity_notify(conn, data.sht_data.humidity.as_millipercent())
        .or_else(|_| {
            server
                .main
                .humidity_set(data.sht_data.humidity.as_millipercent())
        })?;
    server
        .main
        .temperature_notify(conn, data.sht_data.temperature.as_millidegrees_celsius())
        .or_else(|_| {
            server
                .main
                .temperature_set(data.sht_data.temperature.as_millidegrees_celsius())
        })?;
    server
        .main
        .illuminance_notify(conn, data.ltr_data.lux)
        .or_else(|_| server.main.illuminance_set(data.ltr_data.lux))?;
    Ok(())
}

fn update_probe_char(
    server: &Server,
    conn: &Connection,
    data: &SensorData,
) -> Result<(), SetValueError> {
    server
        .probe
        .frequency_notify(conn, data.soil_moisture)
        .or_else(|_| server.probe.frequency_set(data.soil_moisture))?;

    server
        .probe
        .temperature_notify(conn, data.soil_temperature)
        .or_else(|_| server.probe.temperature_set(data.soil_temperature))?;

    Ok(())
}

async fn update_gatt(server: &Server, connection: &Connection) {
    loop {
        let sensor_data = SENSOR_DATA.lock().await;
        match sensor_data.as_ref() {
            Some(data) => {
                update_main_char(server, connection, data).unwrap();
                update_probe_char(server, connection, data).unwrap();
            }
            None => {}
        }

        // Unlock mutex.
        mem::drop(sensor_data);

        // Only update every second. TODO: might replace with pub-sub model to be more efficient.
        Timer::after(Duration::from_secs(1)).await;
    }
}

async fn run_bluetooth(sd: &'static Softdevice, server: &Server) {
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
        // equivalent to 1000ms
        config.interval = 1600;
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
        gatt_server::set_sys_attrs(&conn, None).unwrap();

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
}

#[embassy_executor::task]
pub async fn ble_task(spawner: Spawner) {
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
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

    // Enable the softdevice.
    let server = unwrap!(Server::new(sd));
    unwrap!(spawner.spawn(softdevice_task(sd)));

    // Does not return.
    run_bluetooth(sd, &server).await;
}

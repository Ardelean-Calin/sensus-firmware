#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]

#[path = "tasks/app.rs"]
mod app;

#[path = "../common.rs"]
mod common;

use core::mem;
use defmt::{assert_eq, info, *};
use embassy_executor::Spawner;
use futures::pin_mut;
use nrf52832_pac as pac;
use nrf_softdevice::ble::gatt_server::SetValueError;
use nrf_softdevice::ble::{gatt_server, peripheral, Connection, TxPower};
use nrf_softdevice::{raw, Softdevice};
use raw::{sd_power_dcdc_mode_set, NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE};

use crate::app::{DataPacket, SENSOR_DATA_BUS};

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[nrf_softdevice::gatt_service(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a600323d")]
struct MainService {
    #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a601323d", read, notify)]
    all_data: [u8; 14],
    // #[characteristic(uuid = "dc7af6ef-1bf2-4722-8cbd-12e4a605323d", read, notify)]
    // flags: u8, // Different flags. Charging, plugged in, etc.
}

#[nrf_softdevice::gatt_service(uuid = "3913a152-fffb-45af-95f6-9177d2005e39")]
struct ControlService {}

#[nrf_softdevice::gatt_server]
struct Server {
    main: MainService,
    // control: ControlService,
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

// Reconfigure UICR to enable reset pin if required (resets if changed).
pub fn configure_reset_pin() {
    let uicr = unsafe { &*pac::UICR::ptr() };
    let nvmc = unsafe { &*pac::NVMC::ptr() };

    #[cfg(feature = "nrf52840")]
    const RESET_PIN: u8 = 18;
    #[cfg(feature = "nrf52832")]
    const RESET_PIN: u8 = 21;

    // Sequence copied from Nordic SDK components/toolchain/system_nrf52.c
    if uicr.pselreset[0].read().connect().is_disconnected()
        || uicr.pselreset[1].read().connect().is_disconnected()
    {
        nvmc.config.write(|w| w.wen().wen());
        while nvmc.ready.read().ready().is_busy() {}

        for i in 0..=1 {
            uicr.pselreset[i].write(|w| {
                unsafe {
                    w.pin().bits(RESET_PIN);
                } // should be 21 for 52832

                #[cfg(feature = "nrf52840")]
                w.port().clear_bit(); // not present on 52832

                w.connect().connected();
                w
            });
            while nvmc.ready.read().ready().is_busy() {}
        }

        nvmc.config.write(|w| w.wen().ren());
        while nvmc.ready.read().ready().is_busy() {}

        cortex_m::peripheral::SCB::sys_reset();
    }
}

/// Reconfigure NFC pins to be regular GPIO pins (resets if changed).
/// It's a simple bit flag on LSb of the UICR register.
pub fn configure_nfc_pins_as_gpio() {
    let uicr = unsafe { &*pac::UICR::ptr() };
    let nvmc = unsafe { &*pac::NVMC::ptr() };

    // Sequence copied from Nordic SDK components/toolchain/system_nrf52.c line 173
    if uicr.nfcpins.read().protect().is_nfc() {
        nvmc.config.write(|w| w.wen().wen());
        while nvmc.ready.read().ready().is_busy() {}

        uicr.nfcpins.write(|w| w.protect().disabled());
        while nvmc.ready.read().ready().is_busy() {}

        nvmc.config.write(|w| w.wen().ren());
        while nvmc.ready.read().ready().is_busy() {}

        cortex_m::peripheral::SCB::sys_reset();
    }
}

// #[entry]
// fn main() -> ! {
//     let mut config = embassy_nrf::config::Config::default();
//     config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
//     config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P7;
//     config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
//     config.lfclk_source = embassy_nrf::config::LfclkSource::InternalRC;

//     // Peripherals config
//     let p = embassy_nrf::init(config);
//     unsafe {
//         ble_app_init();
//     }
//     let executor = EXECUTOR_LOW.init(Executor::new());
//     executor.run(|spawner| {
//         unwrap!(spawner.spawn(app::application_task(p)));
//         // unwrap!(spawner.spawn(ble::ble_task(spawner)));
//     });
// }

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Configure NFC pins as gpio.
    // configure_nfc_pins_as_gpio();
    // Configure Pin 21 as reset pin (for now)
    configure_reset_pin();

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

    // Enable the softdevice.
    let server = unwrap!(Server::new(sd));
    let spawner = Spawner::for_current_executor().await;
    unwrap!(spawner.spawn(softdevice_task(sd)));

    // I need to create two tasks
    // 1) BLE task => Handles everything BLE
    // 2) Application task => Handles my application; Split into sub-tasks (coroutines):
    //      a) Aquisition coroutine => Handles data aquisition
    //      b) Timer coroutine => Handles keeping time & triggering the aquisition task

    // Important! We NEED to setup the priorities before initializing the softdevice.
    // That's why we create the peripheral structure here.

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

    let mut config = embassy_nrf::config::Config::default();
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P7;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P2;
    config.lfclk_source = embassy_nrf::config::LfclkSource::InternalRC;

    // Peripherals config
    let p = embassy_nrf::init(config);
    spawner.must_spawn(app::application_task(p));

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

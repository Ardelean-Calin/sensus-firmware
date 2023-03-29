use core::cell::OnceCell;
use core::mem;

use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use nrf_softdevice::ble::Address;
use nrf_softdevice::raw;
use nrf_softdevice::Softdevice;

// Private modules
mod macros;

// Public modules
pub mod coroutines;
pub mod payload_manager;
pub mod state_machines;
pub mod types;
// Exported variables
pub static mut MAC_ADDRESS: Option<Address> = None;

// Synchronization variables
/// Synchronizes new advertising data between state machine and advertising loop.
static ADV_DATA: Signal<ThreadModeRawMutex, types::AdvertismentData> = Signal::new();

/// Configures BLE and returns a reference to the SoftDevice.
pub fn configure_ble<'a>() -> &'a mut Softdevice {
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_XTAL as u8,
            rc_ctiv: 0,
            rc_temp_ctiv: 0,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_20_PPM as u8,
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
            p_value: b"Sensus" as *const u8 as _,
            current_len: 6,
            max_len: 6,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);
    let mac_address = nrf_softdevice::ble::get_address(sd);
    unsafe {
        MAC_ADDRESS.replace(mac_address);
    }
    defmt::info!("BLE MAC address: {:?}", mac_address);
    // Enable DC/DC converter for the Softdevice.
    unsafe {
        let ret =
            raw::sd_power_dcdc_mode_set(raw::NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE as u8);
        assert_eq!(ret, 0, "Error when enabling DC/DC converter: {}", ret);
    }

    sd
}

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

#[path = "../common.rs"]
mod common;

use core::mem;

use defmt::{info, *};
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::saadc::{Input, Oversample, Saadc};
use embassy_nrf::twim::{self, Twim};
use embassy_nrf::{interrupt, saadc};
use embassy_time::{Duration, Timer};
use futures::future::{select, Either};
use futures::pin_mut;
use nrf52832_pac as pac;
use nrf_softdevice::ble::{gatt_server, peripheral, Connection};
use nrf_softdevice::{raw, Softdevice};

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

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

// TODO: Remove from the main.rs file and move to battery.rs
async fn read_battery_level<'a>(
    saadc: &'a mut Saadc<'_, 1>,
    server: &'a Server,
    connection: &'a Connection,
) {
    loop {
        let mut buf = [0i16; 1];
        saadc.sample(&mut buf).await;
        let voltage: u32 = u32::from(buf[0].unsigned_abs()) * 200000 / 113778;

        // Send the voltage somehow to a task that can access the server struct
        match server.bas.battery_level_notify(connection, voltage) {
            Ok(_) => info!("Battery voltage: {=u32}mV", &voltage),
            Err(_) => unwrap!(server.bas.battery_level_set(voltage)),
        };

        Timer::after(Duration::from_secs(1)).await
    }
}

#[nrf_softdevice::gatt_service(uuid = "180f")]
struct BatteryService {
    #[characteristic(uuid = "2a19", read, notify)]
    battery_level: u32,
}

#[nrf_softdevice::gatt_server]
struct Server {
    bas: BatteryService,
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello World!");

    // Configure NFC pins as gpio.
    configure_nfc_pins_as_gpio();

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
    let server = unwrap!(Server::new(sd));

    unwrap!(spawner.spawn(softdevice_task(sd)));

    #[rustfmt::skip]
    let adv_data = &[
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        0x03, 0x03, 0x09, 0x18,
        0x0a, 0x09, b'H', b'e', b'l', b'l', b'o', b'R', b'u', b's', b't',
    ];
    #[rustfmt::skip]
    let scan_data = &[
        0x03, 0x03, 0x09, 0x18,
    ];

    let mut p = embassy_nrf::init(Default::default());
    let mut sen = Output::new(p.P0_06, Level::Low, OutputDrive::Standard);

    // ADC initialization
    let adc_pin = p.P0_29.degrade_saadc();
    let mut config = saadc::Config::default();
    config.oversample = Oversample::OVER64X;
    let channel_cfg = saadc::ChannelConfig::single_ended(adc_pin);
    let mut saadc = saadc::Saadc::new(p.SAADC, interrupt::take!(SAADC), config, [channel_cfg]);
    saadc.calibrate().await;

    // Enable power to the sensors
    sen.set_high();
    // 0 -> 3.3V in less than 500us. Measured on oscilloscope. So I will set 2 ms just to be sure.
    Timer::after(Duration::from_millis(2)).await;

    let mut irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
    let mut i2c_config = twim::Config::default();
    i2c_config.frequency = twim::Frequency::K250; // Middle ground between speed and power consumption.
    i2c_config.scl_pullup = true;
    i2c_config.sda_pullup = true;

    let mut twi = Twim::new(
        &mut p.TWISPI0,
        &mut irq,
        &mut p.P0_08,
        &mut p.P0_09,
        i2c_config,
    );

    let delay = embassy_time::Delay;
    let mut shtc3 = shtcx::shtc3(twi);
    let dev_id = shtc3.device_identifier().unwrap();
    info!("Read dev_id: {}", dev_id);

    // Reading the device identifier is complete in about 4ms
    // Disable power to the sensors.
    sen.set_low();

    loop {
        let mut config = peripheral::Config::default();
        // equivalent to 1000ms
        config.interval = 1600;
        // config.tx_power = TxPower::Plus3dBm;

        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data,
            scan_data,
        };
        let conn = unwrap!(peripheral::advertise_connectable(sd, adv, &config).await);
        info!("advertising done! I have a connection.");

        // Now that we have a connection, we can initialize the sensors and start measuring.
        let adc_fut = read_battery_level(&mut saadc, &server, &conn);

        // Run the GATT server on the connection. This returns when the connection gets disconnected.
        let gatt_fut = gatt_server::run(&conn, &server, |e| match e {
            ServerEvent::Bas(e) => match e {
                BatteryServiceEvent::BatteryLevelCccdWrite { notifications } => {
                    info!("battery notifications: {}", notifications)
                }
            },
        });

        // I basically use "select" to wait for either one of the futures to complete. This way I can borrow
        // data in the futures.
        pin_mut!(adc_fut);
        pin_mut!(gatt_fut);

        let _ = match select(adc_fut, gatt_fut).await {
            Either::Left((_, _)) => {
                info!("ADC encountered an error and stopped!")
            }
            Either::Right((res, _)) => {
                if let Err(e) = res {
                    info!("gatt_server run exited with error: {:?}", e);
                }
            }
        };
    }
}

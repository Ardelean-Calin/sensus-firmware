use core::mem::size_of;

use embassy_sync::{
    blocking_mutex::raw::ThreadModeRawMutex,
    mutex::{Mutex, TryLockError},
};
use embedded_storage_async::nor_flash::NorFlash;

// Stores the current sensus configuration globally, so that other parts of the program can load it.
pub static SENSUS_CONFIG: Mutex<ThreadModeRawMutex, Option<SensusConfig>> = Mutex::new(None);
// I got to make sure my config fits in this amount of bytes.
const CONFIG_SIZE: usize = size_of::<types::SensusConfig>();

// Public interfaces.
pub mod types;

use core::sync::atomic::Ordering::Relaxed;
use embassy_boot_nrf::AlignedBuffer;
use types::ConfigPayload;

use crate::{
    comm_manager::types::{CommResponse, ResponseTypeErr, ResponseTypeOk},
    common,
    globals::TX_BUS,
    power_manager::PLUGGED_IN_FLAG,
    sensors::{ONBOARD_SAMPLE_PERIOD, PROBE_SAMPLE_PERIOD},
    FLASH_DRIVER,
};

use self::types::{ConfigError, ConfigResponse, SensusConfig};

extern "C" {
    static __config_section_start__: u32;
    static __config_section_end__: u32;
}

/// Stores a `SensusConfig` structure to flash.
pub async fn store_sensus_config(config: types::SensusConfig) -> Result<(), ConfigError> {
    // Verifies if the fields are in the expected ranges.
    let config = config.verify()?;

    if config == load_sensus_config() {
        defmt::info!("Config the same as stored config. Skipping rewrite.");
        return Ok(());
    }

    let mut buf: AlignedBuffer<CONFIG_SIZE> = AlignedBuffer([0; CONFIG_SIZE]);
    postcard::to_slice(&config, buf.as_mut()).map_err(|_| ConfigError::SerializationError)?;

    let mut f = FLASH_DRIVER.lock().await;
    let flash_ref = defmt::unwrap!(f.as_mut());

    unsafe {
        let p_config_start: *const u32 = &__config_section_start__;
        let p_config_end: *const u32 = &__config_section_end__;

        flash_ref
            .erase(p_config_start as u32, p_config_end as u32)
            .await
            .map_err(|e| ConfigError::Flash(e as u8))?;

        flash_ref
            .write(p_config_start as u32, buf.as_mut())
            .await
            .map_err(|e| ConfigError::Flash(e as u8))?;
    }

    // Store a mirror image of the latest config in RAM.
    *SENSUS_CONFIG.lock().await = Some(config.clone());
    // rgb::restart_state_machine();
    defmt::info!("Config loaded successfully!");
    defmt::info!("New config: {}", config);
    Ok(())
}

/// Loads the saved configuration from flash and performs all necessary configurations.
pub fn load_sensus_config() -> types::SensusConfig {
    let buf = unsafe {
        let p_config_start: *const u32 = &__config_section_start__;
        let ptr = core::slice::from_raw_parts(p_config_start as *const u8, CONFIG_SIZE);
        // I need to clone the data into a pointer found in the stack, otherwise I can't decode it in-place since
        // the Flash is read-only.
        let mut buf = [0u8; CONFIG_SIZE];
        buf.clone_from_slice(ptr);
        buf
    };
    let cfg: types::SensusConfig = postcard::from_bytes(&buf).unwrap_or_default();

    // Restart all state machines that depend on configuration
    match PLUGGED_IN_FLAG.load(Relaxed) {
        true => {
            ONBOARD_SAMPLE_PERIOD.store(cfg.sampling_period.onboard_sdt_plugged_ms, Relaxed);
            PROBE_SAMPLE_PERIOD.store(cfg.sampling_period.probe_sdt_plugged_ms, Relaxed);
        }
        false => {
            ONBOARD_SAMPLE_PERIOD.store(cfg.sampling_period.onboard_sdt_battery_ms, Relaxed);
            PROBE_SAMPLE_PERIOD.store(cfg.sampling_period.probe_sdt_battery_ms, Relaxed);
        }
    }
    cfg
}

pub async fn process_payload(payload: ConfigPayload) {
    // This process is simple, I don't actually need a state machine.
    match payload {
        ConfigPayload::ConfigGet => {
            let config = load_sensus_config();
            TX_BUS
                .dyn_immediate_publisher()
                .publish_immediate(CommResponse::Ok(ResponseTypeOk::Config(
                    ConfigResponse::GetConfig(config.into()),
                )));
        }
        ConfigPayload::ConfigSet(new_cfg) => match store_sensus_config(new_cfg.into()).await {
            Ok(_) => {
                TX_BUS
                    .dyn_immediate_publisher()
                    .publish_immediate(CommResponse::Ok(ResponseTypeOk::Config(
                        ConfigResponse::SetConfig,
                    )));
                refresh_config().expect("Error refreshing config");
                common::restart_state_machines();
            }
            Err(err) => {
                defmt::error!("Error when updating config: {:?}", err);
                TX_BUS
                    .dyn_immediate_publisher()
                    .publish_immediate(CommResponse::Err(ResponseTypeErr::Config(err)));
            }
        },
    };
}

/// Initializes the Config Manager. This needs to be called on boot.
pub fn refresh_config() -> Result<(), TryLockError> {
    let config = load_sensus_config();
    defmt::info!("Loaded the following config: {:?}", config);
    *SENSUS_CONFIG.try_lock()? = Some(config);
    Ok(())
}

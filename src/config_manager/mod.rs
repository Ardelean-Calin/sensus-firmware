use core::mem::size_of;

use aligned::{Aligned, A4};
use embedded_storage_async::nor_flash::NorFlash;

// I got to make sure my config fits in this amount of bytes.
const CONFIG_SIZE: usize = size_of::<types::SensusConfig>();

// Public interfaces.
pub mod types;

use types::ConfigPayload;

use crate::FLASH_DRIVER;

use self::types::{ConfigError, ConfigResponse};

extern "C" {
    static __config_section_start__: u32;
    static __config_section_end__: u32;
}

/// Stores a `SensusConfig` structure to flash.
pub async fn store_sensus_config(config: types::SensusConfig) -> Result<(), ConfigError> {
    // Verifies if the fields are in the expected ranges.
    let config = config.verify()?;

    let mut buf: Aligned<A4, [u8; CONFIG_SIZE]> = Aligned([0; CONFIG_SIZE]);
    let serialized =
        postcard::to_slice(&config, buf.as_mut()).map_err(|_| ConfigError::SerializationError)?;

    let mut f = FLASH_DRIVER.lock().await;
    let flash_ref = f.as_mut().unwrap();

    unsafe {
        let p_config_start: *const u32 = &__config_section_start__;
        let p_config_end: *const u32 = &__config_section_end__;

        flash_ref
            .erase(p_config_start as u32, p_config_end as u32)
            .await
            .expect("Error erasing flash.");

        flash_ref
            .write(p_config_start as u32, serialized)
            .await
            .expect("Error writing config to flash.")
    }

    Ok(())
}

/// Loads the saved configuration from flash.
pub fn load_sensus_config() -> types::SensusConfig {
    unsafe {
        let p_config_start: *const u32 = &__config_section_start__;
        let ptr = core::slice::from_raw_parts(p_config_start as *const u8, CONFIG_SIZE);
        // I need to clone the data into a pointer found in the stack, otherwise I can't decode it in-place since
        // the Flash is read-only.
        let mut buf = [0u8; CONFIG_SIZE];
        buf.clone_from_slice(ptr);
        let cfg: types::SensusConfig = postcard::from_bytes(&buf).unwrap_or_default();
        cfg
    }
}

pub async fn process_payload(payload: ConfigPayload) -> Result<ConfigResponse, ConfigError> {
    // This process is simple, I don't actually need a state machine.
    match payload {
        ConfigPayload::ConfigGet => {
            let config = load_sensus_config();
            Ok(ConfigResponse::GetConfig(config))
        }
        ConfigPayload::ConfigSet(new_cfg) => match store_sensus_config(new_cfg).await {
            Ok(_) => Ok(ConfigResponse::SetConfig),
            Err(e) => Err(e),
        },
    }
}

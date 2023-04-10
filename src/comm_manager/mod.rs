pub mod types;
use crate::{
    ble::MAC_ADDRESS,
    globals::{RX_BUS, TX_BUS},
    sensors::LATEST_SENSOR_DATA,
    FLASH_DRIVER,
};
use embassy_boot_nrf::{AlignedBuffer, FirmwareUpdater};
use types::{CommResponse, ResponseTypeErr};

/// This is the main Communication loop. It handles everything communication-related.
/// Data comes in via a subscriber and gets sent away via a publisher.
async fn comm_mgr_loop() {
    let mut data_rx = RX_BUS
        .dyn_subscriber()
        .expect("Failed to acquire subscriber.");
    let data_tx = TX_BUS
        .dyn_publisher()
        .expect("Failed to acquire publisher.");

    let mut marked_booted = false;

    loop {
        match data_rx.next_message_pure().await {
            Ok(packet) => {
                match packet.payload {
                    types::CommPacketType::DfuPacket(payload) => {
                        if !marked_booted {
                            // If we got a DFU message, we are clearly booted and at least the DFU
                            // seems to be working.
                            let mut updater = FirmwareUpdater::default();
                            let mut magic = AlignedBuffer([0u8; 4]);
                            let mut f = FLASH_DRIVER.lock().await;
                            let flash_ref = f.as_mut().unwrap();
                            updater
                                .mark_booted(flash_ref, magic.as_mut())
                                .await
                                .unwrap();
                            marked_booted = true;
                            defmt::info!("DFU WAS SUCCESSFUL.");
                        };
                        // Feed to DFU state machine for processing.
                        crate::dfu::process_payload(payload).await;
                    }
                    types::CommPacketType::ConfigPacket(payload) => {
                        crate::config_manager::process_payload(payload).await;
                    }
                    types::CommPacketType::GetLatestSensordata => {
                        match LATEST_SENSOR_DATA.try_lock() {
                            Ok(data) => {
                                let latest_data = &*data;
                                let latest_data = latest_data.clone().unwrap_or_default();

                                data_tx
                                    .publish(CommResponse::Ok(types::ResponseTypeOk::SensorData(
                                        latest_data,
                                    )))
                                    .await;
                            }
                            Err(_) => {
                                data_tx
                                    .publish(CommResponse::Err(
                                        types::ResponseTypeErr::FailedToGetSensorData,
                                    ))
                                    .await;
                            }
                        };
                    }
                    types::CommPacketType::GetMacAddress => unsafe {
                        // It's ok since we only write MAC_ADDRESS once.
                        match MAC_ADDRESS {
                            Some(address) => {
                                data_tx
                                    .publish(CommResponse::Ok(types::ResponseTypeOk::MacAddress(
                                        address.bytes(),
                                    )))
                                    .await;
                            }
                            None => {
                                data_tx
                                    .publish(CommResponse::Err(
                                        types::ResponseTypeErr::MacAddressNotInitialized,
                                    ))
                                    .await;
                            }
                        }
                    },
                };
            }
            Err(err) => {
                defmt::error!("[COMM_MANAGER] Packet Error: {:?}", err);
                data_tx
                    .publish(CommResponse::Err(ResponseTypeErr::Phys(err)))
                    .await
            }
        }
    }
}

#[embassy_executor::task]
pub async fn comm_task() {
    comm_mgr_loop().await;
}

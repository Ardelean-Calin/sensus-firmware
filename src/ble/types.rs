use core::str::FromStr;

use embassy_time::Instant;
use heapless::{String, Vec};

use bthome::{self, BTHome};
use defmt::{unwrap, Format};

use crate::{
    bthome_length,
    drivers::{onboard::types::OnboardSample, probe::types::ProbeSample},
    extend_if_some,
};

#[derive(Default, Format, Clone, Copy)]
pub struct AdvertismentPayload {
    pub battery_level: Option<bthome::fields::Voltage_1mV>,
    pub air_temperature: Option<bthome::fields::Temperature_100mK>,
    pub air_humidity: Option<bthome::fields::Humidity_1Per>,
    pub illuminance: Option<bthome::fields::Illuminance_10mLux>,
    pub soil_humidity: Option<bthome::fields::Moisture_10mPer>,
    pub soil_temperature: Option<bthome::fields::Temperature_100mK>,
    pub uptime: Option<bthome::fields::Count_4bytes>,
    pub plugged_in: Option<bthome::flags::Plugged_In>,
    pub packet_id: Option<bthome::fields::PacketID>,
    // TODO. Add errors and stuff.
}

impl AdvertismentPayload {
    pub fn with_packet_id(self, packet_id: u8) -> Self {
        Self {
            packet_id: Some((packet_id as f32).into()),
            ..self
        }
    }

    pub fn with_uptime(self, uptime: Instant) -> Self {
        Self {
            uptime: Some((uptime.as_secs() as f32).into()),
            ..self
        }
    }

    pub fn with_onboard_data(self, data: OnboardSample) -> Self {
        Self {
            battery_level: Some(data.battery_level.value.into()),
            air_temperature: Some(data.environment_data.temperature.into()),
            air_humidity: Some(data.environment_data.humidity.into()),
            illuminance: Some(data.environment_data.illuminance.into()),
            ..self
        }
    }

    pub fn with_probe_data(self, data: ProbeSample) -> Self {
        Self {
            soil_temperature: Some(data.temperature.into()),
            soil_humidity: Some(data.moisture.into()),
            ..self
        }
    }

    pub fn with_plugged_in(self, plugged_in: bool) -> Self {
        Self {
            plugged_in: Some(plugged_in.into()),
            ..self
        }
    }

    fn length(&self) -> usize {
        bthome_length!(self.battery_level)
            + bthome_length!(self.air_temperature)
            + bthome_length!(self.air_humidity)
            + bthome_length!(self.illuminance)
            + bthome_length!(self.soil_humidity)
            + bthome_length!(self.soil_temperature)
            + bthome_length!(self.uptime)
            + bthome_length!(self.plugged_in)
    }

    pub fn as_vec(&self) -> Vec<u8, 64> {
        // We will build upon this vector.
        let mut my_vec = Vec::<u8, 64>::new();

        // NOTE: Order is important here!
        extend_if_some!(my_vec, self.battery_level);
        extend_if_some!(my_vec, self.air_temperature);
        extend_if_some!(my_vec, self.air_humidity);
        extend_if_some!(my_vec, self.illuminance);
        extend_if_some!(my_vec, self.soil_humidity);
        extend_if_some!(my_vec, self.soil_temperature);
        extend_if_some!(my_vec, self.uptime);
        extend_if_some!(my_vec, self.plugged_in);

        my_vec
    }
}

#[derive(Format, Clone)]
pub struct AdvertismentData {
    payload: AdvertismentPayload,
    #[defmt(Display2Format)]
    name: String<16>,
}

impl Default for AdvertismentData {
    fn default() -> Self {
        Self {
            payload: Default::default(),
            name: String::from_str("Sensus")
                .expect("Name too long. Please limit to 16 characters."),
        }
    }
}

impl AdvertismentData {
    pub fn as_vec(&self) -> Vec<u8, 64> {
        let mut buff = Vec::<u8, 64>::new();

        // Flags
        unwrap!(buff.extend_from_slice(&[
            0x02,
            0x01,
            nrf_softdevice::raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        ]));

        // The BTHome Header
        let header = [self.payload.length() as u8 + 4, 0x16, 0xD2, 0xFC, 0x40];
        unwrap!(buff.extend_from_slice(&header));

        // The actual payload
        let payload = self.payload.as_vec();
        unwrap!(buff.extend_from_slice(payload.as_slice()));

        // At the end, just add the name
        unwrap!(buff.push((self.name.len() as u8) + 1)); // AD element length
        unwrap!(buff.push(0x09u8));
        unwrap!(buff.extend_from_slice(self.name.as_bytes()));

        buff
    }

    pub fn with_payload(&self, payload: AdvertismentPayload) -> Self {
        // let mut new_adv_data
        Self {
            payload,
            ..self.clone()
        }
    }
}

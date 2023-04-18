use core::str::FromStr;

use heapless::{String, Vec};

use defmt::{unwrap, Format};

build_bthome_ad!(
    struct BTHomeAD {
        battery_level: bthome::fields::Voltage_1mV,
        air_temperature: bthome::fields::Temperature_100mK,
        air_humidity: bthome::fields::Humidity_1Per,
        illuminance: bthome::fields::Illuminance_10mLux,
        soil_humidity: bthome::fields::Moisture_1Per,
        soil_temperature: bthome::fields::Temperature_100mK,
        battery_low: bthome::flags::Battery,
    }
);

#[derive(Format, Clone)]
pub struct AdvertismentData {
    bthome: BTHomeAD,
    #[defmt(Display2Format)]
    name: String<29>, // 29 bytes because I want to encode this in the scan-response data.
}

impl Default for AdvertismentData {
    fn default() -> Self {
        Self {
            bthome: Default::default(),
            name: String::from_str("Sensus")
                .expect("Name too long. Please limit to 29 characters."),
        }
    }
}

impl AdvertismentData {
    pub fn set_name(&mut self, name: String<29>) {
        self.name = name;
    }

    /// Builds a BTHome AD element. Each AD element is maximum 31 bytes long.
    pub fn get_ad_bthome(&self) -> Vec<u8, 31> {
        self.bthome.as_vec()
    }

    pub fn get_ad_localname(&self) -> Vec<u8, 31> {
        let mut buf = Vec::<u8, 31>::new();

        // At the end, just add the name
        unwrap!(buf.push((self.name.len() as u8) + 1)); // AD element length
        unwrap!(buf.push(0x09u8));
        unwrap!(buf.extend_from_slice(self.name.as_bytes()));

        buf
    }

    pub fn with_bthome(&self, bthome_ad: BTHomeAD) -> Self {
        Self {
            bthome: bthome_ad,
            ..self.clone()
        }
    }
}

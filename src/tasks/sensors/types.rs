use defmt::Format;
use serde::{Deserialize, Serialize};

#[derive(Format, Clone, Serialize, Deserialize)]
pub struct DataPacket {
    pub battery_voltage: u16, // unit: mV
    pub env_data: crate::drivers::environment::EnvironmentData,
    pub probe_data: crate::drivers::soil_sensor::ProbeData,
}

impl DataPacket {
    pub fn to_bytes_array(&self) -> [u8; 14] {
        let mut arr = [0u8; 14];
        // Encode battery voltage
        arr[0] = self.battery_voltage.to_be_bytes()[0];
        arr[1] = self.battery_voltage.to_be_bytes()[1];
        // Encode air temperature
        arr[2] = self.env_data.get_air_temp().to_be_bytes()[0];
        arr[3] = self.env_data.get_air_temp().to_be_bytes()[1];
        // Encode air humidity
        arr[4] = self.env_data.get_air_humidity().to_be_bytes()[0];
        arr[5] = self.env_data.get_air_humidity().to_be_bytes()[1];
        // Encode solar illuminance
        arr[6] = self.env_data.get_illuminance().to_be_bytes()[0];
        arr[7] = self.env_data.get_illuminance().to_be_bytes()[1];
        // Probe data
        // Encode soil temperature
        arr[8] = self.probe_data.soil_temperature.to_be_bytes()[0];
        arr[9] = self.probe_data.soil_temperature.to_be_bytes()[1];
        // Encode soil moisture
        arr[10] = self.probe_data.soil_moisture.to_be_bytes()[0];
        arr[11] = self.probe_data.soil_moisture.to_be_bytes()[1];
        arr[12] = self.probe_data.soil_moisture.to_be_bytes()[2];
        arr[13] = self.probe_data.soil_moisture.to_be_bytes()[3];

        arr
    }
}

use embassy_nrf::twim::Error;
use embedded_hal_async::i2c::{self};

const ADDRESS: u8 = 0x70;
pub struct Shtc3<I2C: i2c::I2c> {
    i2c: I2C,
}

impl<I2C> Shtc3<I2C>
where
    I2C: i2c::I2c + 'static,
{
    pub fn new(i2c: I2C) -> Self {
        Self { i2c }
    }

    /// Get the device ID of your SHTC3.
    pub async fn get_device_id(&mut self) -> Result<u8, Error> {
        let mut buf = [0u8; 3];
        let tx_buf = [0xEF, 0xC8];

        self.i2c
            .write_read(ADDRESS, &tx_buf, &mut buf)
            .await
            .unwrap();

        let ident = u16::from_be_bytes([buf[0], buf[1]]);

        let lsb = (ident & 0b0011_1111) as u8;
        let msb = ((ident & 0b00001000_00000000) >> 5) as u8;
        Ok(lsb | msb)
    }
}

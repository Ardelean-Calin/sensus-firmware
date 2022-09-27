// TODO: I could have the i2c sensors in the same function/task. This way they could share the i2c bus easier

// let mut irq = interrupt::take!(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0);
// let mut i2c_config = twim::Config::default();
// i2c_config.frequency = twim::Frequency::K250; // Middle ground between speed and power consumption.
// i2c_config.scl_pullup = true;
// i2c_config.sda_pullup = true;

// let mut twi = Twim::new(
//     &mut p.TWISPI0,
//     &mut irq,
//     &mut p.P0_08,
//     &mut p.P0_09,
//     i2c_config,
// );

// let delay = embassy_time::Delay;
// let mut shtc3 = shtcx::shtc3(twi);
// let dev_id = shtc3.device_identifier().unwrap();
// info!("Read dev_id: {}", dev_id);

// Reading the device identifier is complete in about 4ms

build-app:
	echo "Building application..."
	cargo build 

build-bootloader:
	echo "Building bootloader..."
	cd ./bootloader
	cargo build

bootloader.hex:
	cd ./bootloader; cargo objcopy --release -- -O ihex bootloader.hex
app.hex:
	cargo objcopy --release -- -O ihex app.hex

mergehex: app.hex bootloader.hex other/softdevice/s132_nrf52_7.3.0/s132_nrf52_7.3.0_softdevice.hex
	echo "Merging HEX files..."
	mergehex -m app.hex ./bootloader/bootloader.hex other/softdevice/s132_nrf52_7.3.0/s132_nrf52_7.3.0_softdevice.hex -o sensus_bl_sd730.hex
	
sensus_bl_sd730.hex: mergehex

flash: sensus_bl_sd730.hex
	echo "Flashing hex..."
	probe-rs download --chip nrf52832_xxAA --chip-erase --format ihex $@ && probe-rs reset --chip nrf52832_xxAA
	
clean:
	cargo clean
#!/usr/bin/bash

probe-rs-cli download --format hex app_bootloader_sd_v0.0.2.hex --chip nrf52832_xxaa --chip-erase && probe-rs-cli reset --chip nrf52832_xxaa
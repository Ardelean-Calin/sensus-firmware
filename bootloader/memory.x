MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  /* NOTE: This memory layout can be squeezed if needed */
  MBR_SOFTDEVICE                    : ORIGIN = 0x00000000, LENGTH = 152K
  ACTIVE                            : ORIGIN = 0x00026000, LENGTH = 160K /* Location of the currently active firmware. Firmware always runs from this place. */
  DFU                               : ORIGIN = 0x0004E000, LENGTH = 164K /* Needs to be 1 page (4k) bigger than ACTIVE. Bootloader will swap the firmware from here into ACTIVE. */
  CONFIG                            : ORIGIN = 0x00078000, LENGTH = 4K
  FLASH                             : ORIGIN = 0x00079000, LENGTH = 24K  /* In this case, FLASH is where we flash our bootloader. */
  BOOTLOADER_STATE                  : ORIGIN = 0x0007F000, LENGTH = 4K   /* Where the bootloader stores the current state describing if the active and dfu partitions need to be swapped. */
  RAM                         (rwx) : ORIGIN = 0x20002cd0, LENGTH = 32K
  uicr_bootloader_start_address (r) : ORIGIN = 0x10001014, LENGTH = 0x4
}

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE);

__bootloader_active_start = ORIGIN(ACTIVE);
__bootloader_active_end = ORIGIN(ACTIVE) + LENGTH(ACTIVE);

__bootloader_dfu_start = ORIGIN(DFU);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU);

__bootloader_start = ORIGIN(FLASH);

SECTIONS
{
  .uicr_bootloader_start_address :
  {
    LONG(__bootloader_start)
  } > uicr_bootloader_start_address
}

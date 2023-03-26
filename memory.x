MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  /* Lengths need to be multiple of 4K page size */
  /* These values correspond to the nRF52832_xxAA with SoftDevices S112 7.3.0 */
  MBR_SOFTDEVICE                    : ORIGIN = 0x00000000, LENGTH = 152K
  FLASH                             : ORIGIN = 0x00026000, LENGTH = 160K
  DFU                               : ORIGIN = 0x0004E000, LENGTH = 164K
  CONFIG                            : ORIGIN = 0x00078000, LENGTH = 4K
  BOOTLOADER                        : ORIGIN = 0x00079000, LENGTH = 24K
  BOOTLOADER_STATE                  : ORIGIN = 0x0007F000, LENGTH = 4K
  RAM                         (rwx) : ORIGIN = 0x20002cd0, LENGTH = 32K
  PANDUMP                           : ORIGIN = 0x20000000 + 0x24b8 + 32K, LENGTH = 1K
}

SECTIONS
{
    /* Here we store user config such as sample interval and advertisment name. */
    .config_section :
    {
        /* Set the memory region to be initialized */
        __config_section_start__ = ADDR(.config_section);
        __config_section_end__ = ADDR(.config_section) + 4K;
    } > CONFIG
}

_panic_dump_start = ORIGIN(PANDUMP);
_panic_dump_end   = ORIGIN(PANDUMP) + LENGTH(PANDUMP);

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE);

__bootloader_dfu_start = ORIGIN(DFU);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU);
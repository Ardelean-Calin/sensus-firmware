MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  /* These values correspond to the nRF52832_xxAA with SoftDevices S112 7.3.0 */
  BOOTLOADER                        : ORIGIN = 0x00078000, LENGTH = 24K
  BOOTLOADER_STATE                  : ORIGIN = 0x0007E000, LENGTH = 4K
  FLASH                             : ORIGIN = 0x00019000, LENGTH = 188K
  DFU                               : ORIGIN = 0x00048000, LENGTH = 192K
  RAM                         (rwx) : ORIGIN = 0x200024b8, LENGTH = 32K
  PANDUMP                           : ORIGIN = 0x20000000 + 0x24b8 + 32K, LENGTH = 1K
}

_panic_dump_start = ORIGIN(PANDUMP);
_panic_dump_end   = ORIGIN(PANDUMP) + LENGTH(PANDUMP);

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE);

__bootloader_dfu_start = ORIGIN(DFU);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU);
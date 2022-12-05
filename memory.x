MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  /* These values correspond to the nRF52832_xxAA with SoftDevices S112 7.3.0 */
  FLASH : ORIGIN = 0x00000000 + 100K, LENGTH = 512K - 100k
  RAM : ORIGIN = 0x200024b8, LENGTH = 63K - 0x24b8
  PANDUMP: ORIGIN = 0x20000000 + 0x24b8 + 63K, LENGTH = 1K
}

_panic_dump_start = ORIGIN(PANDUMP);
_panic_dump_end   = ORIGIN(PANDUMP) + LENGTH(PANDUMP);
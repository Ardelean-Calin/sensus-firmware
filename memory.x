MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  /* These values correspond to the NRF52832 with SoftDevices S132 7.3.0 */
  FLASH : ORIGIN = 0x00000000 + 100K, LENGTH = 512K - 100K
  RAM : ORIGIN = 0x2000e4b0, LENGTH = 64K - 4K
}
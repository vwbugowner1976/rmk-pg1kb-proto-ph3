MEMORY
{
  /*
   * XIAO nRF52840 Plus with Adafruit UF2 bootloader and S140 7.3.0.
   * S140 occupies 0x00001000..0x00026FFF. Keep the application above it.
   * nRF52840 flash ends at 0x00100000, leaving 868 KiB for the application.
   */
  FLASH : ORIGIN = 0x00027000, LENGTH = 868K
  RAM : ORIGIN = 0x20000008, LENGTH = 255K
}

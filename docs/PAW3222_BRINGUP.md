# PAW3222 bring-up

This document tracks the first RMK hardware bring-up of the PG1KB Proto PH3 trackball.

## Current target

Start with the **right / central** half only.

```text
PAW3222 right
  SCLK   P1.05
  SDIO   P1.07
  CS     P1.03
  MOTION P1.15 (active low)
       |
       v
RMK BitBangSpiBus (half-duplex SDIO)
       |
       v
Paw3222Processor (device_id 0)
       |
       v
USB_REPORT_CHANNEL @ max 125 Hz
       |
       v
USB mouse HID
```

The existing working ZMK PAW3222 driver remains the behavioral reference.

## Why RMK BitBangSpiBus is used

RMK already contains a GPIO bit-banged SPI implementation specifically for sensors using a single bidirectional SDIO line. It switches SDIO between output and input and keeps SCLK idle high, which matches the existing PG1KB PAW3222 wiring and avoids needing a custom nRF SPIM workaround for the first bring-up.

## Temporary USB-only bring-up path

RMK 0.9's config macro initializes custom `#[register_processor]` instances before it creates the runtime `keymap`. The stock `PointingProcessor::new()` needs `&keymap`, so it cannot be constructed from the same custom processor initializer used for the PAW3222 sensor.

For the first hardware test, `Paw3222Processor` therefore sends `MouseReport` directly to RMK's public `USB_REPORT_CHANNEL`. This is intentionally temporary and is only used to prove:

1. PAW3222 transport
2. Product ID
3. MOTION handling
4. 12-bit X/Y decoding
5. USB cursor motion

After USB motion is proven, the driver will be integrated through the normal RMK PointingDevice / PointingProcessor path so USB and BLE share the normal active-transport routing.

## Initial settings

- Right PAW3222 device ID: `0`
- CPI: `1178` (`31 * 38`)
- Force awake: `false` initially, matching the existing ZMK power-saving behavior
- Processor poll interval: `1 ms`
- USB report ceiling: `8 ms` / `125 Hz`
- MOTION pin is active low

Fast motion that exceeds the signed 8-bit HID X/Y range is kept in the accumulator and emitted over following reports instead of being discarded.

## Build

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
git pull --ff-only
cargo build --release --bin central
```

If the build succeeds:

```bash
cargo make uf2-central --release
```

Copy to Windows:

```bash
mkdir -p /mnt/d/rmk-firmware
cp -v rmk-pg1kb-proto-ph3-central.uf2 /mnt/d/rmk-firmware/
```

## First hardware test

Flash only the right/central XIAO first.

Expected sequence:

1. Firmware boots.
2. PAW3222 Product ID `0x30` is detected.
3. Moving the right ball asserts MOTION low.
4. Signed 12-bit X/Y deltas are read.
5. The bring-up processor emits USB mouse reports at up to 125 Hz.
6. Cursor movement is verified over USB.

BLE comparison comes after this temporary USB path is proven and replaced by normal RMK pointing integration.

## If Product ID is not 0x30

Do not change CPI or transforms yet. First verify only the transport:

- SCLK = P1.05
- SDIO = P1.07
- CS = P1.03
- MOTION = P1.15
- sensor power / ground
- bitbang read direction switching

The current driver retries Product ID reads up to 10 times with 100 ms between attempts, matching the existing ZMK driver's startup strategy.

## If Product ID works but cursor does not move

Check in this order:

1. MOTION pin goes low while moving the ball.
2. MOTION register bit 7 is set.
3. DELTA_X / DELTA_Y / DELTA_XY_HI return changing values.
4. 12-bit sign extension is correct.
5. USB mouse reports are reaching `USB_REPORT_CHANNEL`.

Do not add scroll/inertia/layer transforms until raw cursor motion is confirmed.

## Next milestones

After right USB cursor movement works:

1. Replace the temporary USB-only report bridge with normal RMK PointingDevice / PointingProcessor integration.
2. Compare right cursor smoothness over BLE against ZMK.
3. Tune report/poll timing only if required.
4. Add the left/peripheral PAW3222 with a distinct `device_id = 1`.
5. Confirm split forwarding preserves device IDs.
6. Restore PG1KB layer-dependent cursor/scroll transforms.
7. Recreate inertia only after the raw pointing path is stable.

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
PointingEvent @ max 125 Hz
       |
       v
PointingProcessor
       |
       v
USB / BLE mouse HID
```

The existing working ZMK PAW3222 driver remains the behavioral reference.

## Why RMK BitBangSpiBus is used

RMK already contains a GPIO bit-banged SPI implementation specifically for sensors using a single bidirectional SDIO line. It switches SDIO between output and input and keeps SCLK idle high, which matches the existing PG1KB PAW3222 wiring and avoids needing a custom nRF SPIM workaround for the first bring-up.

## Initial settings

- Right PAW3222 device ID: `0`
- CPI: `1178` (`31 * 38`)
- Force awake: `false` initially, matching the existing ZMK power-saving behavior
- Processor poll interval: `1 ms`
- HID pointing event ceiling: `8 ms` / `125 Hz`
- MOTION pin is active low

The 125 Hz output limit follows RMK's built-in pointing-device design. RMK documents this as a way to avoid flooding the event channel, especially over BLE.

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
5. RMK publishes `PointingEvent` for device ID 0.
6. The default `PointingProcessor` converts this into mouse movement.
7. Cursor movement is verified over USB first.
8. BLE cursor feel is compared with the existing ZMK firmware after USB works.

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
5. `PointingEvent` is published.
6. The central `PointingProcessor` is running for `device_id = 0`.

Do not add scroll/inertia/layer transforms until raw cursor motion is confirmed.

## Next milestones

After right USB cursor movement works:

1. Compare right cursor smoothness over BLE against ZMK.
2. Tune report/poll timing only if required.
3. Add the left/peripheral PAW3222 with a distinct `device_id = 1`.
4. Confirm split forwarding preserves device IDs.
5. Restore PG1KB layer-dependent cursor/scroll transforms.
6. Recreate inertia only after the raw pointing path is stable.

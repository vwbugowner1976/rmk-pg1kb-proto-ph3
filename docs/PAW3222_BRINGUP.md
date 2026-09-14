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
       +--> RMK usb_log CDC ACM diagnostics (v2 branch)
       |
       v
USB_REPORT_CHANNEL @ max 125 Hz
       |
       v
USB mouse HID
```

The existing working ZMK PAW3222 driver remains the behavioral reference.

## Verified status

### 2026-09-14: v1 central build / UF2

The PAW3222-enabled central firmware compiled successfully on the local WSL environment and the UF2 was generated and copied to Windows.

- `cargo build --release --bin central`: **SUCCESS**
- `cargo make uf2-central --release`: **SUCCESS**
- Output size: about 805 KiB
- WSL -> Windows copy: **SUCCESS**
- SHA-256 on both sides:
  `69f1ae183e3ba5eaa54a75676cc758629a705932b2cdb8b611fc92845ba621b0`

### 2026-09-14: v1 right-central hardware bring-up

The v1 firmware was flashed to the **right / central** half and tested on real hardware over USB.

- Right PAW3222 sensor path: **WORKING ON HARDWARE**
- Windows USB mouse cursor movement: **WORKING ON HARDWARE**
- User observation: trackball moved the cursor normally
- Current RMK BitBangSpiBus timing: **sufficient for basic real-hardware operation**
- Current CS-held delta sequence (`DELTA_X` -> `DELTA_Y` -> `DELTA_XY_HI`): **confirmed functional on PG1KB hardware**
- Temporary direct `USB_REPORT_CHANNEL` mouse-report bridge: **confirmed functional on PG1KB hardware**
- BLE pointing path: **NOT YET VERIFIED**
- Left / peripheral PAW3222: **NOT YET VERIFIED**

This closes the first major bring-up milestone: **right PAW3222 -> RMK -> USB HID works on real PG1KB Proto PH3 hardware**.

The exact Product ID, MOTION register values, 12-bit mode state and per-report counters were not captured in v1 because it used `defmt` rather than USB CDC logging. The v2 diagnostic branch exists to capture those details without changing the already-proven v1 baseline.

## v2 diagnostic branch

Branch: `paw3222-v2-bringup`

This branch is prepared for the next hardware test. It is intentionally separate from `main`.

Changes relative to the first bring-up version:

1. Enable RMK 0.9's official `usb_log` feature, which exposes logging as a USB CDC ACM serial port.
2. Change PAW3222 bring-up messages to the `log` crate so they are visible over USB.
3. Explicitly enable `MOUSE_OPTION` bit 2 (`XY12bit_Enh`) after reset.
4. Read `MOUSE_OPTION` back and only decode 12-bit deltas when the bit is confirmed set; otherwise fall back to signed 8-bit deltas.
5. Keep the now hardware-confirmed PG1KB/ZMK delta sequence: `DELTA_X`, `DELTA_Y`, `DELTA_XY_HI` while CS remains asserted.
6. Add a one-second diagnostic heartbeat so useful state is still visible even though USB logging cannot capture early boot messages before the serial port is opened.
7. Count sensor reads, motion events, successful USB HID reports, busy HID-channel attempts and motion-read errors.

The diagnostic heartbeat includes values similar to:

```text
PAW3222 diag ready=true init_error=None pid=0x30 12bit=true mouse_opt=0x04 motion_pin=false motion_reg=0x00 reads=123 events=120 last_dx=4 last_dy=-2 accum_x=0 accum_y=0 hid=118 hid_busy=0 read_err=0
```

**Important:** the v2 branch has been prepared in GitHub but has not yet been locally compiled or tested on hardware. Compile success must be recorded only after running the local build.

## USB logging on Windows

RMK's `usb_log` feature uses a CDC ACM serial interface alongside the keyboard/mouse USB interfaces. Boot-stage messages may be lost because the serial port is not open yet; the v2 one-second heartbeat solves this for PAW3222 bring-up.

A helper script is included:

```text
scripts/read_usb_log.py
```

On Windows:

```powershell
cd D:\path\to\rmk-pg1kb-proto-ph3
py -m pip install pyserial
py scripts\read_usb_log.py
```

The first run lists COM ports. Then run it again with the keyboard's logging port, for example:

```powershell
py scripts\read_usb_log.py COM7
```

The baud rate value is not meaningful for USB CDC, but the helper opens the port at 115200 for compatibility with serial APIs.

## Why RMK BitBangSpiBus is used

RMK already contains a GPIO bit-banged SPI implementation specifically for sensors using a single bidirectional SDIO line. It switches SDIO between output and input and keeps SCLK idle high, which matches the existing PG1KB PAW3222 wiring and avoids needing a custom nRF SPIM workaround for the first bring-up.

The current v1 implementation is now confirmed to work on real PG1KB hardware for normal trackball cursor motion, so BitBangSpiBus timing should no longer be treated as the first suspect for basic operation. The upstream RMK PAW3222 proposal still changes the sampling point and timing, so it remains useful for later high-speed/dropout comparison if needed.

## Temporary USB-only bring-up path

RMK 0.9's config macro initializes custom `#[register_processor]` instances before it creates the runtime `keymap`. The stock `PointingProcessor::new()` needs `&keymap`, so it cannot be constructed from the same custom processor initializer used for the PAW3222 sensor.

For the first hardware test, `Paw3222Processor` therefore sends `MouseReport` directly to RMK's public `USB_REPORT_CHANNEL`. This temporary path has now been proven on real hardware for right-central USB cursor motion.

The next architectural step is to replace it with the normal RMK PointingDevice / PointingProcessor path so USB and BLE share the normal active-transport routing.

## Initial settings

- Right PAW3222 device ID: `0`
- CPI: `1178` (`31 * 38`)
- Force awake: `false` initially, matching the existing ZMK power-saving behavior
- Processor poll interval: `1 ms`
- USB report ceiling: `8 ms` / `125 Hz`
- MOTION pin is active low
- v2 diagnostic heartbeat: `1 s`

Fast motion that exceeds the signed 8-bit HID X/Y range is kept in the accumulator and emitted over following reports instead of being discarded.

## Build v2 locally

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
git fetch origin
git switch paw3222-v2-bringup
git pull --ff-only
cargo build --release --bin central
```

Only after that succeeds:

```bash
cargo make uf2-central-v2 --release
mkdir -p /mnt/d/rmk-firmware
cp -v rmk-pg1kb-proto-ph3-central-v2.uf2 /mnt/d/rmk-firmware/
sha256sum rmk-pg1kb-proto-ph3-central-v2.uf2
sha256sum /mnt/d/rmk-firmware/rmk-pg1kb-proto-ph3-central-v2.uf2
```

## Next hardware test: v2 diagnostics

Flash only the right/central XIAO first.

Expected sequence:

1. Firmware boots and Windows enumerates the keyboard/mouse plus a CDC logging COM port.
2. Open the CDC port with `scripts/read_usb_log.py`.
3. The one-second heartbeat reports Product ID and initialization state.
4. PAW3222 Product ID should be `0x30`.
5. `12bit=true` and `mouse_opt` bit 2 should be set if 12-bit setup succeeds.
6. Moving the right ball should assert MOTION low and increase `reads` / `events`.
7. `last_dx` / `last_dy` should change.
8. `hid` should increase and the Windows cursor should continue to move normally.

v1 is the known-good USB baseline. If v2 changes cursor behavior, compare v1 and v2 before making further architectural changes.

## Diagnostic interpretation

### PID is not 0x30

Do not change CPI or transforms. Verify only the transport and wiring:

- SCLK = P1.05
- SDIO = P1.07
- CS = P1.03
- MOTION = P1.15
- sensor power / ground
- bitbang read direction switching

The driver retries Product ID reads up to 10 times with 100 ms between attempts, matching the existing ZMK startup strategy.

### PID = 0x30, but `motion_pin` never becomes active

Suspect the P1.15 MOTION path, pull-up, pin mapping or sensor interrupt behavior before changing the delta decoder.

### `motion_pin` reacts but `events` / deltas are wrong

Check the MOTION register and the CS-held delta sequence. If data corruption appears mostly during faster movement, compare RMK's current BitBangSpiBus sampling timing with the upstream PAW3222 proposal.

### Deltas change but `hid` does not increase

The sensor path is working and the problem is at or after `USB_REPORT_CHANNEL`. `hid_busy` indicates failed `try_send` attempts.

### `hid` increases but the cursor does not move

Focus on USB HID enumeration/report routing rather than the PAW3222 transport.

## Next milestones

With right USB cursor movement now proven:

1. Build and test the v2 USB diagnostic firmware while preserving v1 as the known-good baseline.
2. Replace the temporary USB-only report bridge with normal RMK PointingDevice / PointingProcessor integration, using the upstream RMK PAW3222 work as a reference.
3. Verify right cursor motion over USB again through the normal RMK pointing path.
4. Compare right cursor smoothness over BLE against the known ZMK behavior.
5. Separate sensor polling cadence from HID reporting cadence and compare timings if BLE still shows differences.
6. Add the left/peripheral PAW3222 with a distinct `device_id = 1`.
7. Confirm split forwarding preserves device IDs.
8. Restore PG1KB layer-dependent cursor/scroll transforms.
9. Recreate inertia only after the raw pointing path is stable.

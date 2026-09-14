# PAW3222 v4 — Official RMK + Custom Driver + BLE

Branch: `paw3222-v4-official-ble-keys`

## Purpose

Establish an apples-to-apples BLE comparison using the hardware-verified custom PAW3222 driver on official RMK 0.9.

## Baselines

- v1/v2 custom PAW3222 over USB: hardware verified smooth cursor.
- v3 t-ogura `pr/paw3222` native pointing path over BLE: keyboard and trackball both worked. Lowering report rate to 50 Hz improved the cursor somewhat, but BLE still felt noticeably more choppy than USB.

## v4 architecture

- RMK dependency: official crates.io `rmk = 0.9`
- Sensor transport/decoder: existing custom `src/paw3222.rs`, already hardware verified in v1/v2
- Right/central PAW3222 wiring unchanged:
  - SCLK = P1.05
  - SDIO = P1.07
  - CS = P1.03
  - MOTION = P1.15 active-low with pull-up
- CPI = 1178
- 12-bit mode enabled and verified by the custom driver
- Poll interval = 1 ms
- HID report cadence = 8 ms / 125 Hz
- BLE output wrapper = `src/paw3222_ble.rs`
- BLE output destination = RMK public `BLE_REPORT_CHANNEL`
- Right-side matrix remains enabled for simultaneous keyboard + pointing stress tests
- Left trackball remains disabled for this milestone

## Why keep 125 Hz in v4

v3 showed that reducing the native PR path to 50 Hz made BLE somewhat better but still worse than USB. v4 intentionally returns to the USB-verified 125 Hz cadence so the sensor/report behavior matches the known-good USB baseline as closely as possible. If v4 is still choppy, the next target is RMK BLE transport/connection timing rather than PAW3222 decoding.

## Build

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
git fetch origin
git switch paw3222-v4-official-ble-keys
git pull --ff-only
ai-build cargo build --release --bin central
```

UF2 + Windows copy:

```bash
ai-build cargo make uf2-central-v4 --release && \
mkdir -p /mnt/d/rmk-firmware && \
cp -v rmk-pg1kb-proto-ph3-central-v4.uf2 /mnt/d/rmk-firmware/ && \
sha256sum rmk-pg1kb-proto-ph3-central-v4.uf2 && \
sha256sum /mnt/d/rmk-firmware/rmk-pg1kb-proto-ph3-central-v4.uf2
```

## Hardware test

1. Flash right/central v4 UF2.
2. Pair/reconnect Windows BLE.
3. Verify right-side keys.
4. Verify right trackball.
5. Stress test by typing while moving the trackball.
6. Compare cursor smoothness against:
   - v1/v2 USB custom driver
   - v3 PR native BLE at 125 Hz
   - v3 PR native BLE at 50 Hz

## Interpretation

- v4 smooth, v3 choppy: likely native PR pointing/event path issue.
- v4 and v3 both choppy while USB is smooth: likely common RMK BLE HID transport/connection interval issue.
- v4 worse than v3: inspect direct BLE report channel pacing/backpressure.

Compile success and hardware confirmation must be recorded separately.

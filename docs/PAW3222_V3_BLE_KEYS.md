# PAW3222 v3 BLE + key bring-up

Branch: `paw3222-v3-ble-keys`

## Purpose

Test the already-proven right PAW3222 over BLE while normal central-side matrix key input is active at the same time.

## Important baseline

- v1 right PAW3222 -> USB cursor: hardware verified working
- v2 PAW3222 diagnostics: PID `0x30`, 12-bit mode confirmed, USB cursor working, no sensor read errors reported
- v3 hardware status: **not yet tested**
- v3 compile status: **not yet verified locally**

## v3 changes

1. BLE local name shortened to `PG1KB-PH3`.
   - RMK legacy host advertising has a 16-byte local-name budget.
   - The previous `PG1KB Proto PH3` exceeded that budget and produced `BleHost(InsufficientSpace)`.
2. RMK dependency switched to `t-ogura/rmk` branch `pr/paw3222` for this experiment.
3. Right PAW3222 moved from the temporary direct `USB_REPORT_CHANNEL` processor to RMK's native PAW3222 `PointingDevice` / `PointingProcessor` path.
4. Right PAW3222 configuration:
   - SCLK `P1_05`
   - SDIO `P1_07`
   - CS `P1_03`
   - MOTION `P1_15`
   - CPI `1178`
   - report rate `125 Hz`
   - force awake `false`
   - right cursor software transform: none
5. A temporary right-central Base keymap is enabled so keyboard + pointing can be stressed together before the final ZMK keymap is ported.
6. Left PAW3222, layers, scroll transforms and inertia remain intentionally out of scope.

## Local build

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
git fetch origin
git switch paw3222-v3-ble-keys
git pull --ff-only
cargo build --release --bin central
```

Do not mark v3 compile success until the command above succeeds locally.

If successful:

```bash
cargo make uf2-central-v3 --release
mkdir -p /mnt/d/rmk-firmware
cp -v rmk-pg1kb-proto-ph3-central-v3.uf2 /mnt/d/rmk-firmware/
sha256sum rmk-pg1kb-proto-ph3-central-v3.uf2
sha256sum /mnt/d/rmk-firmware/rmk-pg1kb-proto-ph3-central-v3.uf2
```

## Hardware test order

1. Flash only the right / central half.
2. Confirm `BleHost(InsufficientSpace)` no longer repeats.
3. Pair `PG1KB-PH3` with Windows over BLE.
4. Disconnect USB after pairing when evaluating BLE transport, because RMK prefers USB by default when both USB and BLE are ready.
5. Verify right trackball cursor movement over BLE.
6. Verify right-side keys produce input over BLE.
7. Move the trackball while repeatedly typing to stress simultaneous pointing + keyboard reports.
8. Compare smoothness directly against the existing ZMK BLE firmware.

## Temporary Base keymap

The v3 Base map is diagnostic only. It is intentionally simple and will be replaced by the real PG1KB ZMK Base/Num/Sym layout after the BLE transport comparison is complete.

## Success criteria

- BLE advertising succeeds without `InsufficientSpace`
- Windows connects over BLE
- Right PAW3222 moves the cursor over BLE
- Right central matrix keys generate keyboard reports over BLE
- Trackball + key input work concurrently
- No obvious cursor stutter under normal typing load

Only user-reported hardware results should be recorded as hardware-verified.

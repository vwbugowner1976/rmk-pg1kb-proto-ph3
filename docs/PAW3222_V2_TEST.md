# PAW3222 v2 diagnostic test

Branch: `paw3222-v2-bringup`

Status when this file was created:

- v1 PAW3222 central compile: verified successful
- v1 UF2 generation / Windows copy: verified successful
- v1 hardware behavior: not yet tested
- v2 source preparation: complete
- v2 local compile: **not yet verified**
- v2 hardware behavior: **not yet verified**

## What v2 changes

- RMK 0.9 `usb_log` CDC ACM logging
- one-second PAW3222 diagnostic heartbeat
- explicit `MOUSE_OPTION.XY12bit_Enh` enable and readback
- signed 8-bit fallback if 12-bit mode does not stick
- known-working PG1KB/ZMK CS-held delta read sequence retained
- sensor read / motion / HID / channel-busy / read-error counters
- separate v2 UF2 filename for A/B comparison

## Build tomorrow

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
git fetch origin
git switch paw3222-v2-bringup
git pull --ff-only
cargo build --release --bin central
```

If and only if that succeeds, record the successful build in `docs/PAW3222_BRINGUP.md`, then generate the v2 UF2:

```bash
cargo make uf2-central-v2 --release
mkdir -p /mnt/d/rmk-firmware
cp -v rmk-pg1kb-proto-ph3-central-v2.uf2 /mnt/d/rmk-firmware/
sha256sum rmk-pg1kb-proto-ph3-central-v2.uf2
sha256sum /mnt/d/rmk-firmware/rmk-pg1kb-proto-ph3-central-v2.uf2
```

This produces:

```text
D:\rmk-firmware\rmk-pg1kb-proto-ph3-central-v2.uf2
```

and does not overwrite the earlier v1 file.

## Read the USB log on Windows

The firmware should enumerate an additional CDC ACM COM port.

```powershell
py -m pip install pyserial
py scripts\read_usb_log.py
```

Pick the new RMK logging COM port from the list and open it, for example:

```powershell
py scripts\read_usb_log.py COM7
```

The PAW3222 diagnostic line repeats every second, so missing the boot-time messages is not a problem.

## Expected idle line

Values will differ, but a healthy initialized sensor should look conceptually like:

```text
PAW3222 diag ready=true init_error=None pid=0x30 12bit=true mouse_opt=0x04 motion_pin=false motion_reg=0x00 reads=0 events=0 last_dx=0 last_dy=0 accum_x=0 accum_y=0 hid=0 hid_busy=0 read_err=0
```

## While rolling the right trackball

Watch these fields:

- `motion_pin` should become `true` while MOTION is active-low
- `reads` should increase
- `events` should increase when MOTION register bit 7 is set
- `last_dx` / `last_dy` should change
- `hid` should increase as mouse reports enter the USB channel
- `hid_busy` should normally remain near zero
- `read_err` should remain zero

At the same time, verify Windows cursor movement.

## Fast fault isolation

- `pid != 0x30`: SPI / power / pin-level problem
- `pid=0x30`, `motion_pin` never reacts: MOTION P1.15 path
- MOTION reacts, deltas bad: CS-held delta sequence / BitBangSpiBus timing / 12-bit mode
- deltas good, `hid=0`: USB report channel path
- `hid` increases, cursor does not move: USB HID enumeration/report path

Do not add transforms, scrolling, inertia, the left sensor, or BLE tuning until the raw right-side USB path is proven.

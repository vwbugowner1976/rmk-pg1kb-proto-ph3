# Local build environment (WSL2 / Ubuntu)

For development, keep the RMK workspace inside the WSL Linux filesystem (for example `~/rmk-dev`) rather than under `/mnt/c` or `/mnt/d`. Cargo creates many small files and is noticeably faster on the native WSL filesystem.

## Why local build is recommended

For PG1KB/RMK development, local builds are the preferred workflow rather than GitHub Actions.

- The first build downloads and compiles the Rust dependency graph, but later incremental builds reuse Cargo's cache.
- PAW3222 development involves many small edit/build/test iterations, so rebuilding only the right/central half is much faster than starting a remote CI job every time.
- UF2 files can be generated locally and copied directly to the Windows side for flashing.
- GitHub Actions is not required for this project.

The intended development loop is therefore:

```text
edit PAW3222/RMK code
        ↓
cargo build --release --bin central
        ↓
fix compile errors if any
        ↓
cargo make uf2-central --release
        ↓
copy UF2 to Windows / flash XIAO
        ↓
test on hardware
```

## Current bring-up target

The first hardware target is intentionally limited to the right half of PG1KB:

```text
XIAO nRF52840 Plus (right / central)
        │
        ├── PG1KB key matrix
        │
        └── PAW3222
              │
              ▼
         RMK pointing input
              │
              ▼
          USB HID cursor
```

The initial milestones are:

1. Build the right/central firmware locally.
2. Bring up the PAW3222 transport on the existing PG1KB wiring.
3. Read PAW3222 Product ID `0x30` from real hardware.
4. Read signed 12-bit X/Y motion data.
5. Publish PAW3222 movement as an RMK `PointingEvent`.
6. Confirm smooth USB cursor movement.
7. Enable BLE HID and compare cursor smoothness with the existing ZMK firmware.
8. Add the left/peripheral half and BLE split.
9. Add layer-dependent Cursor / Scroll processing.
10. Re-enable configurator support (Vial/Rynk) after the basic input path is stable.

During initial bring-up, Vial/Rynk remains intentionally disabled so matrix, split, BLE and PAW3222 problems can be isolated more easily.

## PAW3222 development notes

The Rust PAW3222 implementation starts inside this repository (`src/paw3222.rs`). It will only be split into a standalone `rmk-driver-paw3222` repository after the driver is stable and reusable outside PG1KB.

The existing ZMK PAW3222 driver is the behavioral reference. Important characteristics already carried into the Rust core are:

- Product ID: `0x30`
- Motion values: signed 12-bit X/Y
- Resolution step: 38 CPI
- Reset handling
- Write-protect handling
- Sleep / force-awake handling
- Motion status register handling

Current PG1KB PAW3222 wiring on both halves:

| Signal | nRF52840 | XIAO nRF52840 Plus |
|---|---|---|
| SCLK | P1.05 | D18 |
| SDIO | P1.07 | D19 |
| CS | P1.03 | D17 |
| MOTION | P1.15 | D10 |

The SDIO connection is a single-wire data arrangement, so the next driver step is to implement the Embassy/nRF transport needed to reproduce the existing working PAW3222 communication.

## 1. Install host packages

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  git \
  curl \
  gcc-arm-none-eabi
```

`gcc-arm-none-eabi` is required by RMK's nRF52 BLE build for the optimized P-256 assembly used during pairing.

## 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

rustup update stable
rustup target add thumbv7em-none-eabihf
rustup component add llvm-tools
```

The repository also contains `rust-toolchain.toml`, so Cargo will keep the required target/components in sync when entering the project.

## 3. Install RMK build tools

```bash
cargo install --locked rmkit
cargo install --locked flip-link cargo-make
cargo install --locked cargo-binutils cargo-hex-to-uf2
```

`probe-rs` is optional because XIAO nRF52840 Plus can normally be flashed using its UF2 bootloader.

## 4. Clone the repository

```bash
mkdir -p ~/rmk-dev
cd ~/rmk-dev

git clone git@github.com:vwbugowner1976/rmk-pg1kb-proto-ph3.git
cd rmk-pg1kb-proto-ph3
```

HTTPS also works:

```bash
git clone https://github.com/vwbugowner1976/rmk-pg1kb-proto-ph3.git
```

## 5. Prime the Cargo cache

```bash
cargo fetch
```

The first build downloads and compiles the Rust dependency graph. Later incremental builds are much faster.

## 6. Build

Central/right half:

```bash
cargo build --release --bin central
```

Peripheral/left half:

```bash
cargo build --release --bin peripheral
```

Both UF2 files:

```bash
cargo make uf2 --release
```

Expected outputs in the project root:

```text
rmk-pg1kb-proto-ph3-central.uf2
rmk-pg1kb-proto-ph3-peripheral.uf2
```

## 7. Optional: copy UF2 files to the Windows D: drive

```bash
mkdir -p /mnt/d/ZMK-Firmware/rmk-builds
cp -v rmk-pg1kb-proto-ph3-*.uf2 /mnt/d/ZMK-Firmware/rmk-builds/
```

## Fast edit/build loop

During PAW3222 development, build only the half being tested:

```bash
cargo build --release --bin central
```

Generate a central UF2 only when the ELF build succeeds:

```bash
cargo make uf2-central --release
```

This avoids rebuilding/converting the peripheral image on every driver change.

When the PAW3222 core changes but the left/peripheral firmware is not being tested, there is no reason to rebuild the peripheral image on every iteration.

## Recommended day-to-day commands

After the environment has been installed once:

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
git pull --ff-only
cargo build --release --bin central
```

When the build succeeds:

```bash
cargo make uf2-central --release
mkdir -p /mnt/d/ZMK-Firmware/rmk-builds
cp -v rmk-pg1kb-proto-ph3-central.uf2 /mnt/d/ZMK-Firmware/rmk-builds/
```

For a full two-half verification:

```bash
cargo build --release --bin central
cargo build --release --bin peripheral
cargo make uf2 --release
```

# Local build environment (WSL2 / Ubuntu)

For development, keep the RMK workspace inside the WSL Linux filesystem (for example `~/rmk-dev`) rather than under `/mnt/c` or `/mnt/d`. Cargo creates many small files and is noticeably faster on the native WSL filesystem.

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

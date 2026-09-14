#!/usr/bin/env bash
set -euo pipefail

sudo apt update
sudo apt install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  git \
  curl \
  gcc-arm-none-eabi

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

# shellcheck disable=SC1091
source "$HOME/.cargo/env"

rustup update stable
rustup target add thumbv7em-none-eabihf
rustup component add llvm-tools

cargo install --locked rmkit
cargo install --locked flip-link cargo-make
cargo install --locked cargo-binutils cargo-hex-to-uf2

echo
echo "RMK WSL environment is ready."
echo "Next: clone the repo under ~/rmk-dev and run cargo fetch."

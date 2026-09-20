#!/usr/bin/env bash
set -euo pipefail

if [ -z "${PG1KB_RMK_ROOT:-}" ]; then
  echo "PG1KB_RMK_ROOT is not set" >&2
  exit 1
fi

CARGO_TOML="$PG1KB_RMK_ROOT/Cargo.toml"
LIB_RS="$PG1KB_RMK_ROOT/src/lib.rs"

python3 - "$CARGO_TOML" "$LIB_RS" <<'PY'
from pathlib import Path
import sys

cargo = Path(sys.argv[1])
lib = Path(sys.argv[2])

cargo_text = cargo.read_text()
if 'custom_message = [' not in cargo_text and 'custom_message = []' not in cargo_text:
    marker = '[features]\n'
    if marker not in cargo_text:
        raise SystemExit(f"Could not find [features] in {cargo}")
    cargo_text = cargo_text.replace(
        marker,
        marker + 'custom_message = []\n',
        1,
    )
    cargo.write_text(cargo_text)

lib_text = lib.read_text()
marker = '// TODO: re-export to `constants`?\npub(crate) use rmk_types::constants::*;'
if 'PG1KB_CUSTOM_MESSAGE_CONSTANTS' not in lib_text:
    if marker not in lib_text:
        raise SystemExit(f"Could not find RMK constant insertion point in {lib}")
    block = '''// PG1KB_CUSTOM_MESSAGE_CONSTANTS
// crates.io RMK 0.9.0 contains the custom-message transport code but did not
// publish the corresponding Cargo feature/constants. Keep this local to the
// project-local RMK copy and size it only for the 6-byte raw motion packet.
#[cfg(feature = "custom_message")]
pub const CUSTOM_MESSAGE_MAX_SIZE: usize = 8;
#[cfg(feature = "custom_message")]
pub const CUSTOM_MESSAGE_EVENT_CHANNEL_SIZE: usize = 8;
#[cfg(feature = "custom_message")]
pub const CUSTOM_MESSAGE_EVENT_PUB_SIZE: usize = 4;
#[cfg(feature = "custom_message")]
pub const CUSTOM_MESSAGE_EVENT_SUB_SIZE: usize = 4;
#[cfg(feature = "custom_message")]
pub const CUSTOM_MESSAGE_OUT_EVENT_CHANNEL_SIZE: usize = 8;
#[cfg(feature = "custom_message")]
pub const CUSTOM_MESSAGE_OUT_EVENT_PUB_SIZE: usize = 4;
#[cfg(feature = "custom_message")]
pub const CUSTOM_MESSAGE_OUT_EVENT_SUB_SIZE: usize = 4;

'''
    lib_text = lib_text.replace(marker, block + marker, 1)
    lib.write_text(lib_text)

print("Patched RMK 0.9 custom_message feature/constants for PG1KB raw motion")
PY

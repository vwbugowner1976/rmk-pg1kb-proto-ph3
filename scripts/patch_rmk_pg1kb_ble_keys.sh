#!/bin/bash
set -euo pipefail

ROOT="${PG1KB_RMK_ROOT:?PG1KB_RMK_ROOT is required}"
TARGET="$ROOT/src/keyboard.rs"

if [ ! -f "$TARGET" ]; then
  echo "RMK keyboard source not found: $TARGET" >&2
  exit 1
fi

python3 - "$TARGET" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()

marker = "PG1KB clear all BLE profiles"
if marker in text:
    print("PG1KB BLE profile keys already patched")
    raise SystemExit(0)

needle = """                } else if id == NUM_BLE_PROFILE as u8 + 3 {
                    // Toggle preferred transport (USB <-> BLE);
                    // only meaningful when both transports exist in this build.
                    #[cfg(not(feature = "_no_usb"))]
                    crate::state::toggle_preferred().await;
                }
"""

replacement = """                } else if id == NUM_BLE_PROFILE as u8 + 3 {
                    // Toggle preferred transport (USB <-> BLE);
                    // only meaningful when both transports exist in this build.
                    #[cfg(not(feature = "_no_usb"))]
                    crate::state::toggle_preferred().await;
                } else if id == NUM_BLE_PROFILE as u8 + 5 {
                    // PG1KB clear all BLE profiles. Mirrors ZMK BT_CLR_ALL.
                    // PG1KB clear all BLE profiles
                    for slot in 0..NUM_BLE_PROFILE as u8 {
                        BLE_PROFILE_CHANNEL.send(BleProfileAction::ClearSlot(slot)).await;
                    }
                }
"""

if needle not in text:
    print("Could not find RMK BLE profile action block", file=sys.stderr)
    raise SystemExit(1)

path.write_text(text.replace(needle, replacement, 1))
print(f"Patched PG1KB BLE profile keys: {path}")
PY

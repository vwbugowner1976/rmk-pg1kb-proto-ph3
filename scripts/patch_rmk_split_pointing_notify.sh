#!/usr/bin/env bash
set -euo pipefail

if [ -z "${PG1KB_RMK_ROOT:-}" ]; then
  echo "PG1KB_RMK_ROOT is not set" >&2
  exit 1
fi

FILE="$PG1KB_RMK_ROOT/src/split/ble/peripheral.rs"
python3 - "$FILE" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
s = p.read_text()
old = """.notify(self.conn, &gatt_msg, true)"""
if old not in s:
    raise SystemExit(f"Could not find split BLE notify call in {p}")
new = """.notify(self.conn, &gatt_msg, !matches!(message, SplitMessage::Pointing(_)))"""
s = s.replace(old, new, 1)
p.write_text(s)
print("Patched RMK split Pointing to unconfirmed notification; keys remain indicated")
print(f"File: {p}")
PY

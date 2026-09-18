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

marker = "PG1KB process_inner action"
if marker in text:
    print("PG1KB process_inner action diagnostic already patched")
    raise SystemExit(0)

needle = """        // Process key
        let key_action = &self.keymap.get_action_with_layer_cache(event);
"""

replacement = """        // Process key
        let key_action = &self.keymap.get_action_with_layer_cache(event);
        if let KeyboardEventPos::Key(key_pos) = event.pos {
            if key_pos.col < 6 {
                info!(
                    "PG1KB process_inner action pos=({},{}) pressed={} action={:?}",
                    key_pos.row,
                    key_pos.col,
                    event.pressed,
                    key_action
                );
            }
        }
"""

if needle not in text:
    print("Could not find process_inner key_action resolution block", file=sys.stderr)
    for i, line in enumerate(text.splitlines(), 1):
        if "get_action_with_layer_cache" in line or "Process key" in line:
            print(f"{i}: {line}", file=sys.stderr)
    raise SystemExit(1)

path.write_text(text.replace(needle, replacement, 1))
print(f"Patched process_inner resolved action logging: {path}")
PY

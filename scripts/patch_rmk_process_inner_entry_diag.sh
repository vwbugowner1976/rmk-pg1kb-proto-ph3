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

marker = "PG1KB process_inner ENTER"
if marker in text:
    print("PG1KB process_inner entry diagnostic already patched")
    raise SystemExit(0)

sig = """    pub async fn process_inner(&mut self, event: KeyboardEvent) {
"""
entry = """    pub async fn process_inner(&mut self, event: KeyboardEvent) {
        if let KeyboardEventPos::Key(key_pos) = event.pos {
            if key_pos.col < 6 {
                info!(
                    "PG1KB process_inner ENTER pos=({},{}) pressed={}",
                    key_pos.row,
                    key_pos.col,
                    event.pressed
                );
            }
        }
"""

if sig not in text:
    print("Could not find process_inner function signature", file=sys.stderr)
    for i, line in enumerate(text.splitlines(), 1):
        if "process_inner" in line:
            print(f"{i}: {line}", file=sys.stderr)
    raise SystemExit(1)

text = text.replace(sig, entry, 1)

needle = """        let key_action = &self.keymap.get_action_with_layer_cache(event);
"""
replacement = """        let key_action = &self.keymap.get_action_with_layer_cache(event);
        if let KeyboardEventPos::Key(key_pos) = event.pos {
            if key_pos.col < 6 {
                info!(
                    "PG1KB process_inner RESOLVED pos=({},{}) pressed={} action={:?}",
                    key_pos.row,
                    key_pos.col,
                    event.pressed,
                    key_action
                );
            }
        }
"""

if needle in text and "PG1KB process_inner RESOLVED" not in text:
    text = text.replace(needle, replacement, 1)

path.write_text(text)
print(f"Patched process_inner entry/action logging: {path}")
PY

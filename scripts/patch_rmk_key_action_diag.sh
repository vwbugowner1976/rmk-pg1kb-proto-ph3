#!/bin/bash
set -euo pipefail

ROOT="${PG1KB_RMK_ROOT:?PG1KB_RMK_ROOT is required}"
TARGET="$ROOT/src/keymap.rs"

if [ ! -f "$TARGET" ]; then
  echo "RMK keymap source not found: $TARGET" >&2
  exit 1
fi

python3 - "$TARGET" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()

marker = "PG1KB resolved left key"
if marker in text:
    print("PG1KB key-action diagnostic already patched")
    raise SystemExit(0)

start = text.find("fn get_action_with_layer_cache")
if start < 0:
    print("Could not find get_action_with_layer_cache()", file=sys.stderr)
    raise SystemExit(1)

end = text.find("\n    fn ", start + 1)
if end < 0:
    end = len(text)

block = text[start:end]
needle = """                self.save_layer_cache(event.pos, layer_idx as u8);
                return action;
"""

replacement = """                self.save_layer_cache(event.pos, layer_idx as u8);
                if let KeyboardEventPos::Key(key_pos) = event.pos {
                    if key_pos.col < 6 {
                        info!(
                            "PG1KB resolved left key pos=({},{}) layer={} action={:?}",
                            key_pos.row,
                            key_pos.col,
                            layer_idx,
                            action
                        );
                    }
                }
                return action;
"""

if needle not in block:
    print("Could not find resolved-action return site inside get_action_with_layer_cache()", file=sys.stderr)
    raise SystemExit(1)

patched_block = block.replace(needle, replacement, 1)
path.write_text(text[:start] + patched_block + text[end:])
print(f"Patched resolved left-key action logging: {path}")
PY

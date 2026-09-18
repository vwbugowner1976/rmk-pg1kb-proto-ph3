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

marker = "PG1KB keyboard event"
if marker in text:
    print("PG1KB keyboard-event diagnostic already patched")
    raise SystemExit(0)

needle = """            match event {
                Some(event) => self.process_inner(event).await,
                None => self.fire_expired().await,
            }
"""

replacement = """            match event {
                Some(event) => {
                    if let crate::event::KeyboardEventPos::Key(key_pos) = event.pos {
                        if key_pos.col < 6 {
                            info!(
                                "PG1KB keyboard event pos=({},{}) pressed={}",
                                key_pos.row,
                                key_pos.col,
                                event.pressed
                            );
                        }
                    }
                    self.process_inner(event).await
                }
                None => self.fire_expired().await,
            }
"""

if needle not in text:
    print("Could not find Keyboard::run event dispatch block", file=sys.stderr)
    raise SystemExit(1)

path.write_text(text.replace(needle, replacement, 1))
print(f"Patched keyboard event logging: {path}")
PY

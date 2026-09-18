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

# RMK 0.9.0 and current main use different Keyboard::run() dispatch shapes.
# Patch either the direct process_inner(event) call used by 0.9.0 or the
# deadline-aware match used by newer RMK.
direct = "            self.process_inner(event).await;\n"
direct_replacement = """            if let crate::event::KeyboardEventPos::Key(key_pos) = event.pos {
                if key_pos.col < 6 {
                    info!(
                        "PG1KB keyboard event pos=({},{}) pressed={}",
                        key_pos.row,
                        key_pos.col,
                        event.pressed
                    );
                }
            }
            self.process_inner(event).await;
"""

newer = """            match event {
                Some(event) => self.process_inner(event).await,
                None => self.fire_expired().await,
            }
"""
newer_replacement = """            match event {
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

if newer in text:
    text = text.replace(newer, newer_replacement, 1)
elif direct in text:
    text = text.replace(direct, direct_replacement, 1)
else:
    print("Could not find Keyboard::run process_inner(event) call", file=sys.stderr)
    # Print nearby candidates to make the next mismatch self-diagnosing.
    for i, line in enumerate(text.splitlines(), 1):
        if "process_inner" in line or "keyboard_event_subscriber" in line:
            print(f"{i}: {line}", file=sys.stderr)
    raise SystemExit(1)

path.write_text(text)
print(f"Patched keyboard event logging: {path}")
PY

#!/bin/bash
set -euo pipefail

ROOT="${PG1KB_RMK_ROOT:?PG1KB_RMK_ROOT is required}"
TARGET="$ROOT/src/split/driver.rs"

if [ ! -f "$TARGET" ]; then
  echo "RMK split driver not found: $TARGET" >&2
  exit 1
fi

python3 - "$TARGET" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()

needle = """                    publish_event_async(KeyboardEvent::key(
                        key_pos.row + self.matrix_config.row_offset,
                        key_pos.col + self.matrix_config.col_offset,
                        e.pressed,
                    ))
                    .await;
"""

replacement = """                    let mapped_row = key_pos.row + self.matrix_config.row_offset;
                    let mapped_col = key_pos.col + self.matrix_config.col_offset;
                    info!(
                        "PG1KB split key peri={} local=({},{}) mapped=({},{}) pressed={}",
                        self.id,
                        key_pos.row,
                        key_pos.col,
                        mapped_row,
                        mapped_col,
                        e.pressed
                    );
                    publish_event_async(KeyboardEvent::key(
                        mapped_row,
                        mapped_col,
                        e.pressed,
                    ))
                    .await;
"""

if "PG1KB split key peri=" in text:
    print("PG1KB split key diagnostic already patched")
    raise SystemExit(0)

if needle not in text:
    print("Could not find RMK split key forwarding block to patch", file=sys.stderr)
    raise SystemExit(1)

path.write_text(text.replace(needle, replacement, 1))
print(f"Patched split key coordinate logging: {path}")
PY

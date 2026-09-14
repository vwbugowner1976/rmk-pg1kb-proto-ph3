#!/usr/bin/env bash
set -euo pipefail

TARGET_MS="${1:-50}"

if ! [[ "$TARGET_MS" =~ ^[0-9]+$ ]] || [ "$TARGET_MS" -lt 20 ] || [ "$TARGET_MS" -gt 500 ]; then
  echo "usage: $0 [20..500]" >&2
  exit 2
fi

# RMK is currently consumed from crates.io (0.9.x). This helper intentionally
# patches only the local Cargo registry copy for A/B testing. It does NOT modify
# upstream RMK and does not change this repository's Cargo.toml.
mapfile -t CANDIDATES < <(
  find "$HOME/.cargo/registry/src" -type f \
    \( -path '*/rmk-0.9.*/src/ble/adv.rs' -o -path '*/rmk-0.9.*/rmk/src/ble/adv.rs' \) \
    2>/dev/null | sort
)

if [ "${#CANDIDATES[@]}" -eq 0 ]; then
  echo "RMK 0.9 adv.rs not found in Cargo registry." >&2
  echo "Run 'cargo fetch' or one normal build first, then retry." >&2
  exit 1
fi

ADV_RS="${CANDIDATES[-1]}"
BACKUP="${ADV_RS}.pg1kb-backup"

if [ ! -f "$BACKUP" ]; then
  cp "$ADV_RS" "$BACKUP"
fi

python3 - "$ADV_RS" "$TARGET_MS" <<'PY'
from pathlib import Path
import re, sys

path = Path(sys.argv[1])
target = int(sys.argv[2])
text = path.read_text()

# Match the host advertising tuple without touching split/peripheral advertising.
pat = re.compile(r'(Self::Host\s*\{\s*\.\.\s*\}\s*=>\s*\(\s*PhyKind::Le2M\s*,\s*Duration::from_millis\()\d+(\)\s*\))')
m = pat.search(text)
if not m:
    # Older 0.9 snapshots may use a simpler host-only parameter assignment.
    # In that case, accept exactly one 200ms advertising interval occurrence.
    occurrences = list(re.finditer(r'Duration::from_millis\(200\)', text))
    if len(occurrences) != 1:
        print(f"Could not safely locate unique RMK host advertising interval in {path}", file=sys.stderr)
        print("No file was changed.", file=sys.stderr)
        sys.exit(3)
    text = text[:occurrences[0].start()] + f'Duration::from_millis({target})' + text[occurrences[0].end():]
else:
    text = text[:m.start()] + m.group(1) + str(target) + m.group(2) + text[m.end():]

path.write_text(text)
print(f"Patched RMK host advertising interval -> {target} ms")
print(f"File: {path}")
PY

echo
echo "Verify with:"
echo "  grep -n -C2 'from_millis(${TARGET_MS})' '$ADV_RS'"
echo
echo "Restore with:"
echo "  cp '$BACKUP' '$ADV_RS'"

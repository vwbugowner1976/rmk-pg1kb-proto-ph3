#!/usr/bin/env bash
set -euo pipefail

TARGET_LATENCY="${1:-0}"

if ! [[ "$TARGET_LATENCY" =~ ^[0-9]+$ ]] || [ "$TARGET_LATENCY" -gt 30 ]; then
  echo "usage: $0 [0..30]" >&2
  exit 2
fi

# RMK is consumed from crates.io (0.9.x). Patch only the local Cargo registry
# copy for hardware A/B testing. This keeps upstream/main untouched.
mapfile -t CANDIDATES < <(
  find "$HOME/.cargo/registry/src" -type f \
    \( -path '*/rmk-0.9.*/src/split/ble/central.rs' -o -path '*/rmk-0.9.*/rmk/src/split/ble/central.rs' \) \
    2>/dev/null | sort
)

if [ "${#CANDIDATES[@]}" -eq 0 ]; then
  echo "RMK 0.9 split/ble/central.rs not found in Cargo registry." >&2
  echo "Run 'cargo fetch' or one normal build first, then retry." >&2
  exit 1
fi

CENTRAL_RS="${CANDIDATES[-1]}"
BACKUP="${CENTRAL_RS}.pg1kb-backup"

if [ ! -f "$BACKUP" ]; then
  cp "$CENTRAL_RS" "$BACKUP"
fi

python3 - "$CENTRAL_RS" "$TARGET_LATENCY" <<'PY'
from pathlib import Path
import re, sys

path = Path(sys.argv[1])
target = int(sys.argv[2])
text = path.read_text()

# Only patch default_split_conn_params(). Do not change the sleep parameters.
fn = re.search(
    r'(fn\s+default_split_conn_params\s*\(\s*\)\s*->\s*RequestedConnParams\s*\{)(.*?)(\n\})',
    text,
    flags=re.S,
)
if not fn:
    print(f"Could not locate default_split_conn_params() in {path}", file=sys.stderr)
    sys.exit(3)

body = fn.group(2)
pat = re.compile(r'(max_latency\s*:\s*)\d+')
m = pat.search(body)
if not m:
    print(f"Could not locate max_latency in default_split_conn_params() in {path}", file=sys.stderr)
    sys.exit(4)

new_body = body[:m.start()] + m.group(1) + str(target) + body[m.end():]
new_text = text[:fn.start(2)] + new_body + text[fn.end(2):]
path.write_text(new_text)

print(f"Patched RMK active split max_latency -> {target}")
print("Connection interval remains unchanged (normally 7.5 ms).")
print(f"File: {path}")
PY

echo
echo "Verify with:"
echo "  grep -n -A12 'fn default_split_conn_params' '$CENTRAL_RS'"
echo
echo "Restore with:"
echo "  cp '$BACKUP' '$CENTRAL_RS'"

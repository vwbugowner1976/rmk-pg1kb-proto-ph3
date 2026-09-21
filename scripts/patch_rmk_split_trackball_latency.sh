#!/usr/bin/env bash
set -euo pipefail

TARGET_LATENCY="${1:-0}"

if ! [[ "$TARGET_LATENCY" =~ ^[0-9]+$ ]] || [ "$TARGET_LATENCY" -gt 30 ]; then
  echo "usage: $0 [0..30]" >&2
  exit 2
fi

if [ -n "${PG1KB_RMK_ROOT:-}" ]; then
  CENTRAL_RS="$PG1KB_RMK_ROOT/src/split/ble/central.rs"
else
  mapfile -t CANDIDATES < <(
    find "$HOME/.cargo/registry/src" -type f \
      \( -path '*/rmk-0.9.*/src/split/ble/central.rs' -o -path '*/rmk-0.9.*/rmk/src/split/ble/central.rs' \) \
      2>/dev/null | sort
  )
  if [ "${#CANDIDATES[@]}" -eq 0 ]; then
    echo "RMK 0.9 split/ble/central.rs not found." >&2
    exit 1
  fi
  CENTRAL_RS="${CANDIDATES[-1]}"
fi

[ -f "$CENTRAL_RS" ] || { echo "RMK split central.rs not found: $CENTRAL_RS" >&2; exit 1; }

python3 - "$CENTRAL_RS" "$TARGET_LATENCY" <<'PY'
from pathlib import Path
import re, sys

path = Path(sys.argv[1])
target = int(sys.argv[2])
text = path.read_text()
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
new_body = re.sub(
    r'min_connection_interval\s*:\s*Duration::from_micros\(\d+\)',
    'min_connection_interval: Duration::from_micros(10000)',
    new_body,
    count=1,
)
new_body = re.sub(
    r'max_connection_interval\s*:\s*Duration::from_micros\(\d+\)',
    'max_connection_interval: Duration::from_micros(10000)',
    new_body,
    count=1,
)
path.write_text(text[:fn.start(2)] + new_body + text[fn.end(2):])
print(f"Patched RMK active split max_latency -> {target}")
print("Patched RMK active split connection interval -> 10 ms.")
print(f"File: {path}")
PY

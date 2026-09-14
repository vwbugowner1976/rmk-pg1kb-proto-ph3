#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

ai-build cargo make uf2-v5 --release

OUT=/mnt/d/rmk-firmware
mkdir -p "$OUT"

cp -v rmk-pg1kb-proto-ph3-central-v5.uf2 "$OUT/"
cp -v rmk-pg1kb-proto-ph3-peripheral-v5.uf2 "$OUT/"

echo
echo "=== SHA256 ==="
sha256sum rmk-pg1kb-proto-ph3-central-v5.uf2
sha256sum "$OUT/rmk-pg1kb-proto-ph3-central-v5.uf2"
sha256sum rmk-pg1kb-proto-ph3-peripheral-v5.uf2
sha256sum "$OUT/rmk-pg1kb-proto-ph3-peripheral-v5.uf2"

echo
echo 'DONE:'
echo '  D:\rmk-firmware\rmk-pg1kb-proto-ph3-central-v5.uf2'
echo '  D:\rmk-firmware\rmk-pg1kb-proto-ph3-peripheral-v5.uf2'

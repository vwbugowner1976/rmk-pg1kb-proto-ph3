#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

# Keep the official RMK 0.9 dependency while applying the hardware-verified
# PG1KB transport tuning and the tiny application-owned Rynk extension hook.
# Invoke helpers through bash so executable-bit changes are never required.
cargo fetch
bash scripts/patch_rmk_fast_host_adv.sh 50
bash scripts/patch_rmk_split_trackball_latency.sh 0
bash scripts/patch_rmk_rynk_custom_hook.sh

ai-build cargo make uf2-v7 --release

OUT=/mnt/d/rmk-firmware
mkdir -p "$OUT"

cp -v rmk-pg1kb-proto-ph3-central-v7.uf2 "$OUT/"
cp -v rmk-pg1kb-proto-ph3-peripheral-v7.uf2 "$OUT/"

echo
echo "=== SHA256 ==="
sha256sum rmk-pg1kb-proto-ph3-central-v7.uf2
sha256sum "$OUT/rmk-pg1kb-proto-ph3-central-v7.uf2"
sha256sum rmk-pg1kb-proto-ph3-peripheral-v7.uf2
sha256sum "$OUT/rmk-firmware/rmk-pg1kb-proto-ph3-peripheral-v7.uf2" 2>/dev/null || sha256sum "$OUT/rmk-pg1kb-proto-ph3-peripheral-v7.uf2"

echo
echo 'DONE:'
echo '  D:\rmk-firmware\rmk-pg1kb-proto-ph3-central-v7.uf2'
echo '  D:\rmk-firmware\rmk-pg1kb-proto-ph3-peripheral-v7.uf2'

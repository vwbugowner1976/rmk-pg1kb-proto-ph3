#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

# Build against a project-local copy of the official crates.io RMK 0.9.0
# source. This avoids mutating ~/.cargo/registry/src and guarantees Cargo sees
# the PG1KB patches as a normal path dependency.
REGISTRY_RMK="$(find "$HOME/.cargo/registry/src" -maxdepth 2 -type d -name 'rmk-0.9.0' 2>/dev/null | sort | tail -n 1)"
if [ -z "$REGISTRY_RMK" ] || [ ! -f "$REGISTRY_RMK/Cargo.toml" ]; then
    echo "Official RMK 0.9.0 source not found in Cargo registry." >&2
    echo "Run one normal RMK 0.9 build/fetch first, then retry." >&2
    exit 1
fi

LOCAL_RMK="$PWD/.rmk-patched/rmk-0.9.0"
rm -rf "$LOCAL_RMK"
mkdir -p "$(dirname "$LOCAL_RMK")"
cp -a "$REGISTRY_RMK" "$LOCAL_RMK"
export PG1KB_RMK_ROOT="$LOCAL_RMK"

echo "Using project-local official RMK copy:"
echo "  $LOCAL_RMK"

bash scripts/patch_rmk_fast_host_adv.sh 50
bash scripts/patch_rmk_split_trackball_latency.sh 0
bash scripts/patch_rmk_split_key_diag.sh
bash scripts/patch_rmk_rynk_custom_hook.sh
bash scripts/patch_rmk_pg1kb_runtime.sh

# Cargo.toml points at .rmk-patched/rmk-0.9.0, so this is guaranteed to build
# the freshly patched source rather than a stale crates.io rlib.
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
sha256sum "$OUT/rmk-pg1kb-proto-ph3-peripheral-v7.uf2"

echo
echo 'DONE:'
echo '  D:\rmk-firmware\rmk-pg1kb-proto-ph3-central-v7.uf2'
echo '  D:\rmk-firmware\rmk-pg1kb-proto-ph3-peripheral-v7.uf2'

#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

# Generate stock RMK keyboard.toml from the reusable JP-key source.
# The helper repo is pinned for reproducible local builds and cached locally.
JPKEYS_REV="2507c18f5d5526b12cf0bc75eb5796e2aeda8612"
JPKEYS_DIR="$PWD/.cache/rmk-jpkeys-for-us-layout"
JPKEYS_URL="https://github.com/vwbugowner1976/rmk-jpkeys-for-us-layout.git"

if [ ! -d "$JPKEYS_DIR/.git" ]; then
    rm -rf "$JPKEYS_DIR"
    mkdir -p "$(dirname "$JPKEYS_DIR")"
    git clone --filter=blob:none "$JPKEYS_URL" "$JPKEYS_DIR"
fi

if ! git -C "$JPKEYS_DIR" cat-file -e "$JPKEYS_REV^{commit}" 2>/dev/null; then
    git -C "$JPKEYS_DIR" fetch --depth=1 origin "$JPKEYS_REV"
fi
git -C "$JPKEYS_DIR" checkout --detach --quiet "$JPKEYS_REV"

python3 "$JPKEYS_DIR/tools/apply_jpkeys.py" keyboard.jp.toml keyboard.toml


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
bash scripts/patch_rmk_split_pointing_notify.sh
bash scripts/patch_rmk_rynk_custom_hook.sh
bash scripts/patch_rmk_pg1kb_ble_keys.sh
bash scripts/patch_rmk_pg1kb_runtime.sh
bash scripts/patch_rmk_position_combos.sh

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

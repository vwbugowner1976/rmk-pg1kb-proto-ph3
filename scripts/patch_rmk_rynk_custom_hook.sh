#!/usr/bin/env bash
set -euo pipefail

# Resolve the exact RMK package selected by this project's Cargo.lock/metadata.
RMK_MANIFEST="$(cargo metadata --format-version 1 --locked 2>/dev/null | python3 -c '
import json, sys
m = json.load(sys.stdin)
for p in m["packages"]:
    if p["name"] == "rmk" and p["version"].startswith("0.9."):
        print(p["manifest_path"])
        break
')"

if [ -z "$RMK_MANIFEST" ] || [ ! -f "$RMK_MANIFEST" ]; then
  echo "Active RMK 0.9 manifest not found via cargo metadata." >&2
  exit 1
fi

RMK_ROOT="$(dirname "$RMK_MANIFEST")"
RYNK_RS="$RMK_ROOT/src/host/rynk/mod.rs"
HOST_MOD_RS="$RMK_ROOT/src/host/mod.rs"
BACKUP="${RYNK_RS}.pg1kb-backup"
HOST_MOD_BACKUP="${HOST_MOD_RS}.pg1kb-backup"

[ -f "$BACKUP" ] || cp "$RYNK_RS" "$BACKUP"
[ -f "$HOST_MOD_BACKUP" ] || cp "$HOST_MOD_RS" "$HOST_MOD_BACKUP"

python3 - "$RYNK_RS" "$HOST_MOD_RS" <<'PY'
from pathlib import Path
import sys

rynk = Path(sys.argv[1])
host = Path(sys.argv[2])
text = rynk.read_text()
marker = "PG1KB_RYNK_CUSTOM_HOOK_V1"

if marker not in text:
    import_anchor = "use embassy_futures::select::{Either, select};\n"
    const_anchor = "const RYNK_UNLOCK_WINDOW: embassy_time::Duration = embassy_time::Duration::from_millis(500);\n"
    dispatch_anchor = "        if self.requires_unlock(cmd) && !locker.is_unlocked() {\n            return Err(RynkError::Locked);\n        }\n\n"
    for anchor, code in [
        (import_anchor, 2),
        (const_anchor, 3),
        (dispatch_anchor, 4),
    ]:
        if anchor not in text:
            print(f"Could not locate required Rynk anchor in {rynk}", file=sys.stderr)
            raise SystemExit(code)

    text = text.replace(
        import_anchor,
        import_anchor + "use core::cell::RefCell;\nuse embassy_sync::blocking_mutex::Mutex;\n",
        1,
    )
    hook = r'''

// PG1KB_RYNK_CUSTOM_HOOK_V1
pub type CustomRynkHandler =
    for<'a> fn(&mut RynkMessage<'a>) -> Option<Result<(), RynkError>>;

static CUSTOM_RYNK_HANDLER: Mutex<crate::RawMutex, RefCell<Option<CustomRynkHandler>>> =
    Mutex::new(RefCell::new(None));

pub fn register_custom_handler(handler: CustomRynkHandler) {
    CUSTOM_RYNK_HANDLER.lock(|slot| {
        *slot.borrow_mut() = Some(handler);
    });
}

fn dispatch_custom(msg: &mut RynkMessage<'_>) -> Option<Result<(), RynkError>> {
    CUSTOM_RYNK_HANDLER.lock(|slot| {
        let handler = *slot.borrow();
        handler.and_then(|handler| handler(msg))
    })
}
'''
    text = text.replace(const_anchor, const_anchor + hook, 1)
    text = text.replace(
        dispatch_anchor,
        dispatch_anchor + "        if let Some(result) = dispatch_custom(msg) {\n            return result;\n        }\n\n",
        1,
    )
    rynk.write_text(text)
    print("Patched active RMK Rynk custom-command hook")
else:
    print(f"Rynk custom hook already present in active RMK: {rynk}")

# Do not depend on host::rynk visibility. Re-export only the public hook from
# the already-public host module, so application code can call rmk::host::register_custom_handler.
host_text = host.read_text()
reexport = '#[cfg(feature = "rynk")]\npub use rynk::register_custom_handler;'
if reexport not in host_text:
    anchor = '#[cfg(feature = "rynk")]\npub use rynk::RynkService as HostService;'
    if anchor not in host_text:
        print(f"Could not locate Rynk HostService export in {host}", file=sys.stderr)
        raise SystemExit(5)
    host_text = host_text.replace(anchor, reexport + "\n" + anchor, 1)
    host.write_text(host_text)
    print("Re-exported RMK Rynk custom handler from rmk::host")
else:
    print(f"RMK Rynk custom handler already re-exported: {host}")

verify_host = host.read_text()
verify_rynk = rynk.read_text()
if reexport not in verify_host:
    print("VERIFY FAILED: rmk::host custom-handler re-export missing", file=sys.stderr)
    raise SystemExit(6)
if marker not in verify_rynk:
    print("VERIFY FAILED: active RMK custom hook missing", file=sys.stderr)
    raise SystemExit(7)

print("VERIFY OK: active RMK custom Rynk hook + host re-export")
print(f"Active RMK root: {rynk.parents[2]}")
PY

echo
echo "Verify with:"
echo "  grep -n -C2 'PG1KB_RYNK_CUSTOM_HOOK_V1' '$RYNK_RS'"
echo "  grep -n -C1 'register_custom_handler' '$HOST_MOD_RS'"

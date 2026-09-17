#!/usr/bin/env bash
set -euo pipefail

mapfile -t CANDIDATES < <(
  find "$HOME/.cargo/registry/src" -type f \
    -path '*/rmk-0.9.*/src/host/rynk/mod.rs' 2>/dev/null | sort
)

if [ "${#CANDIDATES[@]}" -eq 0 ]; then
  echo "RMK 0.9 Rynk source not found in Cargo registry." >&2
  echo "Run 'cargo fetch' or one normal build first, then retry." >&2
  exit 1
fi

RYNK_RS="${CANDIDATES[-1]}"
HOST_MOD_RS="$(dirname "$(dirname "$RYNK_RS")")/mod.rs"
BACKUP="${RYNK_RS}.pg1kb-backup"
HOST_MOD_BACKUP="${HOST_MOD_RS}.pg1kb-backup"

if [ ! -f "$BACKUP" ]; then
  cp "$RYNK_RS" "$BACKUP"
fi
if [ ! -f "$HOST_MOD_BACKUP" ]; then
  cp "$HOST_MOD_RS" "$HOST_MOD_BACKUP"
fi

python3 - "$RYNK_RS" "$HOST_MOD_RS" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
host_mod = Path(sys.argv[2])
text = path.read_text()

marker = "PG1KB_RYNK_CUSTOM_HOOK_V1"
if marker not in text:
    import_anchor = "use embassy_futures::select::{Either, select};\n"
    if import_anchor not in text:
        print(f"Could not locate Rynk import anchor in {path}", file=sys.stderr)
        raise SystemExit(2)
    text = text.replace(
        import_anchor,
        import_anchor + "use core::cell::RefCell;\nuse embassy_sync::blocking_mutex::Mutex;\n",
        1,
    )

    const_anchor = "const RYNK_UNLOCK_WINDOW: embassy_time::Duration = embassy_time::Duration::from_millis(500);\n"
    if const_anchor not in text:
        print(f"Could not locate RYNK_UNLOCK_WINDOW anchor in {path}", file=sys.stderr)
        raise SystemExit(3)

    hook = r'''

// PG1KB_RYNK_CUSTOM_HOOK_V1
// Small application-owned extension point for commands outside RMK's standard table.
// A handler returns None when it does not own the command, or Some(Result) when it does.
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

    dispatch_anchor = "        if self.requires_unlock(cmd) && !locker.is_unlocked() {\n            return Err(RynkError::Locked);\n        }\n\n"
    if dispatch_anchor not in text:
        print(f"Could not locate dispatch unlock anchor in {path}", file=sys.stderr)
        raise SystemExit(4)
    text = text.replace(
        dispatch_anchor,
        dispatch_anchor + "        if let Some(result) = dispatch_custom(msg) {\n            return result;\n        }\n\n",
        1,
    )
    path.write_text(text)
    print("Patched RMK Rynk application custom-command hook")
else:
    print(f"Rynk custom hook already present: {path}")

# RMK 0.9 keeps host::rynk crate-private. PG1KB needs to call the tiny
# register_custom_handler() extension from the application crate.
host_text = host_mod.read_text()
private_decl = '#[cfg(feature = "rynk")]\npub(crate) mod rynk;'
public_decl = '#[cfg(feature = "rynk")]\npub mod rynk;'
if public_decl in host_text:
    print(f"Rynk module already public: {host_mod}")
elif private_decl in host_text:
    host_text = host_text.replace(private_decl, public_decl, 1)
    host_mod.write_text(host_text)
    print("Exposed RMK host::rynk module for PG1KB custom handler registration")
else:
    print(f"Could not locate private Rynk module declaration in {host_mod}", file=sys.stderr)
    raise SystemExit(5)

print(f"Rynk file: {path}")
print(f"Host mod : {host_mod}")
PY

echo
echo "Verify with:"
echo "  grep -n -C2 'PG1KB_RYNK_CUSTOM_HOOK_V1' '$RYNK_RS'"
echo "  grep -n -C1 'pub mod rynk' '$HOST_MOD_RS'"
echo
echo "Restore with:"
echo "  cp '$BACKUP' '$RYNK_RS'"
echo "  cp '$HOST_MOD_BACKUP' '$HOST_MOD_RS'"

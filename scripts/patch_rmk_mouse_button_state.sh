#!/usr/bin/env bash
set -euo pipefail

ROOT="${PG1KB_RMK_ROOT:?PG1KB_RMK_ROOT is required}"
CHANNEL="$ROOT/src/channel.rs"
KEYBOARD="$ROOT/src/keyboard.rs"

python3 - "$CHANNEL" "$KEYBOARD" <<'PY'
from pathlib import Path
import sys

channel = Path(sys.argv[1])
keyboard = Path(sys.argv[2])

c = channel.read_text()
needle = "use core::future::poll_fn;\n"
insert = """use core::future::poll_fn;
use core::sync::atomic::{AtomicU8, Ordering};
"""
if "pub static MOUSE_BUTTON_STATE" not in c:
    if needle not in c:
        raise SystemExit("channel.rs import anchor not found")
    c = c.replace(needle, insert, 1)
    anchor = "type ReportChannel = Channel<RawMutex, Report, REPORT_CHANNEL_SIZE>;\n"
    block = """type ReportChannel = Channel<RawMutex, Report, REPORT_CHANNEL_SIZE>;

/// Current RMK mouse-button bitmask. Custom pointing producers must preserve
/// this state in every MouseReport so holding MouseBtn1 continues to drag.
pub static MOUSE_BUTTON_STATE: AtomicU8 = AtomicU8::new(0);

pub fn mouse_button_state() -> u8 {
    MOUSE_BUTTON_STATE.load(Ordering::Relaxed)
}

pub fn set_mouse_button_state(buttons: u8) {
    MOUSE_BUTTON_STATE.store(buttons, Ordering::Relaxed);
}
"""
    if anchor not in c:
        raise SystemExit("channel.rs report channel anchor not found")
    c = c.replace(anchor, block, 1)
channel.write_text(c)

k = keyboard.read_text()
old = "self.keymap.set_mouse_buttons(self.mouse.report.buttons);"
new = """self.keymap.set_mouse_buttons(self.mouse.report.buttons);
        crate::channel::set_mouse_button_state(self.mouse.report.buttons);"""
count = k.count(old)
if count < 2:
    raise SystemExit(f"keyboard.rs expected >=2 mouse button sync sites, found {count}")
k = k.replace(old, new)
keyboard.write_text(k)

print("Patched RMK mouse button state bridge for custom pointing reports")
PY

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

# Normalize mouse button bits at the final HID writer, not only when a custom
# trackball report is queued. This prevents a stale pre-click motion report
# already sitting in the queue from briefly sending buttons=0 after MouseBtn1
# was pressed (or buttons=1 after release), which Windows can interpret as
# multiple clicks and maximize a title bar instead of dragging it.
ble = Path(channel.parent / "ble" / "mod.rs")
if ble.exists():
    b = ble.read_text()
    old = """        loop {
            let report = BLE_REPORT_CHANNEL.receive().await;
            if let Err(e) = ble_hid_server.write_report(&report).await {
"""
    new = """        loop {
            let mut report = BLE_REPORT_CHANNEL.receive().await;
            if let crate::hid::Report::MouseReport(ref mut mouse) = report {
                mouse.buttons = crate::channel::mouse_button_state();
            }
            if let Err(e) = ble_hid_server.write_report(&report).await {
"""
    if old not in b and "mouse.buttons = crate::channel::mouse_button_state();" not in b:
        raise SystemExit("ble/mod.rs writer anchor not found")
    if old in b:
        b = b.replace(old, new, 1)
    ble.write_text(b)

usb = Path(channel.parent / "usb" / "mod.rs")
if usb.exists():
    u = usb.read_text()
    old = """        loop {
            let report = USB_REPORT_CHANNEL.receive().await;

            // EndpointError::Disabled never fires"""
    new = """        loop {
            let mut report = USB_REPORT_CHANNEL.receive().await;
            if let crate::hid::Report::MouseReport(ref mut mouse) = report {
                mouse.buttons = crate::channel::mouse_button_state();
            }

            // EndpointError::Disabled never fires"""
    if old not in u and "mouse.buttons = crate::channel::mouse_button_state();" not in u:
        raise SystemExit("usb/mod.rs writer anchor not found")
    if old in u:
        u = u.replace(old, new, 1)
    usb.write_text(u)

print("Patched RMK mouse button state bridge and final HID writer normalization")
PY

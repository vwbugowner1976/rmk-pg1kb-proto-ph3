#![no_main]
#![no_std]

use rmk::macros::rmk_central;

// v3 intentionally uses the native PAW3222 input-device support from
// t-ogura/rmk pr/paw3222 via keyboard.toml. This removes the temporary
// USB_REPORT_CHANNEL-only processor used by v1/v2 and lets RMK route both
// keyboard and pointing reports through the active USB/BLE transport.
#[rmk_central]
mod keyboard_central {}

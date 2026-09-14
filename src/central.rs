#![no_main]
#![no_std]

mod paw3222;

use rmk::macros::rmk_central;

#[rmk_central]
mod keyboard_central {}

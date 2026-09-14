use rmk::event::{PointingSetCpiEvent, publish_event};

pub const RIGHT_TRACKBALL_ID: u8 = 0;
pub const LEFT_TRACKBALL_ID: u8 = 1;
pub const DEFAULT_CPI: u16 = 988;

/// Runtime CPI entry point intended for MyKeebStudio/RPC integration.
/// The custom PAW3222 processors subscribe to PointingSetCpiEvent.
pub fn set_pointing_cpi(device_id: u8, cpi: u16) {
    publish_event(PointingSetCpiEvent { device_id, cpi });
}

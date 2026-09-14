use rmk::event::{PointingSetCpiEvent, publish_event};

use crate::trackball_config::{LEFT_TRACKBALL_CONFIG, RIGHT_TRACKBALL_CONFIG, RuntimeTrackballConfig, SensorRotation};

pub const RIGHT_TRACKBALL_ID: u8 = 0;
pub const LEFT_TRACKBALL_ID: u8 = 1;
pub const DEFAULT_CPI: u16 = 988;

pub fn config(device_id: u8) -> &'static RuntimeTrackballConfig {
    if device_id == LEFT_TRACKBALL_ID {
        &LEFT_TRACKBALL_CONFIG
    } else {
        &RIGHT_TRACKBALL_CONFIG
    }
}

pub fn set_pointing_cpi(device_id: u8, cpi: u16) {
    config(device_id).set_cpi(cpi);
    publish_event(PointingSetCpiEvent { device_id, cpi });
}

pub fn set_cursor_gain_q8(device_id: u8, gain_q8: u16) {
    config(device_id).set_cursor_gain_q8(gain_q8.max(16));
}

pub fn set_scroll_scale_den(device_id: u8, den: u16) {
    config(device_id).set_scroll_scale_den(den.max(1));
}

pub fn set_inertia_enabled(device_id: u8, enabled: bool) {
    config(device_id).set_inertia_enabled(enabled);
}

pub fn set_inertia_decay(device_id: u8, num: u8, den: u8) {
    config(device_id).set_inertia_decay(num, den.max(1));
}

pub fn set_sensor_rotation(device_id: u8, rotation: SensorRotation) {
    config(device_id).set_rotation(rotation);
}

pub fn set_sensor_rotation_degrees(device_id: u8, degrees: u16) {
    let rotation = match degrees {
        90 => SensorRotation::Deg90,
        180 => SensorRotation::Deg180,
        270 => SensorRotation::Deg270,
        _ => SensorRotation::Deg0,
    };
    set_sensor_rotation(device_id, rotation);
}

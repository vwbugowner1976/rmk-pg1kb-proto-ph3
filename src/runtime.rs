use rmk::event::{PointingSetCpiEvent, publish_event};
use rmk::types::protocol::rynk::{RynkError, RynkMessage};

use crate::trackball_config::{LEFT_TRACKBALL_CONFIG, RIGHT_TRACKBALL_CONFIG, RuntimeTrackballConfig, SensorRotation};

pub const RIGHT_TRACKBALL_ID: u8 = 0;
pub const LEFT_TRACKBALL_ID: u8 = 1;
pub const DEFAULT_CPI: u16 = 988;

pub const RYNK_GET_TRACKBALL_CONFIG: u16 = 0x0901;
pub const RYNK_SET_TRACKBALL_CONFIG: u16 = 0x0902;
const TRACKBALL_CONFIG_WIRE_LEN: usize = 16;

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

fn encode_config(device_id: u8) -> [u8; TRACKBALL_CONFIG_WIRE_LEN] {
    let cfg = config(device_id);
    let (decay_num, decay_den) = cfg.inertia_decay();
    let mut out = [0u8; TRACKBALL_CONFIG_WIRE_LEN];
    out[0..2].copy_from_slice(&cfg.cpi().to_le_bytes());
    out[2..4].copy_from_slice(&cfg.cursor_gain_q8().to_le_bytes());
    out[4..6].copy_from_slice(&cfg.scroll_scale_den().to_le_bytes());
    out[6] = cfg.inertia_enabled() as u8;
    out[7] = decay_num;
    out[8] = decay_den;
    out[9] = cfg.rotation().raw();
    // Capability bits: CPI, cursor gain, scroll scale, inertia, rotation.
    // Left CPI is intentionally read-only until CPI routing to the peripheral is added.
    out[10] = if device_id == RIGHT_TRACKBALL_ID { 0b0001_1111 } else { 0b0001_1110 };
    // Mode hint used by MyKeebStudio: 0=cursor, 1=scroll.
    out[11] = if device_id == LEFT_TRACKBALL_ID { 1 } else { 0 };
    out
}

fn apply_config(device_id: u8, data: &[u8; TRACKBALL_CONFIG_WIRE_LEN]) {
    let cpi = u16::from_le_bytes([data[0], data[1]]).clamp(100, 5000);
    let cursor_gain_q8 = u16::from_le_bytes([data[2], data[3]]).clamp(16, 2048);
    let scroll_scale_den = u16::from_le_bytes([data[4], data[5]]).clamp(1, 64);
    let inertia_enabled = data[6] != 0;
    let decay_den = data[8].max(1);
    let decay_num = data[7].min(decay_den);
    let rotation = SensorRotation::from_raw(data[9]);

    if device_id == RIGHT_TRACKBALL_ID {
        set_pointing_cpi(device_id, cpi);
    }
    set_cursor_gain_q8(device_id, cursor_gain_q8);
    set_scroll_scale_den(device_id, scroll_scale_den);
    set_inertia_enabled(device_id, inertia_enabled);
    set_inertia_decay(device_id, decay_num, decay_den);
    set_sensor_rotation(device_id, rotation);
}

/// PG1KB private Rynk extension.
///
/// GET  0x0901 request: device_id:u8
/// GET  response: Result<[u8;16], RynkError>
/// SET  0x0902 request: [device_id, 16 config bytes]
/// SET  response: Result<(), RynkError>
pub fn handle_rynk_trackball(msg: &mut RynkMessage<'_>) -> Option<Result<(), RynkError>> {
    match msg.header().cmd.raw() {
        RYNK_GET_TRACKBALL_CONFIG => {
            let device_id = match msg.decode_request::<u8>() {
                Ok(value) if value <= LEFT_TRACKBALL_ID => value,
                Ok(_) => return Some(Err(RynkError::Malformed)),
                Err(error) => return Some(Err(error)),
            };
            let response = encode_config(device_id);
            Some(msg.encode_response(&response))
        }
        RYNK_SET_TRACKBALL_CONFIG => {
            let request = match msg.decode_request::<[u8; TRACKBALL_CONFIG_WIRE_LEN + 1]>() {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let device_id = request[0];
            if device_id > LEFT_TRACKBALL_ID {
                return Some(Err(RynkError::Malformed));
            }
            let mut data = [0u8; TRACKBALL_CONFIG_WIRE_LEN];
            data.copy_from_slice(&request[1..]);
            apply_config(device_id, &data);
            Some(msg.encode_response(&()))
        }
        _ => None,
    }
}

use core::sync::atomic::{AtomicBool, Ordering};

use rmk::event::{PointingSetCpiEvent, publish_event};
use rmk::types::protocol::rynk::{RynkError, RynkMessage};

use crate::trackball_config::{LEFT_TRACKBALL_CONFIG, RIGHT_TRACKBALL_CONFIG, RuntimeTrackballConfig, SensorRotation};

pub const RIGHT_TRACKBALL_ID: u8 = 0;
pub const LEFT_TRACKBALL_ID: u8 = 1;
pub const DEFAULT_CPI: u16 = 988;

pub const RYNK_GET_TRACKBALL_CONFIG: u16 = 0x0901;
pub const RYNK_SET_TRACKBALL_CONFIG: u16 = 0x0902;
pub const RYNK_SAVE_TRACKBALL_CONFIG: u16 = 0x0903;
pub const RYNK_LOAD_TRACKBALL_DEFAULTS: u16 = 0x0904;
const TRACKBALL_CONFIG_WIRE_LEN: usize = 16;
pub const TRACKBALL_PERSISTED_LEN: usize = TRACKBALL_CONFIG_WIRE_LEN * 2;

static SAVE_REQUESTED: AtomicBool = AtomicBool::new(false);

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
    // Both sides now support CPI; left is forwarded across the split link.
    out[10] = 0b0001_1111;
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

    set_pointing_cpi(device_id, cpi);
    set_cursor_gain_q8(device_id, cursor_gain_q8);
    set_scroll_scale_den(device_id, scroll_scale_den);
    set_inertia_enabled(device_id, inertia_enabled);
    set_inertia_decay(device_id, decay_num, decay_den);
    set_sensor_rotation(device_id, rotation);
}

pub fn encode_persisted_blob() -> [u8; TRACKBALL_PERSISTED_LEN] {
    let mut out = [0u8; TRACKBALL_PERSISTED_LEN];
    let right = encode_config(RIGHT_TRACKBALL_ID);
    let left = encode_config(LEFT_TRACKBALL_ID);
    out[..TRACKBALL_CONFIG_WIRE_LEN].copy_from_slice(&right);
    out[TRACKBALL_CONFIG_WIRE_LEN..].copy_from_slice(&left);
    out
}

pub fn apply_persisted_blob(data: &[u8; TRACKBALL_PERSISTED_LEN]) {
    let mut right = [0u8; TRACKBALL_CONFIG_WIRE_LEN];
    let mut left = [0u8; TRACKBALL_CONFIG_WIRE_LEN];
    right.copy_from_slice(&data[..TRACKBALL_CONFIG_WIRE_LEN]);
    left.copy_from_slice(&data[TRACKBALL_CONFIG_WIRE_LEN..]);
    apply_config(RIGHT_TRACKBALL_ID, &right);
    apply_config(LEFT_TRACKBALL_ID, &left);
}

pub fn load_defaults() {
    let right = [
        0xdc, 0x03, // 988 CPI
        0x00, 0x01, // 1.0x cursor gain
        0x06, 0x00, // scroll denominator 6
        0x00,       // inertia off
        0x0f, 0x10, // 15/16
        0x00,       // rotation 0
        0x1f,       // capabilities
        0x00,       // cursor mode
        0x00, 0x00, 0x00, 0x00,
    ];
    let left = [
        0xdc, 0x03, // 988 CPI
        0x00, 0x01, // 1.0x cursor gain
        0x06, 0x00, // scroll denominator 6
        0x01,       // inertia on
        0x0f, 0x10, // 15/16
        0x01,       // rotation 90 deg
        0x1f,       // capabilities
        0x01,       // scroll mode
        0x00, 0x00, 0x00, 0x00,
    ];
    apply_config(RIGHT_TRACKBALL_ID, &right);
    apply_config(LEFT_TRACKBALL_ID, &left);
}

pub fn request_save() {
    SAVE_REQUESTED.store(true, Ordering::Release);
}

pub fn take_save_request() -> bool {
    SAVE_REQUESTED.swap(false, Ordering::AcqRel)
}

/// PG1KB private Rynk extension.
///
/// GET      0x0901 request: device_id:u8
/// GET      response: Result<[u8;16], RynkError>
/// SET      0x0902 request: [device_id, 16 config bytes]
/// SET      response: Result<(), RynkError>
/// SAVE     0x0903 request: () ; queues flash persistence
/// DEFAULTS 0x0904 request: () ; restores firmware defaults live
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
        RYNK_SAVE_TRACKBALL_CONFIG => {
            if let Err(error) = msg.decode_request::<()>() {
                return Some(Err(error));
            }
            request_save();
            Some(msg.encode_response(&()))
        }
        RYNK_LOAD_TRACKBALL_DEFAULTS => {
            if let Err(error) = msg.decode_request::<()>() {
                return Some(Err(error));
            }
            load_defaults();
            Some(msg.encode_response(&()))
        }
        _ => None,
    }
}

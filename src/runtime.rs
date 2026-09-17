use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};

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
pub const RYNK_GET_SAVE_STATUS: u16 = 0x0905;
pub const RYNK_GET_TRACKBALL_STATE: u16 = 0x0906;
const TRACKBALL_CONFIG_WIRE_LEN: usize = 16;
pub const TRACKBALL_PERSISTED_LEN: usize = TRACKBALL_CONFIG_WIRE_LEN * 2;

pub const SAVE_IDLE: u8 = 0;
pub const SAVE_PENDING: u8 = 1;
pub const SAVE_OK: u8 = 2;
pub const SAVE_FAILED: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TrackballMode {
    Cursor = 0,
    Scroll = 1,
}

static SAVE_REQUESTED: AtomicBool = AtomicBool::new(false);
static SAVE_GENERATION: AtomicU16 = AtomicU16::new(0);
static SAVE_COMPLETED_GENERATION: AtomicU16 = AtomicU16::new(0);
static SAVE_STATUS: AtomicU8 = AtomicU8::new(SAVE_IDLE);
static ACTIVE_LAYER: AtomicU8 = AtomicU8::new(0);

pub fn config(device_id: u8) -> &'static RuntimeTrackballConfig {
    if device_id == LEFT_TRACKBALL_ID { &LEFT_TRACKBALL_CONFIG } else { &RIGHT_TRACKBALL_CONFIG }
}

pub fn active_layer() -> u8 { ACTIVE_LAYER.load(Ordering::Relaxed) }
pub fn set_active_layer(layer: u8) { ACTIVE_LAYER.store(layer, Ordering::Relaxed); }

/// Fixed PG1KB v8 layer roles, matching the known-good ZMK layout intent:
/// Base(0): left scroll 1/2 + inertia, right cursor 3/2
/// Num(1): left cursor 3/2, right precision cursor 1/2
/// Sym(2): left precise scroll 1/6 + inertia, right scroll 1/2 + inertia
pub fn effective_mode(device_id: u8) -> TrackballMode {
    match (active_layer(), device_id) {
        (0, RIGHT_TRACKBALL_ID) => TrackballMode::Cursor,
        (0, LEFT_TRACKBALL_ID) => TrackballMode::Scroll,
        (1, _) => TrackballMode::Cursor,
        (2, _) => TrackballMode::Scroll,
        (_, RIGHT_TRACKBALL_ID) => TrackballMode::Cursor,
        _ => TrackballMode::Scroll,
    }
}

pub fn effective_cursor_gain_q8(device_id: u8) -> u16 {
    match (active_layer(), device_id) {
        (0, RIGHT_TRACKBALL_ID) => 384,
        (1, RIGHT_TRACKBALL_ID) => 128,
        (1, LEFT_TRACKBALL_ID) => 384,
        _ => config(device_id).cursor_gain_q8(),
    }
}

pub fn effective_scroll_scale_den(device_id: u8) -> u16 {
    match (active_layer(), device_id) {
        (0, LEFT_TRACKBALL_ID) => 2,
        (2, RIGHT_TRACKBALL_ID) => 2,
        (2, LEFT_TRACKBALL_ID) => 6,
        _ => config(device_id).scroll_scale_den(),
    }
}

pub fn effective_inertia_enabled(device_id: u8) -> bool {
    match (active_layer(), device_id) {
        (0, LEFT_TRACKBALL_ID) | (2, RIGHT_TRACKBALL_ID) | (2, LEFT_TRACKBALL_ID) => true,
        _ => config(device_id).inertia_enabled(),
    }
}

pub fn set_pointing_cpi(device_id: u8, cpi: u16) {
    config(device_id).set_cpi(cpi);
    publish_event(PointingSetCpiEvent { device_id, cpi });
}

pub fn resync_left_cpi() {
    publish_event(PointingSetCpiEvent { device_id: LEFT_TRACKBALL_ID, cpi: config(LEFT_TRACKBALL_ID).cpi() });
}

pub fn set_cursor_gain_q8(device_id: u8, gain_q8: u16) { config(device_id).set_cursor_gain_q8(gain_q8.max(16)); }
pub fn set_scroll_scale_den(device_id: u8, den: u16) { config(device_id).set_scroll_scale_den(den.max(1)); }
pub fn set_inertia_enabled(device_id: u8, enabled: bool) { config(device_id).set_inertia_enabled(enabled); }
pub fn set_inertia_decay(device_id: u8, num: u8, den: u8) { config(device_id).set_inertia_decay(num, den.max(1)); }
pub fn set_sensor_rotation(device_id: u8, rotation: SensorRotation) { config(device_id).set_rotation(rotation); }
pub fn set_direction_noise_threshold(device_id: u8, value: u8) { config(device_id).set_direction_noise_threshold(value.max(1)); }
pub fn set_direction_reverse_threshold(device_id: u8, value: u8) { config(device_id).set_direction_reverse_threshold(value.max(1)); }

pub fn set_sensor_rotation_degrees(device_id: u8, degrees: u16) {
    let rotation = match degrees { 90 => SensorRotation::Deg90, 180 => SensorRotation::Deg180, 270 => SensorRotation::Deg270, _ => SensorRotation::Deg0 };
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
    // CPI, cursor gain, scroll scale, inertia, rotation, noise thresholds.
    out[10] = 0b0011_1111;
    out[11] = effective_mode(device_id) as u8;
    out[12] = cfg.direction_noise_threshold();
    out[13] = cfg.direction_reverse_threshold();
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
    let noise = if data[12] == 0 { 2 } else { data[12].min(32) };
    let reverse = if data[13] == 0 { 4 } else { data[13].min(64) };

    set_pointing_cpi(device_id, cpi);
    set_cursor_gain_q8(device_id, cursor_gain_q8);
    set_scroll_scale_den(device_id, scroll_scale_den);
    set_inertia_enabled(device_id, inertia_enabled);
    set_inertia_decay(device_id, decay_num, decay_den);
    set_sensor_rotation(device_id, rotation);
    set_direction_noise_threshold(device_id, noise);
    set_direction_reverse_threshold(device_id, reverse.max(noise));
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
    let right = [0xdc,0x03, 0x00,0x01, 0x06,0x00, 0x00, 0x0f,0x10, 0x00, 0x3f,0x00, 0x02,0x04, 0x00,0x00];
    let left  = [0xdc,0x03, 0x00,0x01, 0x06,0x00, 0x01, 0x0f,0x10, 0x01, 0x3f,0x01, 0x02,0x04, 0x00,0x00];
    apply_config(RIGHT_TRACKBALL_ID, &right);
    apply_config(LEFT_TRACKBALL_ID, &left);
}

pub fn request_save() -> u16 {
    let generation = SAVE_GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    SAVE_STATUS.store(SAVE_PENDING, Ordering::Release);
    SAVE_REQUESTED.store(true, Ordering::Release);
    generation
}

pub fn take_save_request() -> Option<u16> {
    if SAVE_REQUESTED.swap(false, Ordering::AcqRel) { Some(SAVE_GENERATION.load(Ordering::Acquire)) } else { None }
}

pub fn complete_save(generation: u16, ok: bool) {
    SAVE_COMPLETED_GENERATION.store(generation, Ordering::Release);
    SAVE_STATUS.store(if ok { SAVE_OK } else { SAVE_FAILED }, Ordering::Release);
}

pub fn save_status_wire() -> [u8; 5] {
    let requested = SAVE_GENERATION.load(Ordering::Acquire).to_le_bytes();
    let completed = SAVE_COMPLETED_GENERATION.load(Ordering::Acquire).to_le_bytes();
    [SAVE_STATUS.load(Ordering::Acquire), requested[0], requested[1], completed[0], completed[1]]
}

pub fn state_wire() -> [u8; 7] {
    [
        active_layer(),
        effective_mode(RIGHT_TRACKBALL_ID) as u8,
        effective_mode(LEFT_TRACKBALL_ID) as u8,
        (effective_cursor_gain_q8(RIGHT_TRACKBALL_ID) / 16).min(255) as u8,
        (effective_cursor_gain_q8(LEFT_TRACKBALL_ID) / 16).min(255) as u8,
        effective_scroll_scale_den(RIGHT_TRACKBALL_ID).min(255) as u8,
        effective_scroll_scale_den(LEFT_TRACKBALL_ID).min(255) as u8,
    ]
}

/// PG1KB private Rynk extension, 0x0901..0x0906.
pub fn handle_rynk_trackball(msg: &mut RynkMessage<'_>) -> Option<Result<(), RynkError>> {
    match msg.header().cmd.raw() {
        RYNK_GET_TRACKBALL_CONFIG => {
            let device_id = match msg.decode_request::<u8>() { Ok(value) if value <= LEFT_TRACKBALL_ID => value, Ok(_) => return Some(Err(RynkError::Malformed)), Err(error) => return Some(Err(error)) };
            Some(msg.encode_response(&encode_config(device_id)))
        }
        RYNK_SET_TRACKBALL_CONFIG => {
            let request = match msg.decode_request::<[u8; TRACKBALL_CONFIG_WIRE_LEN + 1]>() { Ok(value) => value, Err(error) => return Some(Err(error)) };
            let device_id = request[0];
            if device_id > LEFT_TRACKBALL_ID { return Some(Err(RynkError::Malformed)); }
            let mut data = [0u8; TRACKBALL_CONFIG_WIRE_LEN];
            data.copy_from_slice(&request[1..]);
            apply_config(device_id, &data);
            Some(msg.encode_response(&()))
        }
        RYNK_SAVE_TRACKBALL_CONFIG => {
            if let Err(error) = msg.decode_request::<()>() { return Some(Err(error)); }
            let generation = request_save();
            Some(msg.encode_response(&generation))
        }
        RYNK_LOAD_TRACKBALL_DEFAULTS => {
            if let Err(error) = msg.decode_request::<()>() { return Some(Err(error)); }
            load_defaults();
            Some(msg.encode_response(&()))
        }
        RYNK_GET_SAVE_STATUS => {
            if let Err(error) = msg.decode_request::<()>() { return Some(Err(error)); }
            Some(msg.encode_response(&save_status_wire()))
        }
        RYNK_GET_TRACKBALL_STATE => {
            if let Err(error) = msg.decode_request::<()>() { return Some(Err(error)); }
            Some(msg.encode_response(&state_wire()))
        }
        _ => None,
    }
}

use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SensorRotation {
    Deg0 = 0,
    Deg90 = 1,
    Deg180 = 2,
    Deg270 = 3,
}

impl SensorRotation {
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 0x03 {
            1 => Self::Deg90,
            2 => Self::Deg180,
            3 => Self::Deg270,
            _ => Self::Deg0,
        }
    }

    pub const fn raw(self) -> u8 {
        self as u8
    }

    pub const fn degrees(self) -> u16 {
        match self {
            Self::Deg0 => 0,
            Self::Deg90 => 90,
            Self::Deg180 => 180,
            Self::Deg270 => 270,
        }
    }

    pub const fn apply(self, x: i32, y: i32) -> (i32, i32) {
        match self {
            Self::Deg0 => (x, y),
            Self::Deg90 => (y, -x),
            Self::Deg180 => (-x, -y),
            Self::Deg270 => (-y, x),
        }
    }
}

pub struct RuntimeTrackballConfig {
    cpi: AtomicU16,
    cursor_gain_q8: AtomicU16,
    scroll_scale_den: AtomicU16,
    inertia_enabled: AtomicBool,
    inertia_decay_num: AtomicU8,
    inertia_decay_den: AtomicU8,
    rotation: AtomicU8,
}

impl RuntimeTrackballConfig {
    pub const fn new(
        cpi: u16,
        cursor_gain_q8: u16,
        scroll_scale_den: u16,
        inertia_enabled: bool,
        inertia_decay_num: u8,
        inertia_decay_den: u8,
        rotation: SensorRotation,
    ) -> Self {
        Self {
            cpi: AtomicU16::new(cpi),
            cursor_gain_q8: AtomicU16::new(cursor_gain_q8),
            scroll_scale_den: AtomicU16::new(scroll_scale_den),
            inertia_enabled: AtomicBool::new(inertia_enabled),
            inertia_decay_num: AtomicU8::new(inertia_decay_num),
            inertia_decay_den: AtomicU8::new(inertia_decay_den),
            rotation: AtomicU8::new(rotation as u8),
        }
    }

    pub fn cpi(&self) -> u16 { self.cpi.load(Ordering::Relaxed) }
    pub fn cursor_gain_q8(&self) -> u16 { self.cursor_gain_q8.load(Ordering::Relaxed) }
    pub fn scroll_scale_den(&self) -> u16 { self.scroll_scale_den.load(Ordering::Relaxed).max(1) }
    pub fn inertia_enabled(&self) -> bool { self.inertia_enabled.load(Ordering::Relaxed) }
    pub fn inertia_decay(&self) -> (u8, u8) {
        (
            self.inertia_decay_num.load(Ordering::Relaxed),
            self.inertia_decay_den.load(Ordering::Relaxed).max(1),
        )
    }
    pub fn rotation(&self) -> SensorRotation { SensorRotation::from_raw(self.rotation.load(Ordering::Relaxed)) }

    pub fn set_cpi(&self, value: u16) { self.cpi.store(value, Ordering::Relaxed); }
    pub fn set_cursor_gain_q8(&self, value: u16) { self.cursor_gain_q8.store(value, Ordering::Relaxed); }
    pub fn set_scroll_scale_den(&self, value: u16) { self.scroll_scale_den.store(value.max(1), Ordering::Relaxed); }
    pub fn set_inertia_enabled(&self, value: bool) { self.inertia_enabled.store(value, Ordering::Relaxed); }
    pub fn set_inertia_decay(&self, num: u8, den: u8) {
        self.inertia_decay_num.store(num, Ordering::Relaxed);
        self.inertia_decay_den.store(den.max(1), Ordering::Relaxed);
    }
    pub fn set_rotation(&self, value: SensorRotation) { self.rotation.store(value as u8, Ordering::Relaxed); }
}

pub static RIGHT_TRACKBALL_CONFIG: RuntimeTrackballConfig = RuntimeTrackballConfig::new(
    988,
    256,
    6,
    false,
    15,
    16,
    SensorRotation::Deg0,
);

pub static LEFT_TRACKBALL_CONFIG: RuntimeTrackballConfig = RuntimeTrackballConfig::new(
    988,
    256,
    6,
    true,
    15,
    16,
    SensorRotation::Deg90,
);

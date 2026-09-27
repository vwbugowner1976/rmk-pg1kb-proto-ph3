use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_time::Instant;
use rmk::event::BatteryStatusEvent;
use rmk::macros::processor;
use rmk::types::battery::{BatteryStatus, ChargeState};

pub const HISTORY_LEN: usize = 288;
pub const RECORD_LEN: usize = 6;
pub const DATA_OFFSET: usize = 3;
pub const CAPACITY: usize = (HISTORY_LEN - DATA_OFFSET) / RECORD_LEN;
const MAGIC: u8 = 0xB7;
pub const SAMPLE_INTERVAL_MIN: u16 = 30;
pub const CHUNK_RECORDS: usize = 4;
pub const CHUNK_COUNT: usize = CAPACITY / CHUNK_RECORDS;

static CLEAR_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct HistoryState {
    data: [u8; HISTORY_LEN],
    count: u8,
    next: u8,
}
impl HistoryState { const fn empty() -> Self { Self { data: [0; HISTORY_LEN], count: 0, next: 0 } } }

static HISTORY: critical_section::Mutex<RefCell<HistoryState>> =
    critical_section::Mutex::new(RefCell::new(HistoryState::empty()));

fn encode_record(minutes: u32, status: BatteryStatus) -> [u8; RECORD_LEN] {
    let (level, charging) = match status {
        BatteryStatus::Available { level: Some(level), charge_state } =>
            (level, matches!(charge_state, ChargeState::Charging)),
        _ => return [0; RECORD_LEN],
    };
    let mut out = [0u8; RECORD_LEN];
    out[0..4].copy_from_slice(&minutes.to_le_bytes());
    out[4] = level;
    out[5] = charging as u8;
    out
}

fn load_state(data: [u8; HISTORY_LEN]) {
    let mut count = 0u8;
    let mut next = 0u8;
    if data[0] == MAGIC {
        count = data[1].min(CAPACITY as u8);
        next = data[2].min((CAPACITY.saturating_sub(1)) as u8);
    } else {
        for i in 0..CAPACITY {
            let off = DATA_OFFSET + i * RECORD_LEN;
            if data[off..off + RECORD_LEN].iter().all(|v| *v == 0) { break; }
            count = count.saturating_add(1);
            next = ((i + 1) % CAPACITY) as u8;
        }
    }
    critical_section::with(|cs| {
        let mut state = HISTORY.borrow(cs).borrow_mut();
        state.data = data;
        state.count = count;
        state.next = next;
    });
}

pub fn get_info() -> [u8; 4] {
    critical_section::with(|cs| {
        let state = HISTORY.borrow(cs).borrow();
        [state.count, CAPACITY as u8, state.next, SAMPLE_INTERVAL_MIN as u8, (SAMPLE_INTERVAL_MIN >> 8) as u8]
    })
}

pub fn get_chunk(chunk: u8) -> [u8; 25] {
    let mut out = [0u8; 25];
    let chunk = chunk as usize;
    if chunk >= CHUNK_COUNT { return out; }
    critical_section::with(|cs| {
        let state = HISTORY.borrow(cs).borrow();
        let start = DATA_OFFSET + chunk * CHUNK_RECORDS * RECORD_LEN;
        let records = CHUNK_RECORDS.min(CAPACITY.saturating_sub(chunk * CHUNK_RECORDS));
        out[0] = records as u8;
        out[1..1 + records * RECORD_LEN]
            .copy_from_slice(&state.data[start..start + records * RECORD_LEN]);
    });
    out
}

pub fn request_clear() { CLEAR_REQUESTED.store(true, Ordering::Release); }

#[processor(subscribe = [BatteryStatusEvent], poll_interval = 1800000)]
pub struct BatteryHistoryProcessor {
    initialized: bool,
    current: BatteryStatus,
}

impl BatteryHistoryProcessor {
    pub const fn new() -> Self {
        Self { initialized: false, current: BatteryStatus::Unavailable }
    }

    async fn save(&self) {
        let data = critical_section::with(|cs| HISTORY.borrow(cs).borrow().data);
        let _ = rmk::host::pg1kb_write_battery_history(data).await;
    }

    fn append(&mut self) {
        let record = encode_record((Instant::now().as_secs() / 60) as u32, self.current);
        if record == [0; RECORD_LEN] { return; }
        critical_section::with(|cs| {
            let mut state = HISTORY.borrow(cs).borrow_mut();
            let off = DATA_OFFSET + state.next as usize * RECORD_LEN;
            state.data[off..off + RECORD_LEN].copy_from_slice(&record);
            state.next = ((state.next as usize + 1) % CAPACITY) as u8;
            state.data[0] = MAGIC;
            state.data[1] = state.count.saturating_add(1).min(CAPACITY as u8);
            state.data[2] = state.next;
            state.count = state.count.saturating_add(1).min(CAPACITY as u8);
        });
    }

    async fn poll(&mut self) {
        if !self.initialized {
            if let Some(data) = rmk::host::pg1kb_read_battery_history().await { load_state(data); }
            self.initialized = true;
        }
        if CLEAR_REQUESTED.swap(false, Ordering::AcqRel) {
            critical_section::with(|cs| *HISTORY.borrow(cs).borrow_mut() = HistoryState::empty());
            let _ = rmk::host::pg1kb_write_battery_history([0; HISTORY_LEN]).await;
            return;
        }
        self.append();
        self.save().await;
    }

    async fn on_battery_status_event(&mut self, event: BatteryStatusEvent) {
        self.current = event.0;
    }
}

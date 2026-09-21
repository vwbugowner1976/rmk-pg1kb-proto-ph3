use embassy_time::{Duration, Instant};
use rmk::channel::BLE_REPORT_CHANNEL;
use rmk::event::{Axis, PointingEvent};
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

use crate::runtime::{self, TrackballMode};

const INERTIA_DIV: i32 = 16;
const STOP_VELOCITY_Q8: i32 = 4;
const Q8_ONE: i32 = 256;
const DIAG_INTERVAL_MS: u64 = 1000;

#[processor(subscribe = [PointingEvent], poll_interval = 8)]
pub struct SplitPointingBleProcessor {
    device_id: u8,
    cursor_x: i32,
    cursor_y: i32,
    cursor_out_x_q8: i32,
    cursor_out_y_q8: i32,
    wheel_accum_q8: i32,
    velocity_q8: i32,
    input_seen: bool,
    direction: i8,
    last_mode: u8,
    hid_reports: u32,
    hid_busy: u32,
    last_rx: Option<Instant>,
    rx_count_window: u32,
    rx_dt_samples: u32,
    rx_dt_sum_us: u64,
    rx_dt_min_us: u64,
    rx_dt_max_us: u64,
    rx_batch_sum: u64,
    rx_batch_max: u32,
    pending_rx_started: Option<Instant>,
    last_hid: Option<Instant>,
    last_hid_attempt: Option<Instant>,
    hid_count_window: u32,
    hid_dt_samples: u32,
    hid_dt_sum_us: u64,
    hid_dt_min_us: u64,
    hid_dt_max_us: u64,
    rx_to_hid_samples: u32,
    rx_to_hid_sum_us: u64,
    rx_to_hid_min_us: u64,
    rx_to_hid_max_us: u64,
    last_diag: Instant,
}

impl SplitPointingBleProcessor {
    pub fn new(device_id: u8) -> Self {
        Self {
            device_id,
            cursor_x: 0,
            cursor_y: 0,
            cursor_out_x_q8: 0,
            cursor_out_y_q8: 0,
            wheel_accum_q8: 0,
            velocity_q8: 0,
            input_seen: false,
            direction: 0,
            last_mode: runtime::effective_mode(device_id) as u8,
            hid_reports: 0,
            hid_busy: 0,
            last_rx: None,
            rx_count_window: 0,
            rx_dt_samples: 0,
            rx_dt_sum_us: 0,
            rx_dt_min_us: 0,
            rx_dt_max_us: 0,
            rx_batch_sum: 0,
            rx_batch_max: 0,
            pending_rx_started: None,
            last_hid: None,
            last_hid_attempt: None,
            hid_count_window: 0,
            hid_dt_samples: 0,
            hid_dt_sum_us: 0,
            hid_dt_min_us: 0,
            hid_dt_max_us: 0,
            rx_to_hid_samples: 0,
            rx_to_hid_sum_us: 0,
            rx_to_hid_min_us: 0,
            rx_to_hid_max_us: 0,
            last_diag: Instant::now(),
        }
    }

    fn reset_motion_state(&mut self) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.cursor_out_x_q8 = 0;
        self.cursor_out_y_q8 = 0;
        self.wheel_accum_q8 = 0;
        self.velocity_q8 = 0;
        self.input_seen = false;
        self.direction = 0;
        self.pending_rx_started = None;
    }

    fn sync_mode(&mut self) -> TrackballMode {
        let mode = runtime::effective_mode(self.device_id);
        if mode as u8 != self.last_mode {
            self.reset_motion_state();
            self.last_mode = mode as u8;
        }
        mode
    }

    async fn on_pointing_event(&mut self, event: PointingEvent) {
        if event.device_id != self.device_id { return; }

        let rx_now = Instant::now();
        if let Some(last) = self.last_rx {
            let dt_us = last.elapsed().as_micros();
            if self.rx_dt_samples == 0 || dt_us < self.rx_dt_min_us {
                self.rx_dt_min_us = dt_us;
            }
            if dt_us > self.rx_dt_max_us {
                self.rx_dt_max_us = dt_us;
            }
            self.rx_dt_sum_us = self.rx_dt_sum_us.saturating_add(dt_us);
            self.rx_dt_samples = self.rx_dt_samples.saturating_add(1);
        }
        self.last_rx = Some(rx_now);
        self.rx_count_window = self.rx_count_window.saturating_add(1);

        let mut raw_x: i32 = 0;
        let mut raw_y: i32 = 0;
        for axis in event.axes {
            match axis.axis {
                Axis::X => raw_x = raw_x.saturating_add(axis.value as i32),
                Axis::Y => raw_y = raw_y.saturating_add(axis.value as i32),
                _ => {}
            }
        }

        let batch = raw_x.abs().saturating_add(raw_y.abs()) as u32;
        self.rx_batch_sum = self.rx_batch_sum.saturating_add(batch as u64);
        self.rx_batch_max = self.rx_batch_max.max(batch);

        let cfg = runtime::config(self.device_id);
        let (logical_x, logical_y) = runtime::effective_rotation(self.device_id).apply(raw_x, raw_y);
        match self.sync_mode() {
            TrackballMode::Cursor => {
                // The split BLE transport now delivers left motion close to the 8 ms source cadence.
                // Do not enqueue one HID report per received event: that can outrun the report
                // consumer and create a growing cursor tail. Accumulate until the 8 ms poll and
                // emit at most one HID report per poll. Right-side processing is untouched.
                self.cursor_x = self.cursor_x.saturating_add(logical_x);
                self.cursor_y = self.cursor_y.saturating_add(logical_y);
                if self.pending_rx_started.is_none() {
                    self.pending_rx_started = Some(rx_now);
                }

                // Hybrid left-side pacing:
                // if the HID path has been idle long enough, flush immediately to
                // avoid the average half-period delay of the 8 ms poll. When events
                // arrive too quickly, keep accumulating and let the poll flush them.
                // The 5 ms guard inside flush_cursor_report() still prevents retry storms.
                let can_flush_now = self
                    .last_hid_attempt
                    .map(|last| last.elapsed() >= Duration::from_millis(5))
                    .unwrap_or(true);
                if can_flush_now {
                    self.flush_cursor_report();
                }
            }
            TrackballMode::Scroll => {
                if logical_y == 0 { return; }
                let incoming_direction: i8 = if logical_y > 0 { 1 } else { -1 };
                let magnitude = logical_y.abs();
                let noise = cfg.direction_noise_threshold() as i32;
                let reverse = cfg.direction_reverse_threshold() as i32;

                if self.direction == 0 {
                    if magnitude < noise { return; }
                    self.direction = incoming_direction;
                } else if incoming_direction != self.direction {
                    if magnitude < reverse { return; }
                    self.direction = incoming_direction;
                    self.wheel_accum_q8 = 0;
                    self.velocity_q8 = 0;
                }

                let stable_y = magnitude.saturating_mul(self.direction as i32);
                let scale_den = runtime::effective_scroll_scale_den(self.device_id) as i32;
                let scroll_q8 = stable_y.saturating_mul(Q8_ONE) / scale_den;
                self.wheel_accum_q8 = self.wheel_accum_q8.saturating_add(scroll_q8);
                if runtime::effective_inertia_enabled(self.device_id) {
                    self.velocity_q8 = scroll_q8 / INERTIA_DIV;
                } else {
                    self.velocity_q8 = 0;
                }
                self.input_seen = true;
            }
        }
    }

    async fn poll(&mut self) {
        match self.sync_mode() {
            TrackballMode::Cursor => self.poll_cursor(),
            TrackballMode::Scroll => self.poll_scroll(),
        }

        if self.last_diag.elapsed() >= Duration::from_millis(DIAG_INTERVAL_MS) {
            self.last_diag = Instant::now();
            let rx_dt_avg_us = if self.rx_dt_samples == 0 { 0 } else { self.rx_dt_sum_us / self.rx_dt_samples as u64 };
            let rx_batch_avg = if self.rx_count_window == 0 { 0 } else { self.rx_batch_sum / self.rx_count_window as u64 };
            let hid_dt_avg_us = if self.hid_dt_samples == 0 { 0 } else { self.hid_dt_sum_us / self.hid_dt_samples as u64 };
            let rx_to_hid_avg_us = if self.rx_to_hid_samples == 0 { 0 } else { self.rx_to_hid_sum_us / self.rx_to_hid_samples as u64 };
            log::info!(
                "LEFT split diag rx_win={} rx_dt_us_min={} avg={} max={} rx_batch_avg={} max={} hid_win={} hid_total={} hid_dt_us_min={} avg={} max={} rx_to_hid_us_min={} avg={} max={} busy={}",
                self.rx_count_window,
                self.rx_dt_min_us,
                rx_dt_avg_us,
                self.rx_dt_max_us,
                rx_batch_avg,
                self.rx_batch_max,
                self.hid_count_window,
                self.hid_reports,
                self.hid_dt_min_us,
                hid_dt_avg_us,
                self.hid_dt_max_us,
                self.rx_to_hid_min_us,
                rx_to_hid_avg_us,
                self.rx_to_hid_max_us,
                self.hid_busy,
            );
            self.rx_count_window = 0;
            self.rx_dt_samples = 0;
            self.rx_dt_sum_us = 0;
            self.rx_dt_min_us = 0;
            self.rx_dt_max_us = 0;
            self.rx_batch_sum = 0;
            self.rx_batch_max = 0;
            self.hid_count_window = 0;
            self.hid_dt_samples = 0;
            self.hid_dt_sum_us = 0;
            self.hid_dt_min_us = 0;
            self.hid_dt_max_us = 0;
            self.rx_to_hid_samples = 0;
            self.rx_to_hid_sum_us = 0;
            self.rx_to_hid_min_us = 0;
            self.rx_to_hid_max_us = 0;
        }
    }

    fn poll_cursor(&mut self) {
        // Retry any cursor report that could not be queued immediately.
        self.flush_cursor_report();
    }

    fn flush_cursor_report(&mut self) {
        if self.cursor_x != 0 || self.cursor_y != 0 {
            let gain_q8 = runtime::effective_cursor_gain_q8(self.device_id) as i32;
            self.cursor_out_x_q8 = self.cursor_out_x_q8.saturating_add(self.cursor_x.saturating_mul(gain_q8));
            self.cursor_out_y_q8 = self.cursor_out_y_q8.saturating_add(self.cursor_y.saturating_mul(gain_q8));
            self.cursor_x = 0;
            self.cursor_y = 0;
        }

        // Embassy Ticker may catch up with several immediate ticks after the task was
        // delayed by BLE/event work. Rate-limit queue attempts as well as successful
        // reports so those catch-up ticks cannot hammer BLE_REPORT_CHANNEL.
        if let Some(last_attempt) = self.last_hid_attempt {
            if last_attempt.elapsed() < Duration::from_millis(5) {
                return;
            }
        }

        let whole_x = self.cursor_out_x_q8 / Q8_ONE;
        let whole_y = self.cursor_out_y_q8 / Q8_ONE;
        if whole_x == 0 && whole_y == 0 { return; }

        let x = whole_x.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let y = whole_y.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let report = Report::MouseReport(MouseReport { buttons: rmk::channel::mouse_button_state(), x, y, wheel: 0, pan: 0 });
        self.last_hid_attempt = Some(Instant::now());
        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            let hid_now = Instant::now();
            if let Some(last) = self.last_hid {
                let dt_us = last.elapsed().as_micros();
                if self.hid_dt_samples == 0 || dt_us < self.hid_dt_min_us {
                    self.hid_dt_min_us = dt_us;
                }
                if dt_us > self.hid_dt_max_us {
                    self.hid_dt_max_us = dt_us;
                }
                self.hid_dt_sum_us = self.hid_dt_sum_us.saturating_add(dt_us);
                self.hid_dt_samples = self.hid_dt_samples.saturating_add(1);
            }
            self.last_hid = Some(hid_now);
            if let Some(started) = self.pending_rx_started.take() {
                let delay_us = started.elapsed().as_micros();
                if self.rx_to_hid_samples == 0 || delay_us < self.rx_to_hid_min_us {
                    self.rx_to_hid_min_us = delay_us;
                }
                if delay_us > self.rx_to_hid_max_us {
                    self.rx_to_hid_max_us = delay_us;
                }
                self.rx_to_hid_sum_us = self.rx_to_hid_sum_us.saturating_add(delay_us);
                self.rx_to_hid_samples = self.rx_to_hid_samples.saturating_add(1);
            }
            self.cursor_out_x_q8 = self.cursor_out_x_q8.saturating_sub((x as i32).saturating_mul(Q8_ONE));
            self.cursor_out_y_q8 = self.cursor_out_y_q8.saturating_sub((y as i32).saturating_mul(Q8_ONE));
            self.hid_reports = self.hid_reports.saturating_add(1);
            self.hid_count_window = self.hid_count_window.saturating_add(1);
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }

    fn poll_scroll(&mut self) {
        let cfg = runtime::config(self.device_id);
        if self.input_seen {
            self.input_seen = false;
        } else if runtime::effective_inertia_enabled(self.device_id) && self.velocity_q8 != 0 {
            self.wheel_accum_q8 = self.wheel_accum_q8.saturating_add(self.velocity_q8);
            let (decay_num, decay_den) = cfg.inertia_decay();
            self.velocity_q8 = self.velocity_q8.saturating_mul(decay_num as i32) / decay_den as i32;
            if self.velocity_q8.abs() < STOP_VELOCITY_Q8 {
                self.velocity_q8 = 0;
                self.direction = 0;
            }
        } else {
            self.velocity_q8 = 0;
            self.direction = 0;
        }

        let wheel_steps = self.wheel_accum_q8 / Q8_ONE;
        if wheel_steps == 0 { return; }
        let wheel = wheel_steps.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let report = Report::MouseReport(MouseReport { buttons: rmk::channel::mouse_button_state(), x: 0, y: 0, wheel, pan: 0 });
        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.wheel_accum_q8 = self.wheel_accum_q8.saturating_sub((wheel as i32).saturating_mul(Q8_ONE));
            self.hid_reports = self.hid_reports.saturating_add(1);
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }
}

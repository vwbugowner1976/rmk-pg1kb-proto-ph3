use rmk::channel::BLE_REPORT_CHANNEL;
use rmk::event::{Axis, PointingEvent};
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

use crate::runtime::{self, TrackballMode};

const INERTIA_DIV: i32 = 16;
const STOP_VELOCITY_Q8: i32 = 4;
const Q8_ONE: i32 = 256;

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

        let mut raw_x: i32 = 0;
        let mut raw_y: i32 = 0;
        for axis in event.axes {
            match axis.axis {
                Axis::X => raw_x = raw_x.saturating_add(axis.value as i32),
                Axis::Y => raw_y = raw_y.saturating_add(axis.value as i32),
                _ => {}
            }
        }

        let cfg = runtime::config(self.device_id);
        let (logical_x, logical_y) = runtime::effective_rotation(self.device_id).apply(raw_x, raw_y);
        match self.sync_mode() {
            TrackballMode::Cursor => {
                // Left-side split events already arrive at roughly one BLE connection interval.
                // Emit the HID report immediately instead of waiting for the separate 8 ms poll;
                // this removes phase jitter unique to the left path. Right-side processing is untouched.
                let gain_q8 = runtime::effective_cursor_gain_q8(self.device_id) as i32;
                self.cursor_out_x_q8 = self.cursor_out_x_q8.saturating_add(logical_x.saturating_mul(gain_q8));
                self.cursor_out_y_q8 = self.cursor_out_y_q8.saturating_add(logical_y.saturating_mul(gain_q8));
                self.flush_cursor_report();
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

        let whole_x = self.cursor_out_x_q8 / Q8_ONE;
        let whole_y = self.cursor_out_y_q8 / Q8_ONE;
        if whole_x == 0 && whole_y == 0 { return; }

        let x = whole_x.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let y = whole_y.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let report = Report::MouseReport(MouseReport { buttons: 0, x, y, wheel: 0, pan: 0 });
        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.cursor_out_x_q8 = self.cursor_out_x_q8.saturating_sub((x as i32).saturating_mul(Q8_ONE));
            self.cursor_out_y_q8 = self.cursor_out_y_q8.saturating_sub((y as i32).saturating_mul(Q8_ONE));
            self.hid_reports = self.hid_reports.saturating_add(1);
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
        let report = Report::MouseReport(MouseReport { buttons: 0, x: 0, y: 0, wheel, pan: 0 });
        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.wheel_accum_q8 = self.wheel_accum_q8.saturating_sub((wheel as i32).saturating_mul(Q8_ONE));
            self.hid_reports = self.hid_reports.saturating_add(1);
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }
}

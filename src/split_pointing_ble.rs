use rmk::channel::BLE_REPORT_CHANNEL;
use rmk::event::{Axis, PointingEvent};
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

use crate::runtime;

const INERTIA_DIV: i32 = 16;
const STOP_VELOCITY_Q8: i32 = 4;
const Q8_ONE: i32 = 256;
const DIRECTION_NOISE_THRESHOLD: i32 = 2;
const DIRECTION_REVERSE_THRESHOLD: i32 = 4;

#[processor(subscribe = [PointingEvent], poll_interval = 8)]
pub struct SplitPointingBleProcessor {
    device_id: u8,
    wheel_accum_q8: i32,
    velocity_q8: i32,
    input_seen: bool,
    direction: i8,
    hid_reports: u32,
    hid_busy: u32,
}

impl SplitPointingBleProcessor {
    pub fn new(device_id: u8) -> Self {
        Self {
            device_id,
            wheel_accum_q8: 0,
            velocity_q8: 0,
            input_seen: false,
            direction: 0,
            hid_reports: 0,
            hid_busy: 0,
        }
    }

    async fn on_pointing_event(&mut self, event: PointingEvent) {
        if event.device_id != self.device_id {
            return;
        }

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
        let (_logical_x, logical_y) = cfg.rotation().apply(raw_x, raw_y);
        if logical_y == 0 {
            return;
        }

        let incoming_direction: i8 = if logical_y > 0 { 1 } else { -1 };
        let magnitude = logical_y.abs();

        if self.direction == 0 {
            if magnitude < DIRECTION_NOISE_THRESHOLD {
                return;
            }
            self.direction = incoming_direction;
        } else if incoming_direction != self.direction {
            if magnitude < DIRECTION_REVERSE_THRESHOLD {
                return;
            }
            self.direction = incoming_direction;
            self.wheel_accum_q8 = 0;
            self.velocity_q8 = 0;
        }

        let stable_y = magnitude.saturating_mul(self.direction as i32);
        let scale_den = cfg.scroll_scale_den() as i32;
        let scroll_q8 = stable_y.saturating_mul(Q8_ONE) / scale_den;

        self.wheel_accum_q8 = self.wheel_accum_q8.saturating_add(scroll_q8);

        if cfg.inertia_enabled() {
            self.velocity_q8 = scroll_q8 / INERTIA_DIV;
        } else {
            self.velocity_q8 = 0;
        }
        self.input_seen = true;
    }

    async fn poll(&mut self) {
        let cfg = runtime::config(self.device_id);

        if self.input_seen {
            self.input_seen = false;
        } else if cfg.inertia_enabled() && self.velocity_q8 != 0 {
            self.wheel_accum_q8 = self.wheel_accum_q8.saturating_add(self.velocity_q8);
            let (decay_num, decay_den) = cfg.inertia_decay();
            self.velocity_q8 = self
                .velocity_q8
                .saturating_mul(decay_num as i32)
                / decay_den as i32;
            if self.velocity_q8.abs() < STOP_VELOCITY_Q8 {
                self.velocity_q8 = 0;
                self.direction = 0;
            }
        } else {
            self.velocity_q8 = 0;
            self.direction = 0;
        }

        let wheel_steps = self.wheel_accum_q8 / Q8_ONE;
        if wheel_steps == 0 {
            return;
        }

        let wheel = wheel_steps.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let report = Report::MouseReport(MouseReport {
            buttons: 0,
            x: 0,
            y: 0,
            wheel,
            pan: 0,
        });

        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.wheel_accum_q8 = self
                .wheel_accum_q8
                .saturating_sub((wheel as i32).saturating_mul(Q8_ONE));
            self.hid_reports = self.hid_reports.saturating_add(1);
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }
}

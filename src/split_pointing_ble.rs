use rmk::channel::BLE_REPORT_CHANNEL;
use rmk::event::{Axis, PointingEvent};
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

// PG1KB left-trackball scroll tuning, based on the previous ZMK behavior.
// The left sensor's horizontal axis is used for vertical scrolling with X inverted.
const INERTIA_TICK_MS: u64 = 8;
const INPUT_SCALE_NUM: i32 = 1;
const INPUT_SCALE_DEN: i32 = 2;
const VELOCITY_GAIN_Q8: i32 = 24;
const DECAY_NUM: i32 = 7;
const DECAY_DEN: i32 = 8;
const STOP_VELOCITY_Q8: i32 = 10;
const Q8_ONE: i32 = 256;

#[processor(subscribe = [PointingEvent], poll_interval = 8)]
pub struct SplitPointingBleProcessor {
    device_id: u8,
    // Q8 fixed-point wheel position and velocity. No floating point is used.
    wheel_accum_q8: i32,
    velocity_q8: i32,
    hid_reports: u32,
    hid_busy: u32,
}

impl SplitPointingBleProcessor {
    pub fn new(device_id: u8) -> Self {
        Self {
            device_id,
            wheel_accum_q8: 0,
            velocity_q8: 0,
            hid_reports: 0,
            hid_busy: 0,
        }
    }

    async fn on_pointing_event(&mut self, event: PointingEvent) {
        if event.device_id != self.device_id {
            return;
        }

        let mut x: i32 = 0;
        for axis in event.axes {
            if matches!(axis.axis, Axis::X) {
                x = x.saturating_add(axis.value as i32);
            }
        }

        if x == 0 {
            return;
        }

        // Previous ZMK left-scroll transform was X_INVERT.
        // Base-layer scroll speed was 1/2, retained here as the initial v6 value.
        let scroll_input = (-x)
            .saturating_mul(INPUT_SCALE_NUM)
            / INPUT_SCALE_DEN;

        // Direct component keeps the ball responsive while velocity provides the tail.
        self.wheel_accum_q8 = self
            .wheel_accum_q8
            .saturating_add(scroll_input.saturating_mul(Q8_ONE));
        self.velocity_q8 = self
            .velocity_q8
            .saturating_add(scroll_input.saturating_mul(VELOCITY_GAIN_Q8));
    }

    async fn poll(&mut self) {
        let _ = INERTIA_TICK_MS; // documents the macro's 8 ms cadence.

        // Continue integrating velocity after the physical motion stops.
        self.wheel_accum_q8 = self.wheel_accum_q8.saturating_add(self.velocity_q8);

        // Emit complete HID wheel steps while retaining the fractional remainder.
        let wheel_steps = self.wheel_accum_q8 / Q8_ONE;
        if wheel_steps != 0 {
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

        // Exponential decay: 7/8 every 8 ms.
        self.velocity_q8 = self.velocity_q8.saturating_mul(DECAY_NUM) / DECAY_DEN;
        if self.velocity_q8.abs() < STOP_VELOCITY_Q8 {
            self.velocity_q8 = 0;
        }
    }
}

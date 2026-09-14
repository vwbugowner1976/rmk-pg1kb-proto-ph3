use rmk::channel::BLE_REPORT_CHANNEL;
use rmk::event::{Axis, PointingEvent};
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

// PG1KB left-trackball scroll tuning.
// Keep all scaling in Q8 so slow/small ball motion is not lost to integer truncation.
const INPUT_SCALE_DEN: i32 = 6;
const DECAY_NUM: i32 = 15;
const DECAY_DEN: i32 = 16;
const INERTIA_DIV: i32 = 16;
const STOP_VELOCITY_Q8: i32 = 4;
const Q8_ONE: i32 = 256;

// PAW3222 can report tiny opposite-sign deltas while the ball is still travelling
// in one physical direction. Ignore those so wheel reports never chatter up/down.
const DIRECTION_NOISE_THRESHOLD: i32 = 2;
const DIRECTION_REVERSE_THRESHOLD: i32 = 4;

#[processor(subscribe = [PointingEvent], poll_interval = 8)]
pub struct SplitPointingBleProcessor {
    device_id: u8,
    // Q8 fixed-point wheel position and velocity. No floating point is used.
    wheel_accum_q8: i32,
    velocity_q8: i32,
    input_seen: bool,
    // -1 / 0 / +1. Keeps the physical scroll direction stable across sensor jitter.
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

        let mut x: i32 = 0;
        for axis in event.axes {
            if matches!(axis.axis, Axis::X) {
                x = x.saturating_add(axis.value as i32);
            }
        }

        if x == 0 {
            return;
        }

        let incoming_direction: i8 = if x > 0 { 1 } else { -1 };
        let magnitude = x.abs();

        if self.direction == 0 {
            if magnitude < DIRECTION_NOISE_THRESHOLD {
                return;
            }
            self.direction = incoming_direction;
        } else if incoming_direction != self.direction {
            // A one- or two-count opposite delta is normally sensor/mechanical jitter.
            // Require a clearly intentional reverse movement before changing direction.
            if magnitude < DIRECTION_REVERSE_THRESHOLD {
                return;
            }

            self.direction = incoming_direction;

            // Never let the previous direction's fractional wheel remainder or inertia
            // leak into the newly requested direction.
            self.wheel_accum_q8 = 0;
            self.velocity_q8 = 0;
        }

        // Direction is intentionally NOT inverted: the first v6 hardware test
        // proved the previous X_INVERT direction was backwards on this build.
        // Force the accepted direction onto the magnitude, then scale to 1/6 in Q8.
        let stable_x = magnitude.saturating_mul(self.direction as i32);
        let scroll_q8 = stable_x.saturating_mul(Q8_ONE) / INPUT_SCALE_DEN;

        // Physical motion is emitted directly at the reduced 1/6 rate.
        self.wheel_accum_q8 = self.wheel_accum_q8.saturating_add(scroll_q8);

        // Seed the inertial tail from the latest physical velocity rather than
        // accumulating it forever during a long scroll.
        self.velocity_q8 = scroll_q8 / INERTIA_DIV;
        self.input_seen = true;
    }

    async fn poll(&mut self) {
        if self.input_seen {
            // Do not add inertia while the ball is still supplying fresh motion.
            // Start the tail on the first tick after motion stops.
            self.input_seen = false;
        } else if self.velocity_q8 != 0 {
            self.wheel_accum_q8 = self.wheel_accum_q8.saturating_add(self.velocity_q8);
            self.velocity_q8 = self.velocity_q8.saturating_mul(DECAY_NUM) / DECAY_DEN;
            if self.velocity_q8.abs() < STOP_VELOCITY_Q8 {
                self.velocity_q8 = 0;
                self.direction = 0;
            }
        } else {
            self.direction = 0;
        }

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
    }
}

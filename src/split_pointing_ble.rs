use rmk::channel::BLE_REPORT_CHANNEL;
use rmk::event::{Axis, PointingEvent};
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

#[processor(subscribe = [PointingEvent])]
pub struct SplitPointingBleProcessor {
    device_id: u8,
    hid_reports: u32,
    hid_busy: u32,
}

impl SplitPointingBleProcessor {
    pub fn new(device_id: u8) -> Self {
        Self {
            device_id,
            hid_reports: 0,
            hid_busy: 0,
        }
    }

    async fn on_pointing_event(&mut self, event: PointingEvent) {
        if event.device_id != self.device_id {
            return;
        }

        let mut x: i16 = 0;
        let mut y: i16 = 0;
        let mut wheel: i16 = 0;
        let mut pan: i16 = 0;

        for axis in event.axes {
            match axis.axis {
                Axis::X => x = x.saturating_add(axis.value),
                Axis::Y => y = y.saturating_add(axis.value),
                Axis::V => wheel = wheel.saturating_add(axis.value),
                Axis::H => pan = pan.saturating_add(axis.value),
                _ => {}
            }
        }

        // First left-hand bring-up is raw cursor movement. Orientation transforms
        // will be applied after the real left sensor direction is verified.
        let report = Report::MouseReport(MouseReport {
            buttons: 0,
            x: x.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            y: y.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            wheel: wheel.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            pan: pan.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
        });

        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.hid_reports = self.hid_reports.saturating_add(1);
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }
}

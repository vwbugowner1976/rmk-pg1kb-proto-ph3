use embassy_time::{Duration, Instant};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::spi::SpiBus;
use log::{error, info, warn};
use rmk::event::{Axis, AxisEvent, AxisValType, PointingEvent, PointingSetCpiEvent, publish_event};
use rmk::macros::processor;

use crate::paw3222::{MotionDelta, Paw3222, Paw3222Error};

// Forward left-side motion to the central twice per 8 ms HID frame.
// The central still emits mouse reports at 8 ms (~125 Hz); this only reduces
// split-link batching/jitter so cursor motion does not arrive in large 8 ms chunks.
const REPORT_INTERVAL_MS: u64 = 4;
const DIAG_INTERVAL_MS: u64 = 1000;

#[processor(subscribe = [PointingSetCpiEvent], poll_interval = 1)]
pub struct Paw3222SplitProcessor<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> {
    id: u8,
    sensor: Paw3222<SPI, CS, MotionPin>,
    init_attempted: bool,
    ready: bool,
    last_init_error: Option<Paw3222Error>,
    accumulated_x: i32,
    accumulated_y: i32,
    last_delta: MotionDelta,
    sensor_reads: u32,
    motion_events: u32,
    published: u32,
    read_errors: u32,
    last_report: Instant,
    last_diag: Instant,
}

impl<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> Paw3222SplitProcessor<SPI, CS, MotionPin> {
    pub fn new(id: u8, spi: SPI, cs: CS, motion: MotionPin, cpi: u16, force_awake: bool) -> Self {
        Self {
            id,
            sensor: Paw3222::new(spi, cs, motion, cpi, force_awake),
            init_attempted: false,
            ready: false,
            last_init_error: None,
            accumulated_x: 0,
            accumulated_y: 0,
            last_delta: MotionDelta::default(),
            sensor_reads: 0,
            motion_events: 0,
            published: 0,
            read_errors: 0,
            last_report: Instant::now(),
            last_diag: Instant::now(),
        }
    }

    async fn poll(&mut self) {
        if !self.init_attempted {
            self.init_attempted = true;
            match self.sensor.configure().await {
                Ok(()) => {
                    self.ready = true;
                    self.last_init_error = None;
                    self.last_report = Instant::now();
                    info!("PAW3222 split processor ready device_id={}", self.id);
                }
                Err(err) => {
                    self.last_init_error = Some(err);
                    error!("PAW3222 split init failed error={:?}", err);
                }
            }
            return;
        }

        if self.ready && self.sensor.motion_pin_active() {
            self.sensor_reads = self.sensor_reads.saturating_add(1);
            match self.sensor.read_motion().await {
                Ok(Some(delta)) => {
                    self.motion_events = self.motion_events.saturating_add(1);
                    self.last_delta = delta;
                    self.accumulated_x = self.accumulated_x.saturating_add(delta.x as i32);
                    self.accumulated_y = self.accumulated_y.saturating_add(delta.y as i32);
                }
                Ok(None) => {}
                Err(_) => self.read_errors = self.read_errors.saturating_add(1),
            }
        }

        if self.ready && self.last_report.elapsed() >= Duration::from_millis(REPORT_INTERVAL_MS) {
            self.publish_motion();
        }

        if self.last_diag.elapsed() >= Duration::from_millis(DIAG_INTERVAL_MS) {
            self.last_diag = Instant::now();
            info!(
                "PAW3222 split diag ready={} pid=0x{:02x} reads={} events={} last_dx={} last_dy={} pub={} read_err={}",
                self.ready,
                self.sensor.last_product_id(),
                self.sensor_reads,
                self.motion_events,
                self.last_delta.x,
                self.last_delta.y,
                self.published,
                self.read_errors,
            );
        }
    }

    fn publish_motion(&mut self) {
        self.last_report = Instant::now();
        if self.accumulated_x == 0 && self.accumulated_y == 0 {
            return;
        }

        let x = self.accumulated_x.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let y = self.accumulated_y.clamp(i16::MIN as i32, i16::MAX as i32) as i16;

        publish_event(PointingEvent {
            device_id: self.id,
            axes: [
                AxisEvent { typ: AxisValType::Rel, axis: Axis::X, value: x },
                AxisEvent { typ: AxisValType::Rel, axis: Axis::Y, value: y },
                AxisEvent { typ: AxisValType::Rel, axis: Axis::Z, value: 0 },
            ],
        });

        self.accumulated_x -= x as i32;
        self.accumulated_y -= y as i32;
        self.published = self.published.saturating_add(1);
    }

    async fn on_pointing_set_cpi_event(&mut self, event: PointingSetCpiEvent) {
        if event.device_id != self.id || !self.ready {
            return;
        }
        info!("PAW3222 split set CPI {}", event.cpi);
        if self.sensor.set_resolution(event.cpi).await.is_err() {
            warn!("PAW3222 split set CPI failed");
        }
    }
}

pub type NrfPaw3222SplitProcessor = Paw3222SplitProcessor<
    rmk::driver::bitbang_spi::BitBangSpiBus<
        embassy_nrf::gpio::Output<'static>,
        embassy_nrf::gpio::Flex<'static>,
    >,
    embassy_nrf::gpio::Output<'static>,
    embassy_nrf::gpio::Input<'static>,
>;

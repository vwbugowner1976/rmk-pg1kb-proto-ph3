use embassy_time::{Duration, Instant};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::spi::SpiBus;
use log::{error, info, warn};
use rmk::channel::BLE_REPORT_CHANNEL;
use rmk::event::PointingSetCpiEvent;
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

use crate::paw3222::{MotionDelta, Paw3222, Paw3222Error};

const REPORT_INTERVAL_MS: u64 = 8; // 125 Hz, same cadence as the verified v1/v2 USB path
const DIAG_INTERVAL_MS: u64 = 1000;

/// BLE diagnostic wrapper around the already hardware-verified custom PAW3222
/// transport/decoder in paw3222.rs.
///
/// The sensor implementation itself is intentionally unchanged from v2:
/// same BitBang SPI, same CS-held X/Y/HI read sequence, same 12-bit handling.
/// Only the HID destination differs: reports are written to RMK's public
/// BLE_REPORT_CHANNEL so we can compare BLE transport behavior without using
/// the t-ogura PAW3222 RMK fork.
#[processor(subscribe = [PointingSetCpiEvent], poll_interval = 1)]
pub struct Paw3222BleProcessor<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> {
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
    hid_reports: u32,
    hid_busy: u32,
    read_errors: u32,
    last_report: Instant,
    last_diag: Instant,
}

impl<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> Paw3222BleProcessor<SPI, CS, MotionPin> {
    pub fn new(
        id: u8,
        spi: SPI,
        cs: CS,
        motion: MotionPin,
        resolution_cpi: u16,
        force_awake: bool,
    ) -> Self {
        Self {
            id,
            sensor: Paw3222::new(spi, cs, motion, resolution_cpi, force_awake),
            init_attempted: false,
            ready: false,
            last_init_error: None,
            accumulated_x: 0,
            accumulated_y: 0,
            last_delta: MotionDelta::default(),
            sensor_reads: 0,
            motion_events: 0,
            hid_reports: 0,
            hid_busy: 0,
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
                    info!("PAW3222 BLE processor ready device_id={}", self.id);
                }
                Err(err) => {
                    self.last_init_error = Some(err);
                    error!("PAW3222 BLE init failed error={:?}", err);
                }
            }
            self.emit_diag_if_due();
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
                Err(_) => {
                    self.read_errors = self.read_errors.saturating_add(1);
                }
            }
        }

        if self.ready && self.last_report.elapsed() >= Duration::from_millis(REPORT_INTERVAL_MS) {
            self.send_ble_report();
        }

        self.emit_diag_if_due();
    }

    fn send_ble_report(&mut self) {
        if self.accumulated_x == 0 && self.accumulated_y == 0 {
            self.last_report = Instant::now();
            return;
        }

        let x = self.accumulated_x.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let y = self.accumulated_y.clamp(i8::MIN as i32, i8::MAX as i32) as i8;

        let report = Report::MouseReport(MouseReport {
            buttons: 0,
            x,
            y,
            wheel: 0,
            pan: 0,
        });

        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.accumulated_x -= x as i32;
            self.accumulated_y -= y as i32;
            self.hid_reports = self.hid_reports.saturating_add(1);
            self.last_report = Instant::now();
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }

    fn emit_diag_if_due(&mut self) {
        if self.last_diag.elapsed() < Duration::from_millis(DIAG_INTERVAL_MS) {
            return;
        }
        self.last_diag = Instant::now();

        let motion_pin = self.sensor.motion_pin_active();
        info!(
            "PAW3222 BLE diag ready={} init_error={:?} pid=0x{:02x} 12bit={} mouse_opt=0x{:02x} motion_pin={} motion_reg=0x{:02x} reads={} events={} last_dx={} last_dy={} accum_x={} accum_y={} hid={} hid_busy={} read_err={}",
            self.ready,
            self.last_init_error,
            self.sensor.last_product_id(),
            self.sensor.twelve_bit(),
            self.sensor.mouse_option(),
            motion_pin,
            self.sensor.last_motion_reg(),
            self.sensor_reads,
            self.motion_events,
            self.last_delta.x,
            self.last_delta.y,
            self.accumulated_x,
            self.accumulated_y,
            self.hid_reports,
            self.hid_busy,
            self.read_errors,
        );
    }

    async fn on_pointing_set_cpi_event(&mut self, event: PointingSetCpiEvent) {
        if event.device_id != self.id || !self.ready {
            return;
        }

        info!("PAW3222 BLE set CPI {}", event.cpi);
        if self.sensor.set_resolution(event.cpi).await.is_err() {
            warn!("PAW3222 BLE set CPI failed");
        }
    }
}

pub type NrfPaw3222BleProcessor = Paw3222BleProcessor<
    rmk::driver::bitbang_spi::BitBangSpiBus<
        embassy_nrf::gpio::Output<'static>,
        embassy_nrf::gpio::Flex<'static>,
    >,
    embassy_nrf::gpio::Output<'static>,
    embassy_nrf::gpio::Input<'static>,
>;

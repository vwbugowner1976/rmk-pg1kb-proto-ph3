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
use crate::runtime::{self, TrackballMode};

const REPORT_INTERVAL_MS: u64 = 8;
const DIAG_INTERVAL_MS: u64 = 1000;
const Q8_ONE: i32 = 256;
const INERTIA_DIV: i32 = 16;
const STOP_VELOCITY_Q8: i32 = 4;

#[processor(subscribe = [PointingSetCpiEvent], poll_interval = 1)]
pub struct Paw3222BleProcessor<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> {
    id: u8,
    sensor: Paw3222<SPI, CS, MotionPin>,
    init_attempted: bool,
    ready: bool,
    last_init_error: Option<Paw3222Error>,
    accumulated_x: i32,
    accumulated_y: i32,
    cursor_out_x_q8: i32,
    cursor_out_y_q8: i32,
    wheel_accum_q8: i32,
    pan_accum_q8: i32,
    wheel_velocity_q8: i32,
    pan_velocity_q8: i32,
    wheel_direction: i8,
    pan_direction: i8,
    last_mode: u8,
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
    pub fn new(id: u8, spi: SPI, cs: CS, motion: MotionPin, resolution_cpi: u16, force_awake: bool) -> Self {
        Self {
            id,
            sensor: Paw3222::new(spi, cs, motion, resolution_cpi, force_awake),
            init_attempted: false,
            ready: false,
            last_init_error: None,
            accumulated_x: 0,
            accumulated_y: 0,
            cursor_out_x_q8: 0,
            cursor_out_y_q8: 0,
            wheel_accum_q8: 0,
            pan_accum_q8: 0,
            wheel_velocity_q8: 0,
            pan_velocity_q8: 0,
            wheel_direction: 0,
            pan_direction: 0,
            last_mode: runtime::effective_mode(id) as u8,
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

    fn reset_motion_state(&mut self) {
        self.accumulated_x = 0;
        self.accumulated_y = 0;
        self.cursor_out_x_q8 = 0;
        self.cursor_out_y_q8 = 0;
        self.wheel_accum_q8 = 0;
        self.pan_accum_q8 = 0;
        self.wheel_velocity_q8 = 0;
        self.pan_velocity_q8 = 0;
        self.wheel_direction = 0;
        self.pan_direction = 0;
    }

    fn sync_mode(&mut self) -> TrackballMode {
        let mode = runtime::effective_mode(self.id);
        if mode as u8 != self.last_mode {
            self.reset_motion_state();
            self.last_mode = mode as u8;
        }
        mode
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

        self.sync_mode();

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
            match self.sync_mode() {
                TrackballMode::Cursor => self.send_cursor_report(),
                TrackballMode::Scroll => self.send_scroll_report(),
            }
        }

        self.emit_diag_if_due();
    }

    fn send_cursor_report(&mut self) {
        let cfg = runtime::config(self.id);
        if self.accumulated_x != 0 || self.accumulated_y != 0 {
            let (rot_x, rot_y) = cfg.rotation().apply(self.accumulated_x, self.accumulated_y);
            let gain_q8 = runtime::effective_cursor_gain_q8(self.id) as i32;
            self.cursor_out_x_q8 = self.cursor_out_x_q8.saturating_add(rot_x.saturating_mul(gain_q8));
            self.cursor_out_y_q8 = self.cursor_out_y_q8.saturating_add(rot_y.saturating_mul(gain_q8));
            self.accumulated_x = 0;
            self.accumulated_y = 0;
        }

        let whole_x = self.cursor_out_x_q8 / Q8_ONE;
        let whole_y = self.cursor_out_y_q8 / Q8_ONE;
        if whole_x == 0 && whole_y == 0 {
            self.last_report = Instant::now();
            return;
        }

        let x = whole_x.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let y = whole_y.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let report = Report::MouseReport(MouseReport { buttons: rmk::channel::mouse_button_state(), x, y, wheel: 0, pan: 0 });

        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.cursor_out_x_q8 = self.cursor_out_x_q8.saturating_sub((x as i32).saturating_mul(Q8_ONE));
            self.cursor_out_y_q8 = self.cursor_out_y_q8.saturating_sub((y as i32).saturating_mul(Q8_ONE));
            self.hid_reports = self.hid_reports.saturating_add(1);
            self.last_report = Instant::now();
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }

    fn update_scroll_axis(
        input: i32,
        scale_den: i32,
        inertia_enabled: bool,
        noise: i32,
        reverse: i32,
        accum_q8: &mut i32,
        velocity_q8: &mut i32,
        direction: &mut i8,
    ) -> bool {
        if input == 0 {
            return false;
        }

        let incoming_direction = if input > 0 { 1 } else { -1 };
        let magnitude = input.abs();
        if *direction == 0 {
            if magnitude < noise {
                return false;
            }
            *direction = incoming_direction;
        } else if incoming_direction != *direction {
            if magnitude < reverse {
                return false;
            }
            *direction = incoming_direction;
            *accum_q8 = 0;
            *velocity_q8 = 0;
        }

        let stable = magnitude.saturating_mul(*direction as i32);
        let scroll_q8 = stable.saturating_mul(Q8_ONE) / scale_den.max(1);
        *accum_q8 = (*accum_q8).saturating_add(scroll_q8);
        *velocity_q8 = if inertia_enabled { scroll_q8 / INERTIA_DIV } else { 0 };
        true
    }

    fn advance_scroll_inertia(
        accum_q8: &mut i32,
        velocity_q8: &mut i32,
        direction: &mut i8,
        decay_num: u8,
        decay_den: u8,
    ) {
        if *velocity_q8 == 0 {
            *direction = 0;
            return;
        }
        *accum_q8 = (*accum_q8).saturating_add(*velocity_q8);
        *velocity_q8 = (*velocity_q8).saturating_mul(decay_num as i32) / decay_den.max(1) as i32;
        if (*velocity_q8).abs() < STOP_VELOCITY_Q8 {
            *velocity_q8 = 0;
            *direction = 0;
        }
    }

    fn send_scroll_report(&mut self) {
        let cfg = runtime::config(self.id);
        let (logical_x, logical_y) = runtime::effective_rotation(self.id).apply(self.accumulated_x, self.accumulated_y);
        self.accumulated_x = 0;
        self.accumulated_y = 0;

        let inertia_enabled = runtime::effective_inertia_enabled(self.id);
        let noise = cfg.direction_noise_threshold() as i32;
        let reverse = cfg.direction_reverse_threshold() as i32;
        let wheel_input = Self::update_scroll_axis(
            logical_y,
            runtime::effective_scroll_scale_den(self.id) as i32,
            inertia_enabled,
            noise,
            reverse,
            &mut self.wheel_accum_q8,
            &mut self.wheel_velocity_q8,
            &mut self.wheel_direction,
        );
        let pan_input = Self::update_scroll_axis(
            logical_x,
            runtime::effective_horizontal_scroll_scale_den(self.id) as i32,
            inertia_enabled,
            noise,
            reverse,
            &mut self.pan_accum_q8,
            &mut self.pan_velocity_q8,
            &mut self.pan_direction,
        );

        if inertia_enabled {
            let (decay_num, decay_den) = cfg.inertia_decay();
            if !wheel_input {
                Self::advance_scroll_inertia(
                    &mut self.wheel_accum_q8,
                    &mut self.wheel_velocity_q8,
                    &mut self.wheel_direction,
                    decay_num,
                    decay_den,
                );
            }
            if !pan_input {
                Self::advance_scroll_inertia(
                    &mut self.pan_accum_q8,
                    &mut self.pan_velocity_q8,
                    &mut self.pan_direction,
                    decay_num,
                    decay_den,
                );
            }
        } else {
            if !wheel_input {
                self.wheel_velocity_q8 = 0;
                self.wheel_direction = 0;
            }
            if !pan_input {
                self.pan_velocity_q8 = 0;
                self.pan_direction = 0;
            }
        }

        let wheel_steps = self.wheel_accum_q8 / Q8_ONE;
        let pan_steps = self.pan_accum_q8 / Q8_ONE;
        if wheel_steps == 0 && pan_steps == 0 {
            self.last_report = Instant::now();
            return;
        }

        let wheel = wheel_steps.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let pan = pan_steps.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let report = Report::MouseReport(MouseReport {
            buttons: rmk::channel::mouse_button_state(),
            x: 0,
            y: 0,
            wheel,
            pan,
        });
        if BLE_REPORT_CHANNEL.try_send(report).is_ok() {
            self.wheel_accum_q8 = self.wheel_accum_q8.saturating_sub((wheel as i32).saturating_mul(Q8_ONE));
            self.pan_accum_q8 = self.pan_accum_q8.saturating_sub((pan as i32).saturating_mul(Q8_ONE));
            self.hid_reports = self.hid_reports.saturating_add(1);
            self.last_report = Instant::now();
        } else {
            self.hid_busy = self.hid_busy.saturating_add(1);
        }
    }

    fn emit_diag_if_due(&mut self) {
        if self.last_diag.elapsed() < Duration::from_millis(DIAG_INTERVAL_MS) { return; }
        self.last_diag = Instant::now();
        let motion_pin = self.sensor.motion_pin_active();
        info!(
            "PAW3222 BLE diag ready={} init_error={:?} pid=0x{:02x} 12bit={} mouse_opt=0x{:02x} motion_pin={} motion_reg=0x{:02x} reads={} events={} last_dx={} last_dy={} accum_x={} accum_y={} hid={} hid_busy={} read_err={} rotation={} mode={} gain_q8={} scroll_den={}",
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
            runtime::config(self.id).rotation().degrees(),
            runtime::effective_mode(self.id) as u8,
            runtime::effective_cursor_gain_q8(self.id),
            runtime::effective_scroll_scale_den(self.id),
        );
    }

    async fn on_pointing_set_cpi_event(&mut self, event: PointingSetCpiEvent) {
        if event.device_id != self.id || !self.ready { return; }
        runtime::config(self.id).set_cpi(event.cpi);
        info!("PAW3222 BLE set CPI {}", event.cpi);
        if self.sensor.set_resolution(event.cpi).await.is_err() { warn!("PAW3222 BLE set CPI failed"); }
    }
}

pub type NrfPaw3222BleProcessor = Paw3222BleProcessor<
    rmk::driver::bitbang_spi::BitBangSpiBus<embassy_nrf::gpio::Output<'static>, embassy_nrf::gpio::Flex<'static>>,
    embassy_nrf::gpio::Output<'static>,
    embassy_nrf::gpio::Input<'static>,
>;

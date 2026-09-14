#![allow(dead_code)]

use embassy_time::{Duration, Instant, Timer};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::spi::SpiBus;
use log::{error, info, warn};
use rmk::channel::USB_REPORT_CHANNEL;
use rmk::event::PointingSetCpiEvent;
use rmk::hid::Report;
use rmk::macros::processor;
use usbd_hid::descriptor::MouseReport;

const PRODUCT_ID1: u8 = 0x00;
const MOTION: u8 = 0x02;
const DELTA_X: u8 = 0x03;
const DELTA_Y: u8 = 0x04;
const OPERATION_MODE: u8 = 0x05;
const CONFIGURATION: u8 = 0x06;
const WRITE_PROTECT: u8 = 0x09;
const CPI_X: u8 = 0x0d;
const CPI_Y: u8 = 0x0e;
const DELTA_XY_HI: u8 = 0x12;
const MOUSE_OPTION: u8 = 0x19;

const PRODUCT_ID_PAW3222: u8 = 0x30;
const SPI_WRITE: u8 = 1 << 7;
const MOTION_STATUS_MOTION: u8 = 1 << 7;
const OPERATION_MODE_SLP_ENH: u8 = 1 << 4;
const OPERATION_MODE_SLP2_ENH: u8 = 1 << 3;
const OPERATION_MODE_SLP_MASK: u8 = OPERATION_MODE_SLP_ENH | OPERATION_MODE_SLP2_ENH;
const CONFIGURATION_RESET: u8 = 1 << 7;
const WRITE_PROTECT_ENABLE: u8 = 0x00;
const WRITE_PROTECT_DISABLE: u8 = 0x5a;
const MOUSE_OPTION_XY12BIT_ENH: u8 = 1 << 2;

const RESET_DELAY_MS: u64 = 2;
const PRODUCT_ID_RETRIES: u8 = 10;
const PRODUCT_ID_RETRY_MS: u64 = 100;
const RES_STEP: u16 = 38;
const RES_MIN: u16 = 16 * RES_STEP;
const RES_MAX: u16 = 127 * RES_STEP;
const REPORT_INTERVAL_MS: u64 = 8; // 125 Hz USB report cadence
const DIAG_INTERVAL_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MotionDelta {
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Paw3222Error {
    Spi,
    InvalidProductId(u8),
    InvalidResolution(u16),
}

/// PAW3222 transport/driver used for PG1KB Proto PH3 bring-up.
///
/// The actual PG1KB ZMK driver remains the hardware-behavior reference. In
/// particular, DELTA_X, DELTA_Y and DELTA_XY_HI are read while CS remains low.
/// v2 additionally enables PAW3222 12-bit mode explicitly and verifies the
/// MOUSE_OPTION readback, following the upstream RMK PAW3222 PR.
pub struct Paw3222<SPI, CS, MotionPin> {
    spi: SPI,
    cs: CS,
    motion: MotionPin,
    resolution_cpi: u16,
    force_awake: bool,
    twelve_bit: bool,
    last_product_id: u8,
    last_motion_reg: u8,
    mouse_option: u8,
}

impl<SPI, CS, MotionPin> Paw3222<SPI, CS, MotionPin>
where
    SPI: SpiBus,
    CS: OutputPin,
    MotionPin: InputPin,
{
    pub fn new(
        spi: SPI,
        mut cs: CS,
        motion: MotionPin,
        resolution_cpi: u16,
        force_awake: bool,
    ) -> Self {
        let _ = cs.set_high();
        Self {
            spi,
            cs,
            motion,
            resolution_cpi,
            force_awake,
            twelve_bit: false,
            last_product_id: 0,
            last_motion_reg: 0,
            mouse_option: 0,
        }
    }

    pub fn motion_pin_active(&mut self) -> bool {
        self.motion.is_low().unwrap_or(false)
    }

    pub fn last_product_id(&self) -> u8 {
        self.last_product_id
    }

    pub fn last_motion_reg(&self) -> u8 {
        self.last_motion_reg
    }

    pub fn mouse_option(&self) -> u8 {
        self.mouse_option
    }

    pub fn twelve_bit(&self) -> bool {
        self.twelve_bit
    }

    pub async fn configure(&mut self) -> Result<(), Paw3222Error> {
        let mut detected = false;

        for attempt in 0..PRODUCT_ID_RETRIES {
            match self.read_reg(PRODUCT_ID1).await {
                Ok(id) => {
                    self.last_product_id = id;
                    if id == PRODUCT_ID_PAW3222 {
                        detected = true;
                        info!("PAW3222 detected product_id=0x{:02x}", id);
                        break;
                    }
                    warn!(
                        "PAW3222 unexpected product_id=0x{:02x} attempt={}",
                        id,
                        attempt + 1
                    );
                }
                Err(_) => {
                    warn!("PAW3222 product ID read failed attempt={}", attempt + 1);
                }
            }

            Timer::after(Duration::from_millis(PRODUCT_ID_RETRY_MS)).await;
        }

        if !detected {
            return Err(Paw3222Error::InvalidProductId(self.last_product_id));
        }

        self.update_reg(CONFIGURATION, CONFIGURATION_RESET, CONFIGURATION_RESET)
            .await?;
        Timer::after(Duration::from_millis(RESET_DELAY_MS)).await;

        self.set_resolution(self.resolution_cpi).await?;
        self.enable_12bit_mode().await?;
        self.set_force_awake(self.force_awake).await?;

        // Match the working ZMK driver: clear stale motion state after reset.
        self.last_motion_reg = self.read_reg(MOTION).await?;
        let _ = self.read_reg(DELTA_X).await?;
        let _ = self.read_reg(DELTA_Y).await?;
        let _ = self.read_reg(DELTA_XY_HI).await?;

        Ok(())
    }

    async fn enable_12bit_mode(&mut self) -> Result<(), Paw3222Error> {
        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_DISABLE).await?;
        let update_result = self
            .update_reg(
                MOUSE_OPTION,
                MOUSE_OPTION_XY12BIT_ENH,
                MOUSE_OPTION_XY12BIT_ENH,
            )
            .await;
        let protect_result = self
            .write_reg(WRITE_PROTECT, WRITE_PROTECT_ENABLE)
            .await;
        update_result?;
        protect_result?;

        self.mouse_option = self.read_reg(MOUSE_OPTION).await?;
        self.twelve_bit = (self.mouse_option & MOUSE_OPTION_XY12BIT_ENH) != 0;

        if self.twelve_bit {
            info!(
                "PAW3222 12-bit mode confirmed mouse_option=0x{:02x}",
                self.mouse_option
            );
        } else {
            warn!(
                "PAW3222 12-bit enable did not stick mouse_option=0x{:02x}; using 8-bit deltas",
                self.mouse_option
            );
        }

        Ok(())
    }

    pub async fn read_motion(&mut self) -> Result<Option<MotionDelta>, Paw3222Error> {
        self.last_motion_reg = self.read_reg(MOTION).await?;
        if self.last_motion_reg & MOTION_STATUS_MOTION == 0 {
            return Ok(None);
        }

        self.read_delta().await.map(Some)
    }

    pub async fn read_delta(&mut self) -> Result<MotionDelta, Paw3222Error> {
        // Reproduce the known-working PG1KB ZMK sequence under one CS assertion:
        // DELTA_X -> byte, DELTA_Y -> byte, DELTA_XY_HI -> byte.
        self.cs.set_low().map_err(|_| Paw3222Error::Spi)?;

        let result = async {
            let mut x_lo = [0u8; 1];
            let mut y_lo = [0u8; 1];
            let mut hi = [0u8; 1];

            self.spi.write(&[DELTA_X]).await.map_err(|_| Paw3222Error::Spi)?;
            self.spi.read(&mut x_lo).await.map_err(|_| Paw3222Error::Spi)?;

            self.spi.write(&[DELTA_Y]).await.map_err(|_| Paw3222Error::Spi)?;
            self.spi.read(&mut y_lo).await.map_err(|_| Paw3222Error::Spi)?;

            self.spi
                .write(&[DELTA_XY_HI])
                .await
                .map_err(|_| Paw3222Error::Spi)?;
            self.spi.read(&mut hi).await.map_err(|_| Paw3222Error::Spi)?;

            let delta = if self.twelve_bit {
                let x_raw = (((hi[0] as u16) << 4) & 0x0f00) | x_lo[0] as u16;
                let y_raw = (((hi[0] as u16) << 8) & 0x0f00) | y_lo[0] as u16;
                MotionDelta {
                    x: sign_extend_12(x_raw),
                    y: sign_extend_12(y_raw),
                }
            } else {
                MotionDelta {
                    x: x_lo[0] as i8 as i16,
                    y: y_lo[0] as i8 as i16,
                }
            };

            Ok(delta)
        }
        .await;

        let _ = self.cs.set_high();
        result
    }

    pub async fn set_resolution(&mut self, resolution_cpi: u16) -> Result<(), Paw3222Error> {
        if !(RES_MIN..=RES_MAX).contains(&resolution_cpi) {
            return Err(Paw3222Error::InvalidResolution(resolution_cpi));
        }

        let value = (resolution_cpi / RES_STEP) as u8;

        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_DISABLE).await?;
        let update_result = async {
            self.write_reg(CPI_X, value).await?;
            self.write_reg(CPI_Y, value).await
        }
        .await;
        let protect_result = self
            .write_reg(WRITE_PROTECT, WRITE_PROTECT_ENABLE)
            .await;
        update_result?;
        protect_result?;

        self.resolution_cpi = resolution_cpi;
        Ok(())
    }

    pub async fn set_force_awake(&mut self, enable: bool) -> Result<(), Paw3222Error> {
        let value = if enable { 0 } else { OPERATION_MODE_SLP_MASK };

        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_DISABLE).await?;
        let update_result = self
            .update_reg(OPERATION_MODE, OPERATION_MODE_SLP_MASK, value)
            .await;
        let protect_result = self
            .write_reg(WRITE_PROTECT, WRITE_PROTECT_ENABLE)
            .await;
        update_result?;
        protect_result?;

        self.force_awake = enable;
        Ok(())
    }

    async fn read_reg(&mut self, address: u8) -> Result<u8, Paw3222Error> {
        self.cs.set_low().map_err(|_| Paw3222Error::Spi)?;

        let result = async {
            self.spi
                .write(&[address & 0x7f])
                .await
                .map_err(|_| Paw3222Error::Spi)?;
            let mut value = [0u8; 1];
            self.spi
                .read(&mut value)
                .await
                .map_err(|_| Paw3222Error::Spi)?;
            Ok(value[0])
        }
        .await;

        let _ = self.cs.set_high();
        result
    }

    async fn write_reg(&mut self, address: u8, value: u8) -> Result<(), Paw3222Error> {
        self.cs.set_low().map_err(|_| Paw3222Error::Spi)?;

        let result = self
            .spi
            .write(&[address | SPI_WRITE, value])
            .await
            .map_err(|_| Paw3222Error::Spi);

        let _ = self.cs.set_high();
        result
    }

    async fn update_reg(
        &mut self,
        address: u8,
        mask: u8,
        value: u8,
    ) -> Result<(), Paw3222Error> {
        let current = self.read_reg(address).await?;
        self.write_reg(address, (current & !mask) | (value & mask))
            .await
    }
}

/// Central-side diagnostic processor for PG1KB's right PAW3222.
///
/// This remains intentionally USB-only for sensor bring-up. It reports raw
/// sensor motion directly through RMK's USB HID channel, while RMK's `usb_log`
/// CDC ACM interface emits a one-second diagnostic heartbeat. After the raw USB
/// path is proven on hardware this processor will be replaced by RMK's native
/// PointingDevice/PointingProcessor path for the USB-vs-BLE comparison.
#[processor(subscribe = [PointingSetCpiEvent], poll_interval = 1)]
pub struct Paw3222Processor<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> {
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

impl<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> Paw3222Processor<SPI, CS, MotionPin> {
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
                    info!("PAW3222 processor ready device_id={}", self.id);
                }
                Err(err) => {
                    self.last_init_error = Some(err);
                    error!("PAW3222 init failed error={:?}", err);
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
            self.send_usb_report();
        }

        self.emit_diag_if_due();
    }

    fn send_usb_report(&mut self) {
        if self.accumulated_x == 0 && self.accumulated_y == 0 {
            self.last_report = Instant::now();
            return;
        }

        // MouseReport uses signed 8-bit X/Y. Keep any excess in the accumulator
        // so fast motion is emitted over subsequent reports instead of discarded.
        let x = self.accumulated_x.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        let y = self.accumulated_y.clamp(i8::MIN as i32, i8::MAX as i32) as i8;

        let report = Report::MouseReport(MouseReport {
            buttons: 0,
            x,
            y,
            wheel: 0,
            pan: 0,
        });

        if USB_REPORT_CHANNEL.try_send(report).is_ok() {
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
            "PAW3222 diag ready={} init_error={:?} pid=0x{:02x} 12bit={} mouse_opt=0x{:02x} motion_pin={} motion_reg=0x{:02x} reads={} events={} last_dx={} last_dy={} accum_x={} accum_y={} hid={} hid_busy={} read_err={}",
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

        info!("PAW3222 set CPI {}", event.cpi);
        if self.sensor.set_resolution(event.cpi).await.is_err() {
            warn!("PAW3222 set CPI failed");
        }
    }
}

/// Concrete nRF52840 type used by the config-macro initializer in central.rs.
pub type NrfPaw3222Processor = Paw3222Processor<
    rmk::driver::bitbang_spi::BitBangSpiBus<
        embassy_nrf::gpio::Output<'static>,
        embassy_nrf::gpio::Flex<'static>,
    >,
    embassy_nrf::gpio::Output<'static>,
    embassy_nrf::gpio::Input<'static>,
>;

fn sign_extend_12(value: u16) -> i16 {
    ((value << 4) as i16) >> 4
}

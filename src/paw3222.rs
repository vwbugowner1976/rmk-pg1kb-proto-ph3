#![allow(dead_code)]

use defmt::{error, info, warn};
use embassy_time::{Duration, Instant, Timer};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::spi::SpiBus;
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

const PRODUCT_ID_PAW3222: u8 = 0x30;
const SPI_WRITE: u8 = 1 << 7;
const MOTION_STATUS_MOTION: u8 = 1 << 7;
const OPERATION_MODE_SLP_ENH: u8 = 1 << 4;
const OPERATION_MODE_SLP2_ENH: u8 = 1 << 3;
const OPERATION_MODE_SLP_MASK: u8 = OPERATION_MODE_SLP_ENH | OPERATION_MODE_SLP2_ENH;
const CONFIGURATION_RESET: u8 = 1 << 7;
const WRITE_PROTECT_ENABLE: u8 = 0x00;
const WRITE_PROTECT_DISABLE: u8 = 0x5a;

const RESET_DELAY_MS: u64 = 2;
const PRODUCT_ID_RETRIES: u8 = 10;
const PRODUCT_ID_RETRY_MS: u64 = 100;
const RES_STEP: u16 = 38;
const RES_MIN: u16 = 16 * RES_STEP;
const RES_MAX: u16 = 127 * RES_STEP;
const REPORT_INTERVAL_MS: u64 = 8; // 125 Hz

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// PAW3222 transport/driver.
///
/// PG1KB uses one bidirectional SDIO line. RMK's BitBangSpiBus switches that
/// pin between output and input, while CS is managed here so a complete PAW3222
/// register operation remains under one chip-select assertion.
pub struct Paw3222<SPI, CS, MotionPin> {
    spi: SPI,
    cs: CS,
    motion: MotionPin,
    resolution_cpi: u16,
    force_awake: bool,
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
        }
    }

    pub fn motion_pin_active(&mut self) -> bool {
        self.motion.is_low().unwrap_or(false)
    }

    pub async fn configure(&mut self) -> Result<(), Paw3222Error> {
        let mut last_id = 0u8;
        let mut detected = false;

        for attempt in 0..PRODUCT_ID_RETRIES {
            match self.read_reg(PRODUCT_ID1).await {
                Ok(id) => {
                    last_id = id;
                    if id == PRODUCT_ID_PAW3222 {
                        detected = true;
                        info!("PAW3222 detected, product ID={=u8:#04x}", id);
                        break;
                    }
                    warn!(
                        "PAW3222 unexpected product ID={=u8:#04x}, attempt {=u8}",
                        id,
                        attempt + 1
                    );
                }
                Err(_) => {
                    warn!("PAW3222 product ID read failed, attempt {=u8}", attempt + 1);
                }
            }

            Timer::after(Duration::from_millis(PRODUCT_ID_RETRY_MS)).await;
        }

        if !detected {
            return Err(Paw3222Error::InvalidProductId(last_id));
        }

        self.update_reg(CONFIGURATION, CONFIGURATION_RESET, CONFIGURATION_RESET)
            .await?;
        Timer::after(Duration::from_millis(RESET_DELAY_MS)).await;

        self.set_resolution(self.resolution_cpi).await?;
        self.set_force_awake(self.force_awake).await?;

        // Match the working ZMK driver: clear stale motion state after reset.
        let _ = self.read_reg(MOTION).await?;
        let _ = self.read_reg(DELTA_X).await?;
        let _ = self.read_reg(DELTA_Y).await?;
        let _ = self.read_reg(DELTA_XY_HI).await?;

        Ok(())
    }

    pub async fn read_motion(&mut self) -> Result<Option<MotionDelta>, Paw3222Error> {
        let motion = self.read_reg(MOTION).await?;
        if motion & MOTION_STATUS_MOTION == 0 {
            return Ok(None);
        }

        self.read_delta().await.map(Some)
    }

    pub async fn read_delta(&mut self) -> Result<MotionDelta, Paw3222Error> {
        // Reproduce the working ZMK register sequence under one CS assertion:
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

            let x_raw = (((hi[0] as u16) << 4) & 0x0f00) | x_lo[0] as u16;
            let y_raw = (((hi[0] as u16) << 8) & 0x0f00) | y_lo[0] as u16;

            Ok(MotionDelta {
                x: sign_extend_12(x_raw),
                y: sign_extend_12(y_raw),
            })
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
        self.write_reg(CPI_X, value).await?;
        self.write_reg(CPI_Y, value).await?;
        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_ENABLE).await?;

        self.resolution_cpi = resolution_cpi;
        Ok(())
    }

    pub async fn set_force_awake(&mut self, enable: bool) -> Result<(), Paw3222Error> {
        let value = if enable { 0 } else { OPERATION_MODE_SLP_MASK };

        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_DISABLE).await?;
        self.update_reg(OPERATION_MODE, OPERATION_MODE_SLP_MASK, value)
            .await?;
        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_ENABLE).await?;

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

/// Central-side bring-up processor for PG1KB's right PAW3222.
///
/// During this first hardware bring-up, reports are written directly to RMK's
/// public USB HID report channel. RMK's config-macro initializes custom
/// `#[register_processor]` instances before it creates `keymap`, so a custom
/// processor cannot construct the stock `PointingProcessor` there. Once the
/// sensor path is proven, this temporary USB-only bridge will be replaced by a
/// proper PointingDevice/PointingProcessor integration for USB + BLE.
#[processor(subscribe = [PointingSetCpiEvent], poll_interval = 1)]
pub struct Paw3222Processor<SPI: SpiBus, CS: OutputPin, MotionPin: InputPin> {
    id: u8,
    sensor: Paw3222<SPI, CS, MotionPin>,
    init_attempted: bool,
    ready: bool,
    accumulated_x: i32,
    accumulated_y: i32,
    last_report: Instant,
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
            accumulated_x: 0,
            accumulated_y: 0,
            last_report: Instant::now(),
        }
    }

    async fn poll(&mut self) {
        if !self.init_attempted {
            self.init_attempted = true;
            match self.sensor.configure().await {
                Ok(()) => {
                    self.ready = true;
                    self.last_report = Instant::now();
                    info!("PAW3222 processor ready, device_id={=u8}", self.id);
                }
                Err(Paw3222Error::InvalidProductId(id)) => {
                    error!("PAW3222 init failed, product ID={=u8:#04x}", id);
                }
                Err(_) => {
                    error!("PAW3222 init failed");
                }
            }
            return;
        }

        if !self.ready {
            return;
        }

        if self.sensor.motion_pin_active() {
            match self.sensor.read_motion().await {
                Ok(Some(delta)) => {
                    self.accumulated_x = self.accumulated_x.saturating_add(delta.x as i32);
                    self.accumulated_y = self.accumulated_y.saturating_add(delta.y as i32);
                }
                Ok(None) => {}
                Err(_) => warn!("PAW3222 motion read failed"),
            }
        }

        if self.last_report.elapsed() < Duration::from_millis(REPORT_INTERVAL_MS) {
            return;
        }

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
            self.last_report = Instant::now();
        }
    }

    async fn on_pointing_set_cpi_event(&mut self, event: PointingSetCpiEvent) {
        if event.device_id != self.id || !self.ready {
            return;
        }

        info!("PAW3222 set CPI {=u16}", event.cpi);
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

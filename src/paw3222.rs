#![allow(dead_code)]

use embassy_time::{Duration, Timer};
use embedded_hal_async::spi::SpiDevice;

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
const RES_STEP: u16 = 38;
const RES_MIN: u16 = 16 * RES_STEP;
const RES_MAX: u16 = 127 * RES_STEP;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionDelta {
    pub x: i16,
    pub y: i16,
}

#[derive(Debug)]
pub enum Paw3222Error<E> {
    Bus(E),
    InvalidProductId(u8),
    InvalidResolution(u16),
}

pub struct Paw3222<SPI> {
    spi: SPI,
    resolution_cpi: u16,
    force_awake: bool,
}

impl<SPI> Paw3222<SPI>
where
    SPI: SpiDevice<u8>,
{
    pub fn new(spi: SPI, resolution_cpi: u16, force_awake: bool) -> Self {
        Self {
            spi,
            resolution_cpi,
            force_awake,
        }
    }

    pub fn release(self) -> SPI {
        self.spi
    }

    pub async fn configure(&mut self) -> Result<(), Paw3222Error<SPI::Error>> {
        let product_id = self.read_reg(PRODUCT_ID1).await?;
        if product_id != PRODUCT_ID_PAW3222 {
            return Err(Paw3222Error::InvalidProductId(product_id));
        }

        self.update_reg(CONFIGURATION, CONFIGURATION_RESET, CONFIGURATION_RESET)
            .await?;
        Timer::after(Duration::from_millis(RESET_DELAY_MS)).await;

        self.set_resolution(self.resolution_cpi).await?;
        self.set_force_awake(self.force_awake).await?;

        // Match the existing ZMK driver: clear stale motion data after reset.
        let _ = self.read_reg(MOTION).await?;
        let _ = self.read_reg(DELTA_X).await?;
        let _ = self.read_reg(DELTA_Y).await?;
        let _ = self.read_reg(DELTA_XY_HI).await?;

        Ok(())
    }

    pub async fn motion_detected(&mut self) -> Result<bool, Paw3222Error<SPI::Error>> {
        Ok(self.read_reg(MOTION).await? & MOTION_STATUS_MOTION != 0)
    }

    pub async fn read_delta(&mut self) -> Result<MotionDelta, Paw3222Error<SPI::Error>> {
        // The PAW3222 returns three 12-bit values through an interleaved SPI read.
        // DELTA_XY_HI lower nibble = X[11:8], upper nibble = Y[11:8].
        let tx = [DELTA_X, 0xff, DELTA_Y, 0xff, DELTA_XY_HI, 0xff];
        let mut rx = [0u8; 6];
        self.spi
            .transfer(&mut rx, &tx)
            .await
            .map_err(Paw3222Error::Bus)?;

        let x_raw = (((rx[5] as u16) << 4) & 0x0f00) | rx[1] as u16;
        let y_raw = (((rx[5] as u16) << 8) & 0x0f00) | rx[3] as u16;

        Ok(MotionDelta {
            x: sign_extend_12(x_raw),
            y: sign_extend_12(y_raw),
        })
    }

    pub async fn set_resolution(
        &mut self,
        resolution_cpi: u16,
    ) -> Result<(), Paw3222Error<SPI::Error>> {
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

    pub async fn set_force_awake(
        &mut self,
        enable: bool,
    ) -> Result<(), Paw3222Error<SPI::Error>> {
        let value = if enable { 0 } else { OPERATION_MODE_SLP_MASK };

        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_DISABLE).await?;
        self.update_reg(OPERATION_MODE, OPERATION_MODE_SLP_MASK, value)
            .await?;
        self.write_reg(WRITE_PROTECT, WRITE_PROTECT_ENABLE).await?;

        self.force_awake = enable;
        Ok(())
    }

    async fn read_reg(&mut self, address: u8) -> Result<u8, Paw3222Error<SPI::Error>> {
        let tx = [address, 0xff];
        let mut rx = [0u8; 2];
        self.spi
            .transfer(&mut rx, &tx)
            .await
            .map_err(Paw3222Error::Bus)?;
        Ok(rx[1])
    }

    async fn write_reg(
        &mut self,
        address: u8,
        value: u8,
    ) -> Result<(), Paw3222Error<SPI::Error>> {
        self.spi
            .write(&[address | SPI_WRITE, value])
            .await
            .map_err(Paw3222Error::Bus)
    }

    async fn update_reg(
        &mut self,
        address: u8,
        mask: u8,
        value: u8,
    ) -> Result<(), Paw3222Error<SPI::Error>> {
        let current = self.read_reg(address).await?;
        self.write_reg(address, (current & !mask) | (value & mask))
            .await
    }
}

fn sign_extend_12(value: u16) -> i16 {
    ((value << 4) as i16) >> 4
}

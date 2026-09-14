#![no_main]
#![no_std]

mod paw3222;
mod paw3222_ble;
mod runtime;
mod split_pointing_ble;

use rmk::macros::rmk_central;

#[rmk_central]
mod keyboard_central {
    #[register_processor(poll)]
    fn paw3222_input() -> crate::paw3222_ble::NrfPaw3222BleProcessor {
        use embassy_nrf::gpio::{Flex, Input, Level, Output, OutputDrive, Pull};
        use rmk::driver::bitbang_spi::BitBangSpiBus;

        // PG1KB Proto PH3 right trackball wiring:
        // SCLK=P1.05, SDIO=P1.07, CS=P1.03, MOTION=P1.15 (active low).
        let sck = Output::new(p.P1_05, Level::High, OutputDrive::Standard);
        let sdio = Flex::new(p.P1_07);
        let cs = Output::new(p.P1_03, Level::High, OutputDrive::Standard);
        let motion = Input::new(p.P1_15, Pull::Up);
        let spi = BitBangSpiBus::new(sck, sdio);

        crate::paw3222_ble::Paw3222BleProcessor::new(
            crate::runtime::RIGHT_TRACKBALL_ID,
            spi,
            cs,
            motion,
            crate::runtime::DEFAULT_CPI,
            false,
        )
    }

    #[register_processor(event)]
    fn left_split_pointing() -> crate::split_pointing_ble::SplitPointingBleProcessor {
        crate::split_pointing_ble::SplitPointingBleProcessor::new(crate::runtime::LEFT_TRACKBALL_ID)
    }
}

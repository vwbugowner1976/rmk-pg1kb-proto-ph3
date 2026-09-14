#![no_main]
#![no_std]

mod paw3222;
mod paw3222_split;
mod runtime;
mod trackball_config;

use rmk::macros::rmk_peripheral;

#[rmk_peripheral(id = 0)]
mod keyboard_peripheral {
    #[register_processor(poll)]
    fn paw3222_left() -> crate::paw3222_split::NrfPaw3222SplitProcessor {
        use embassy_nrf::gpio::{Flex, Input, Level, Output, OutputDrive, Pull};
        use rmk::driver::bitbang_spi::BitBangSpiBus;

        // PG1KB Proto PH3 left trackball wiring is the same sensor pinout as right:
        // SCLK=P1.05, SDIO=P1.07, CS=P1.03, MOTION=P1.15 (active low).
        let sck = Output::new(p.P1_05, Level::High, OutputDrive::Standard);
        let sdio = Flex::new(p.P1_07);
        let cs = Output::new(p.P1_03, Level::High, OutputDrive::Standard);
        let motion = Input::new(p.P1_15, Pull::Up);
        let spi = BitBangSpiBus::new(sck, sdio);

        crate::paw3222_split::Paw3222SplitProcessor::new(
            crate::runtime::LEFT_TRACKBALL_ID,
            spi,
            cs,
            motion,
            crate::runtime::DEFAULT_CPI,
            false,
        )
    }
}

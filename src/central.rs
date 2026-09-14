#![no_main]
#![no_std]

mod paw3222;

use rmk::macros::rmk_central;

#[rmk_central]
mod keyboard_central {
    #[register_processor(poll)]
    fn paw3222_input() -> crate::paw3222::NrfPaw3222Processor {
        use embassy_nrf::gpio::{Flex, Input, Level, Output, OutputDrive, Pull};
        use rmk::driver::bitbang_spi::BitBangSpiBus;

        // PG1KB Proto PH3 right trackball wiring:
        // SCLK=P1.05, SDIO=P1.07, CS=P1.03, MOTION=P1.15 (active low).
        let sck = Output::new(p.P1_05, Level::High, OutputDrive::Standard);
        let sdio = Flex::new(p.P1_07);
        let cs = Output::new(p.P1_03, Level::High, OutputDrive::Standard);
        let motion = Input::new(p.P1_15, Pull::Up);
        let spi = BitBangSpiBus::new(sck, sdio);

        crate::paw3222::Paw3222Processor::new(
            0,     // right/central pointing device id
            spi,
            cs,
            motion,
            1178,  // current PG1KB CPI (31 * 38)
            false, // match existing ZMK power-saving behavior initially
        )
    }
}

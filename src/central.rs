#![no_main]
#![no_std]

mod paw3222;
mod paw3222_ble;
mod runtime;
mod split_pointing_ble;
mod trackball_config;
mod trackball_persistence;

use rmk::macros::rmk_central;

#[rmk_central]
mod keyboard_central {
    #[register_processor(poll)]
    fn paw3222_input() -> crate::paw3222_ble::NrfPaw3222BleProcessor {
        use embassy_nrf::gpio::{Flex, Input, Level, Output, OutputDrive, Pull};
        use rmk::driver::bitbang_spi::BitBangSpiBus;

        // Register the PG1KB private Rynk commands before the host session starts.
        // The build patch re-exports this hook from rmk::host so we do not depend
        // on the visibility of the internal rynk module itself.
        rmk::host::register_custom_handler(crate::runtime::handle_rynk_trackball);

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
            crate::trackball_config::RIGHT_TRACKBALL_CONFIG.cpi(),
            false,
        )
    }

    #[register_processor(poll)]
    fn left_split_pointing() -> crate::split_pointing_ble::SplitPointingBleProcessor {
        crate::split_pointing_ble::SplitPointingBleProcessor::new(crate::runtime::LEFT_TRACKBALL_ID)
    }

    #[register_processor(poll)]
    fn trackball_persistence() -> crate::trackball_persistence::TrackballPersistenceProcessor {
        crate::trackball_persistence::TrackballPersistenceProcessor::new()
    }
}

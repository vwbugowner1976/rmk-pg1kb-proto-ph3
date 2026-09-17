use embassy_time::{Duration, Instant};
use log::{info, warn};
use rmk::event::LayerChangeEvent;
use rmk::macros::processor;

use crate::runtime;

const LOAD_DELAY_MS: u64 = 1200;

#[processor(subscribe = [LayerChangeEvent], poll_interval = 100)]
pub struct TrackballPersistenceProcessor {
    started: Instant,
    loaded: bool,
}

impl TrackballPersistenceProcessor {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            loaded: false,
        }
    }

    async fn on_layer_change_event(&mut self, _event: LayerChangeEvent) {
        // Persistence is timer-driven; this subscription only satisfies RMK's
        // processor contract without changing trackball or BLE timing.
    }

    async fn poll(&mut self) {
        if !self.loaded && self.started.elapsed() >= Duration::from_millis(LOAD_DELAY_MS) {
            self.loaded = true;
            match rmk::host::pg1kb_read_trackball_config().await {
                Some(data) => {
                    runtime::apply_persisted_blob(&data);
                    info!("PG1KB trackball config restored from flash");
                }
                None => {
                    info!("PG1KB trackball config not found in flash; using firmware defaults");
                }
            }
        }

        if self.loaded && runtime::take_save_request() {
            let data = runtime::encode_persisted_blob();
            if rmk::host::pg1kb_write_trackball_config(data).await {
                info!("PG1KB trackball config saved to flash");
            } else {
                warn!("PG1KB trackball config flash save failed");
            }
        }
    }
}

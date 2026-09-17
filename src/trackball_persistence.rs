use embassy_time::{Duration, Instant};
use log::{info, warn};
use rmk::event::{LayerChangeEvent, PeripheralConnectedEvent};
use rmk::macros::processor;

use crate::runtime;

const LOAD_DELAY_MS: u64 = 1200;

#[processor(subscribe = [LayerChangeEvent, PeripheralConnectedEvent], poll_interval = 100)]
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

    async fn on_layer_change_event(&mut self, event: LayerChangeEvent) {
        runtime::set_active_layer(event.0);
        info!("PG1KB trackball layer profile {} active", event.0);
    }

    async fn on_peripheral_connected_event(&mut self, event: PeripheralConnectedEvent) {
        if event.connected {
            // Re-publish the currently configured left CPI after every split reconnect.
            // The split driver forwards this to the peripheral PAW3222 processor.
            runtime::resync_left_cpi();
            info!("PG1KB split peripheral {} connected; left CPI re-synced", event.id);
        }
    }

    async fn poll(&mut self) {
        if !self.loaded && self.started.elapsed() >= Duration::from_millis(LOAD_DELAY_MS) {
            self.loaded = true;
            match rmk::host::pg1kb_read_trackball_config().await {
                Some(data) => {
                    runtime::apply_persisted_blob(&data);
                    // A split peer may have connected before the delayed flash restore.
                    runtime::resync_left_cpi();
                    info!("PG1KB trackball config restored from flash");
                }
                None => {
                    info!("PG1KB trackball config not found in flash; using firmware defaults");
                }
            }
        }

        if self.loaded {
            if let Some(generation) = runtime::take_save_request() {
                let data = runtime::encode_persisted_blob();
                let ok = rmk::host::pg1kb_write_trackball_config(data).await;
                runtime::complete_save(generation, ok);
                if ok {
                    info!("PG1KB trackball config saved to flash generation={}", generation);
                } else {
                    warn!("PG1KB trackball config flash save failed generation={}", generation);
                }
            }
        }
    }
}

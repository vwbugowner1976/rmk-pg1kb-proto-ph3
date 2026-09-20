#!/usr/bin/env bash
set -euo pipefail

if [ -z "${PG1KB_RMK_ROOT:-}" ]; then
  echo "PG1KB_RMK_ROOT is not set" >&2
  exit 1
fi

FILE="$PG1KB_RMK_ROOT/src/split/ble/peripheral.rs"
python3 - "$FILE" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
s = p.read_text()
old = """        self.message_to_central
            .notify(self.conn, &gatt_msg, true)
            .await
            .map_err(|e| {
                error!("BLE notify error: {:?}", e);
                SplitDriverError::BleError(1)
            })?;
"""
new = """        if matches!(message, SplitMessage::Pointing(_)) {
            // Pointing is high-rate and stale relative motion is worse than a rare dropped sample.
            // Never let a full BLE outbound queue stall the whole split peripheral task for
            // hundreds of milliseconds. Also skip attribute-table storage for transient motion.
            match embassy_time::with_timeout(
                embassy_time::Duration::from_millis(6),
                self.message_to_central.notify(self.conn, &gatt_msg, false),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    error!("BLE pointing notify error: {:?}", e);
                    return Err(SplitDriverError::BleError(1));
                }
                Err(_) => {
                    // Drop this stale motion packet and immediately continue with newer input.
                    return Ok(gatt_msg.len);
                }
            }
        } else {
            self.message_to_central
                .notify(self.conn, &gatt_msg, true)
                .await
                .map_err(|e| {
                    error!("BLE notify error: {:?}", e);
                    SplitDriverError::BleError(1)
                })?;
        }
"""
if old not in s:
    raise SystemExit(f"Could not find stock split BLE notify block in {p}")
s = s.replace(old, new, 1)
p.write_text(s)
print("Patched RMK split Pointing: no attribute store + 6 ms send deadline")
print("Other split messages keep stock reliable behavior")
print(f"File: {p}")
PY

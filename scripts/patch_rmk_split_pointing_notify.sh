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

old_use = "use crate::event::{CentralConnectedEvent, KeyboardEvent, SleepStateEvent, SubscribableEvent, publish_event};"
new_use = "use crate::event::{Axis, CentralConnectedEvent, KeyboardEvent, SleepStateEvent, SubscribableEvent, publish_event};"
if old_use not in s:
    raise SystemExit(f"Could not find event import in {p}")
s = s.replace(old_use, new_use, 1)

old = """    conn: &'c GattConnection<'stack, 'server, P>,
}"""
new = """    conn: &'c GattConnection<'stack, 'server, P>,
    pointing_carry_x: i32,
    pointing_carry_y: i32,
}"""
if old not in s:
    raise SystemExit(f"Could not find driver fields in {p}")
s = s.replace(old, new, 1)

old = """            conn,
        }"""
new = """            conn,
            pointing_carry_x: 0,
            pointing_carry_y: 0,
        }"""
if old not in s:
    raise SystemExit(f"Could not find driver init in {p}")
s = s.replace(old, new, 1)

start = s.find("impl<'stack, 'server, 'c, P: PacketPool> SplitWriter for BleSplitPeripheralDriver")
if start < 0:
    raise SystemExit(f"Could not find SplitWriter impl in {p}")
fn_start = s.find("    async fn write(&mut self, message: &SplitMessage)", start)
fn_end = s.find("\n    }\n}\n", fn_start)
if fn_start < 0 or fn_end < 0:
    raise SystemExit(f"Could not locate SplitWriter::write in {p}")
fn_end += len("\n    }")

new_fn = """    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
        let mut outgoing = *message;

        if let SplitMessage::Pointing(ref mut event) = outgoing {
            for axis in &mut event.axes {
                match axis.axis {
                    Axis::X => {
                        let merged = (axis.value as i32).saturating_add(self.pointing_carry_x);
                        axis.value = merged.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    }
                    Axis::Y => {
                        let merged = (axis.value as i32).saturating_add(self.pointing_carry_y);
                        axis.value = merged.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    }
                    _ => {}
                }
            }
        }

        let gatt_msg = GattSplitMessage::try_from(&outgoing)?;
        debug!("Writing split message to central: {:?}", outgoing);

        if let SplitMessage::Pointing(event) = outgoing {
            match embassy_time::with_timeout(
                embassy_time::Duration::from_millis(6),
                self.message_to_central.notify(self.conn, &gatt_msg, false),
            )
            .await
            {
                Ok(Ok(())) => {
                    self.pointing_carry_x = 0;
                    self.pointing_carry_y = 0;
                }
                Ok(Err(e)) => {
                    error!("BLE pointing notify error: {:?}", e);
                    return Err(SplitDriverError::BleError(1));
                }
                Err(_) => {
                    for axis in event.axes {
                        match axis.axis {
                            Axis::X => self.pointing_carry_x = axis.value as i32,
                            Axis::Y => self.pointing_carry_y = axis.value as i32,
                            _ => {}
                        }
                    }
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

        Ok(gatt_msg.len)
    }"""

s = s[:fn_start] + new_fn + s[fn_end:]
p.write_text(s)

print("Patched RMK split Pointing: 6 ms deadline + carry unsent motion forward")
print("Other split messages keep stock behavior")
print(f"File: {p}")
PY

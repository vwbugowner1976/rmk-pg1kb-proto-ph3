#!/usr/bin/env bash
set -euo pipefail

RMK_ROOT="${PG1KB_RMK_ROOT:-}"
if [ -z "$RMK_ROOT" ] || [ ! -f "$RMK_ROOT/Cargo.toml" ]; then
  echo "PG1KB_RMK_ROOT must point at the project-local RMK 0.9.0 copy." >&2
  exit 1
fi

python3 - "$RMK_ROOT" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
storage = root / "src/storage/mod.rs"
host = root / "src/host/mod.rs"
split_mod = root / "src/split/mod.rs"
split_driver = root / "src/split/driver.rs"
split_peripheral = root / "src/split/peripheral.rs"

for p in [storage, host, split_mod, split_driver, split_peripheral]:
    if not p.exists():
        raise SystemExit(f"missing RMK source: {p}")

# ---------------------------------------------------------------------------
# 1) Small persistent 128-byte PG1KB trackball blob using RMK 0.9's storage task.
#    RMK 0.9 has no Flush message, so use a dedicated write-completion Signal.
# ---------------------------------------------------------------------------
s = storage.read_text()
marker = "PG1KB_TRACKBALL_STORAGE_V2"
if marker not in s:
    sig_anchor = 'static ACTIVE_BLE_PROFILE_RESPONSE: Signal<crate::RawMutex, Option<u8>> = Signal::new();\n'
    if sig_anchor not in s:
        raise SystemExit("storage signal anchor not found")
    s = s.replace(sig_anchor, sig_anchor + '''\n// PG1KB_TRACKBALL_STORAGE_V2\nstatic PG1KB_TRACKBALL_RESPONSE: Signal<crate::RawMutex, Option<[u8; 128]>> = Signal::new();\nstatic PG1KB_TRACKBALL_WRITE_RESPONSE: Signal<crate::RawMutex, bool> = Signal::new();\n\npub async fn pg1kb_read_trackball_config() -> Option<[u8; 128]> {\n    PG1KB_TRACKBALL_RESPONSE.reset();\n    FLASH_CHANNEL.send(FlashOperationMessage::ReadPg1kbTrackballConfig).await;\n    PG1KB_TRACKBALL_RESPONSE.wait().await\n}\n\npub async fn pg1kb_write_trackball_config(data: [u8; 128]) -> bool {\n    PG1KB_TRACKBALL_WRITE_RESPONSE.reset();\n    FLASH_CHANNEL.send(FlashOperationMessage::Pg1kbTrackballConfig(data)).await;\n    PG1KB_TRACKBALL_WRITE_RESPONSE.wait().await\n}\n\nmod pg1kb_trackball_bytes_serde {\n    use serde::{de::{Error as DeError, SeqAccess, Visitor}, Deserializer, Serializer};\n\n    pub fn serialize<S>(value: &[u8; 128], serializer: S) -> Result<S::Ok, S::Error>\n    where\n        S: Serializer,\n    {\n        serializer.serialize_bytes(value)\n    }\n\n    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 128], D::Error>\n    where\n        D: Deserializer<'de>,\n    {\n        struct BytesVisitor;\n        impl<'de> Visitor<'de> for BytesVisitor {\n            type Value = [u8; 128];\n            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {\n                formatter.write_str("exactly 128 bytes")\n            }\n            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>\n            where E: DeError {\n                if value.len() != 128 { return Err(E::invalid_length(value.len(), &self)); }\n                let mut bytes = [0u8; 128];\n                bytes.copy_from_slice(value);\n                Ok(bytes)\n            }\n            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>\n            where A: SeqAccess<'de> {\n                let mut bytes = [0u8; 128];\n                for (idx, slot) in bytes.iter_mut().enumerate() {\n                    *slot = seq.next_element()?.ok_or_else(|| A::Error::invalid_length(idx, &self))?;\n                }\n                if (seq.next_element::<u8>()?).is_some() {\n                    return Err(A::Error::invalid_length(129, &self));\n                }\n                Ok(bytes)\n            }\n        }\n        deserializer.deserialize_bytes(BytesVisitor)\n    }\n}\n''', 1)

    enum_anchor = '    ReadActiveBleProfile,\n}\n\n#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]\n'
    if enum_anchor not in s:
        raise SystemExit("storage FlashOperationMessage tail anchor not found")
    s = s.replace(enum_anchor, '''    ReadActiveBleProfile,\n    // PG1KB private persisted trackball settings.\n    Pg1kbTrackballConfig([u8; 128]),\n    ReadPg1kbTrackballConfig,\n}\n\n#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]\n''', 1)

    key_anchor = '    #[cfg(feature = "_ble")]\n    BondInfo(u8),\n'
    if key_anchor not in s:
        raise SystemExit("storage key anchor not found")
    s = s.replace(key_anchor, key_anchor + '    Pg1kbTrackballConfig,\n', 1)

    data_anchor = '    #[cfg(feature = "_ble")]\n    ActiveBleProfile(u8),\n'
    if data_anchor not in s:
        raise SystemExit("storage data anchor not found")
    s = s.replace(data_anchor, data_anchor + '    Pg1kbTrackballConfig(#[serde(with = "pg1kb_trackball_bytes_serde")] [u8; 128]),\n', 1)

    run_anchor = '''                #[cfg(feature = "_ble")]\n                FlashOperationMessage::ReadActiveBleProfile => {\n                    let resp = match self.fetch_data(StorageKey::ActiveBleProfile).await {\n                        Some(StorageData::ActiveBleProfile(v)) => Some(v),\n                        _ => None,\n                    };\n                    ACTIVE_BLE_PROFILE_RESPONSE.signal(resp);\n                    continue;\n                }\n\n'''
    if run_anchor not in s:
        raise SystemExit("storage run ReadActiveBleProfile anchor not found")
    s = s.replace(run_anchor, run_anchor + '''                FlashOperationMessage::Pg1kbTrackballConfig(data) => {\n                    let ok = self\n                        .store_data(\n                            StorageKey::Pg1kbTrackballConfig,\n                            &StorageData::Pg1kbTrackballConfig(data),\n                        )\n                        .await\n                        .is_ok();\n                    PG1KB_TRACKBALL_WRITE_RESPONSE.signal(ok);\n                    continue;\n                }\n                FlashOperationMessage::ReadPg1kbTrackballConfig => {\n                    let resp = match self.fetch_data(StorageKey::Pg1kbTrackballConfig).await {\n                        Some(StorageData::Pg1kbTrackballConfig(v)) => Some(v),\n                        _ => None,\n                    };\n                    PG1KB_TRACKBALL_RESPONSE.signal(resp);\n                    continue;\n                }\n\n''', 1)

    storage.write_text(s)
    print("Patched RMK 0.9 storage for PG1KB trackball persistence")
else:
    print("RMK PG1KB trackball storage patch already present")

# Re-export only the two storage helpers from rmk::host.
h = host.read_text()
export_marker = "PG1KB_TRACKBALL_STORAGE_EXPORT_V1"
if export_marker not in h:
    anchor = '#[cfg(feature = "storage")]\npub(crate) mod storage;\n'
    insertion = '''\n// PG1KB_TRACKBALL_STORAGE_EXPORT_V1\n#[cfg(feature = "storage")]\npub use crate::storage::{pg1kb_read_trackball_config, pg1kb_write_trackball_config};\n'''
    if anchor in h:
        h = h.replace(anchor, anchor + insertion, 1)
    else:
        service_anchor = '#[cfg(feature = "rynk")]\npub use rynk::RynkService as HostService;\n'
        if service_anchor not in h:
            raise SystemExit("host export anchor not found")
        h = h.replace(service_anchor, insertion + service_anchor, 1)
    host.write_text(h)
    print("Re-exported PG1KB trackball storage helpers from rmk::host")
else:
    print("RMK PG1KB storage helpers already exported")

# ---------------------------------------------------------------------------
# 2) Forward PointingSetCpiEvent central -> split peripheral.
#    SplitMessage derives serde + MaxSize, while PointingSetCpiEvent does not.
#    Serialize only primitive fields across the split link and reconstruct the
#    event on the peripheral side.
# ---------------------------------------------------------------------------
sm = split_mod.read_text()
split_marker = "PG1KB_SPLIT_CPI_V2"
if split_marker not in sm:
    variant_anchor = '    /// Led state, on/off, from central to peripheral\n    LedState(bool),\n'
    if variant_anchor not in sm:
        raise SystemExit("split message variant anchor not found")
    sm = sm.replace(variant_anchor, '''    // PG1KB_SPLIT_CPI_V2\n    /// Pointing CPI update, central to peripheral.\n    PointingSetCpi { device_id: u8, cpi: u16 },\n''' + variant_anchor, 1)
    split_mod.write_text(sm)
    print("Added serializable PointingSetCpi split message")
else:
    print("PointingSetCpi split message already present")

sd = split_driver.read_text()
if "PG1KB_SPLIT_CPI_DRIVER_V2" not in sd:
    import_anchor = '    KeyboardEvent, KeyboardEventPos, PeripheralConnectedEvent, SubscribableEvent, publish_event, publish_event_async,\n'
    if import_anchor not in sd:
        raise SystemExit("split driver event import anchor not found")
    sd = sd.replace(import_anchor, '    KeyboardEvent, KeyboardEventPos, PeripheralConnectedEvent, PointingSetCpiEvent, SubscribableEvent, publish_event, publish_event_async,\n', 1)

    sub_anchor = '        let mut sleep_sub = crate::event::SleepStateEvent::subscriber();\n'
    if sub_anchor not in sd:
        raise SystemExit("split driver subscriber anchor not found")
    sd = sd.replace(sub_anchor, sub_anchor + '        // PG1KB_SPLIT_CPI_DRIVER_V2\n        let mut pointing_cpi_sub = PointingSetCpiEvent::subscriber();\n', 1)

    select_anchor = '                    e = sleep_sub.next_event().fuse() => SplitMessage::SleepState(e.0),\n'
    if select_anchor not in sd:
        raise SystemExit("split driver select anchor not found")
    sd = sd.replace(select_anchor, select_anchor + '                    e = pointing_cpi_sub.next_event().fuse() => SplitMessage::PointingSetCpi { device_id: e.device_id, cpi: e.cpi },\n', 1)
    split_driver.write_text(sd)
    print("Forwarding PointingSetCpiEvent over split link as primitive fields")
else:
    print("Split CPI central forwarding already present")

sp = split_peripheral.read_text()
if "PG1KB_SPLIT_CPI_PERIPHERAL_V2" not in sp:
    import_anchor = '    KeyboardEvent, LayerChangeEvent, LedIndicatorEvent, PointingEvent, SleepStateEvent, SubscribableEvent,\n'
    if import_anchor not in sp:
        raise SystemExit("split peripheral event import anchor not found")
    sp = sp.replace(import_anchor, '    KeyboardEvent, LayerChangeEvent, LedIndicatorEvent, PointingEvent, PointingSetCpiEvent, SleepStateEvent, SubscribableEvent,\n', 1)

    match_anchor = '                        SplitMessage::KeyboardIndicator(indicator) => {\n'
    if match_anchor not in sp:
        raise SystemExit("split peripheral match anchor not found")
    sp = sp.replace(match_anchor, '''                        // PG1KB_SPLIT_CPI_PERIPHERAL_V2\n                        SplitMessage::PointingSetCpi { device_id, cpi } => {\n                            publish_event(PointingSetCpiEvent { device_id, cpi });\n                        }\n''' + match_anchor, 1)
    split_peripheral.write_text(sp)
    print("Reconstructing PointingSetCpiEvent on peripheral")
else:
    print("Split CPI peripheral handling already present")

print("VERIFY OK: PG1KB persistence + split CPI patches installed")
PY

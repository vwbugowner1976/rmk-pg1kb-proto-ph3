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
battery = root / "src/input_device/battery.rs"
adc = root / "rmk-macro/src/codegen/input_device/adc.rs"

for p in (battery, adc):
    if not p.exists():
        raise SystemExit(f"missing RMK source: {p}")

b = battery.read_text()
marker = "PG1KB_NON_LIPO_BATTERY_V1"
if marker not in b:
    b = b.replace(
        "pub struct BatteryProcessor {\n"
        "    adc_divider_measured: u32,\n"
        "    adc_divider_total: u32,\n",
        "pub struct BatteryProcessor {\n"
        "    // PG1KB_NON_LIPO_BATTERY_V1\n"
        "    adc_divider_measured: u32,\n"
        "    adc_divider_total: u32,\n"
        "    min_mv: u32,\n"
        "    max_mv: u32,\n"
        "    low_mv: u32,\n",
        1,
    )
    b = b.replace(
        "pub fn new(adc_divider_measured: u32, adc_divider_total: u32) -> Self {\n"
        "        BatteryProcessor {\n"
        "            adc_divider_measured,\n"
        "            adc_divider_total,\n"
        "            battery_status: BatteryStatus::Unavailable,\n"
        "        }\n"
        "    }",
        "pub fn new(adc_divider_measured: u32, adc_divider_total: u32, min_mv: u32, max_mv: u32, low_mv: u32) -> Self {\n"
        "        BatteryProcessor {\n"
        "            adc_divider_measured,\n"
        "            adc_divider_total,\n"
        "            min_mv,\n"
        "            max_mv,\n"
        "            low_mv,\n"
        "            battery_status: BatteryStatus::Unavailable,\n"
        "        }\n"
        "    }",
        1,
    )
    start = "    fn get_battery_percent(&self, val: u16) -> u8 {"
    i = b.find(start)
    if i < 0:
        raise SystemExit("BatteryProcessor::get_battery_percent not found")
    j = b.find("\n    }\n}", i)
    if j < 0:
        raise SystemExit("BatteryProcessor percent function end not found")
    new_fn = '''    fn get_battery_percent(&self, val: u16) -> u8 {
        // RMK's ADC event is the raw nRF SAADC code. Convert it back to
        // battery voltage using the same 1/6 gain, 0.6V reference and
        // 12-bit resolution used by the built-in ADC path.
        let raw = val as u64;
        let measured = self.adc_divider_measured as u64;
        let total = self.adc_divider_total as u64;
        if measured == 0 || total == 0 {
            error!("Battery ADC divider values must be greater than zero");
            return 0;
        }

        let adc_mv = (raw * 600 * 6) / 4096;
        let battery_mv = adc_mv * total / measured;

        if battery_mv <= self.min_mv as u64 {
            0
        } else if battery_mv >= self.max_mv as u64 {
            100
        } else {
            (((battery_mv - self.min_mv as u64) * 100)
                / (self.max_mv - self.min_mv) as u64) as u8
        }
    }

    fn battery_mv(&self, val: u16) -> u32 {
        let raw = val as u64;
        let measured = self.adc_divider_measured as u64;
        let total = self.adc_divider_total as u64;
        if measured == 0 || total == 0 {
            return 0;
        }
        ((raw * 600 * 6) / 4096 * total / measured) as u32
    }'''
    b = b[:i] + new_fn + b[j+6:]

    # Keep the original constructor unit tests but adapt them to PG1KB profile.
    b = b.replace(
        "BatteryProcessor::new(1, 0).get_battery_percent(2000)",
        "BatteryProcessor::new(1, 0, 1000, 1500, 1000).get_battery_percent(2000)",
    ).replace(
        "BatteryProcessor::new(0, 1).get_battery_percent(2000)",
        "BatteryProcessor::new(0, 1, 1000, 1500, 1000).get_battery_percent(2000)",
    ).replace(
        "BatteryProcessor::new(2000, 2806).get_battery_percent(2890)",
        "BatteryProcessor::new(2000, 2806, 1000, 1500, 1000).get_battery_percent(2890)",
    )

    # PG1KB uses 1.0V=0%, 1.5V=100%, 1.0V low threshold.
    # The low threshold is retained in the processor configuration so the
    # next safety layer can use the exact same profile; we deliberately do
    # not call sys_poweroff here because RMK 0.9's generated USB transport
    # does not expose the ZMK zmk_usb_is_powered() predicate to processors.
    b = b.replace(
        "        let val = event.0;\n        trace!("Detected battery ADC value: {:?}", val);",
        "        let val = event.0;\n        let battery_mv = self.battery_mv(val);\n        if battery_mv <= self.low_mv {\n            warn!("PG1KB non-LiPo battery low: {} mV", battery_mv);\n        }\n        trace!("Detected battery ADC value: {:?} (~{} mV)", val, battery_mv);",
        1,
    )
    battery.write_text(b)
    print("Patched RMK BatteryProcessor for PG1KB non-LiPo voltage mapping")
else:
    print("PG1KB non-LiPo BatteryProcessor patch already present")

a = adc.read_text()
marker = "PG1KB_NON_LIPO_BATTERY_ADC_V1"
if marker not in a:
    old = "::rmk::input_device::battery::BatteryProcessor::new(#adc_divider_measured, #adc_divider_total)"
    new = "::rmk::input_device::battery::BatteryProcessor::new(#adc_divider_measured, #adc_divider_total, 1000, 1500, 1000)"
    if old not in a:
        raise SystemExit("BatteryProcessor constructor call not found in RMK ADC macro")
    a = a.replace(
        old,
        new + " /* PG1KB_NON_LIPO_BATTERY_ADC_V1 */",
        1,
    )
    adc.write_text(a)
    print("Patched RMK ADC macro to use PG1KB non-LiPo profile")
else:
    print("PG1KB non-LiPo ADC macro patch already present")

print("VERIFY OK: PG1KB non-LiPo battery mapping installed")
PY

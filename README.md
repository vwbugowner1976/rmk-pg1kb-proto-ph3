# RMK PG1KB Proto PH3

Experimental RMK firmware for PG1KB Proto PH3 using Seeed Studio XIAO nRF52840 Plus controllers and PAW3222 trackball sensors.

## Status

Initial RMK port scaffold.

- RMK 0.9
- nRF52840 BLE split
- Right half: central
- Left half: peripheral
- PG1KB 5x6 matrix per half
- PAW3222 core driver lives in `src/paw3222.rs`
- PAW3222 hardware transport / RMK pointing-event integration is the next step
- Vial/Rynk intentionally disabled during the first bring-up

## Local build

See [`docs/LOCAL_BUILD.md`](docs/LOCAL_BUILD.md) for the WSL2 setup.

Quick build:

```bash
cargo build --release --bin central
cargo build --release --bin peripheral
cargo make uf2 --release
```

Outputs:

- `rmk-pg1kb-proto-ph3-central.uf2`
- `rmk-pg1kb-proto-ph3-peripheral.uf2`

## PAW3222 wiring

Both halves currently use the same sensor wiring:

| Signal | nRF52840 | XIAO nRF52840 Plus |
|---|---|---|
| SCLK | P1.05 | D18 |
| SDIO | P1.07 | D19 |
| CS | P1.03 | D17 |
| MOTION | P1.15 | D10 |

The existing ZMK PAW3222 driver is used as the behavioral reference while the RMK implementation is written in Rust.

# v7 notes

v7 builds on the hardware-verified v6 baseline and adds runtime-adjustable trackball configuration for MyKeebStudio.

Initial focus:

1. Sensor rotation per side: 0° / 90° / 180° / 270°
2. CPI per side
3. Cursor gain per side
4. Scroll speed per side
5. Scroll inertia enable + decay
6. Immediate runtime apply
7. Persistent save to flash after runtime behavior is verified

The right-hand low-level PAW3222/BLE path must remain unchanged while these controls are added.

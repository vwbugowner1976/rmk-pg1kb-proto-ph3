# Trackball runtime configuration plan

Target branch: `paw3222-v7-runtime-ui`

This configuration is intended to be controlled from MyKeebStudio without rebuilding firmware.

## Per-trackball settings

Each trackball has its own runtime settings:

- `cpi`
- `cursor_gain_q8`
- `scroll_scale_num`
- `scroll_scale_den`
- `inertia_enabled`
- `inertia_decay_num`
- `inertia_decay_den`
- `rotation`

## Rotation

Rotation is represented as one of four canonical sensor orientations:

- `0`   = 0°
- `90`  = 90° clockwise
- `180` = 180°
- `270` = 270° clockwise

The transform is applied to raw `(x, y)` before cursor/scroll processing:

```text
0°:   ( x,  y)
90°:  ( y, -x)
180°: (-x, -y)
270°: (-y,  x)
```

This is deliberately exposed as a rotation selector in MyKeebStudio rather than separate swap/invert toggles, because the physical sensor may be mounted at 90° increments and rotation is easier to reason about in the UI.

## Current hardware finding

The left sensor behavior seen during scroll testing is consistent with a possible 90° mounting offset. The v7 runtime UI should therefore allow left/right rotation to be changed live while observing cursor/scroll behavior.

## UI target

MyKeebStudio Trackball settings:

```text
Left Trackball
  Mode            Cursor / Scroll
  Sensor Rotation 0° / 90° / 180° / 270°
  CPI             ...
  Cursor Speed    ...
  Scroll Speed    ...
  Scroll Inertia  On / Off
  Inertia         ...

Right Trackball
  Mode            Cursor / Scroll
  Sensor Rotation 0° / 90° / 180° / 270°
  CPI             ...
  Cursor Speed    ...
  Scroll Speed    ...
  Scroll Inertia  On / Off
  Inertia         ...
```

Changes should apply immediately; an explicit Save action should persist them to flash later.

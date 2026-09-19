#!/bin/bash
set -euo pipefail

: "${PG1KB_RMK_ROOT:?PG1KB_RMK_ROOT must point at the project-local RMK 0.9.0 copy}"

python3 - "$PG1KB_RMK_ROOT" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
combo = root / "src" / "keyboard" / "combo.rs"
keyboard = root / "src" / "keyboard.rs"

c = combo.read_text()

c = c.replace(
    "use crate::event::KeyboardEvent;\n",
    "use crate::event::{KeyboardEvent, KeyboardEventPos};\n"
)

old = """    /// Update the combo's state when a key is pressed.
    /// Returns true if the combo is updated.
    pub(crate) fn update(&mut self, key_action: &KeyAction, key_event: KeyboardEvent, active_layer: u8) -> bool {
"""
new = """    /// PG1KB position-combo wire marker.
    ///
    /// Rynk's Combo wire format in RMK 0.9 stores trigger entries as KeyAction.
    /// To keep that wire format unchanged, PG1KB encodes a physical matrix
    /// position as Morse(0x80 | row << 4 | col). Normal combo actions continue
    /// to work unchanged. Rows 0..7 and columns 0..15 are representable.
    const POSITION_TAG: u8 = 0x80;

    fn position_from_token(token: &KeyAction) -> Option<(u8, u8)> {
        match token {
            KeyAction::Morse(raw) if raw & Self::POSITION_TAG != 0 => {
                let pos = raw & 0x7f;
                Some(((pos >> 4) & 0x07, pos & 0x0f))
            }
            _ => None,
        }
    }

    pub(crate) fn trigger_matches(token: &KeyAction, key_action: &KeyAction, key_event: KeyboardEvent) -> bool {
        if let Some((row, col)) = Self::position_from_token(token) {
            matches!(
                key_event.pos,
                KeyboardEventPos::Key(pos) if pos.row == row && pos.col == col
            )
        } else {
            token == key_action
        }
    }

    fn find_trigger_index(&self, key_action: &KeyAction, key_event: KeyboardEvent) -> Option<usize> {
        self.config
            .actions
            .iter()
            .position(|token| Self::trigger_matches(token, key_action, key_event))
    }

    pub(crate) fn contains_event(&self, key_action: &KeyAction, key_event: KeyboardEvent) -> bool {
        self.find_trigger_index(key_action, key_event).is_some()
    }

    /// Update the combo's state when a key is pressed.
    /// Returns true if the combo is updated.
    pub(crate) fn update(&mut self, key_action: &KeyAction, key_event: KeyboardEvent, active_layer: u8) -> bool {
"""
if old not in c:
    raise SystemExit("position-combo patch: combo update anchor not found")
c = c.replace(old, new, 1)

c = c.replace(
    "        let action_idx = self.config.find_key_action_index(key_action);\n",
    "        let action_idx = self.find_trigger_index(key_action, key_event);\n",
    1,
)

c = c.replace(
    "    pub(crate) fn reassert_if_triggered(&mut self, key_action: &KeyAction) -> bool {\n",
    "    pub(crate) fn reassert_if_triggered(&mut self, key_action: &KeyAction, key_event: KeyboardEvent) -> bool {\n",
    1,
)
c = c.replace(
    "        if let Some(i) = self.config.find_key_action_index(key_action) {\n",
    "        if let Some(i) = self.find_trigger_index(key_action, key_event) {\n",
    1,
)

c = c.replace(
    "    pub(crate) fn update_released(&mut self, key_action: &KeyAction) -> bool {\n        if let Some(i) = self.config.find_key_action_index(key_action) {\n",
    "    pub(crate) fn update_released(&mut self, key_action: &KeyAction, key_event: KeyboardEvent) -> bool {\n        if let Some(i) = self.find_trigger_index(key_action, key_event) {\n",
    1,
)

combo.write_text(c)

k = keyboard.read_text()

repls = [
    (
        "if event.pressed || c.config.contains(key_action) {",
        "if event.pressed || c.contains_event(key_action, event) {",
    ),
    (
        "!combo_actions.contains(&item.action)",
        "!combo_actions.iter().any(|token| Combo::trigger_matches(token, &item.action, item.event))",
    ),
    (
        "if combo.reassert_if_triggered(key_action) {",
        "if combo.reassert_if_triggered(key_action, event) {",
    ),
    (
        "if combo.config.contains(key_action) {",
        "if combo.contains_event(key_action, event) {",
    ),
    (
        "if combo.update_released(key_action) {",
        "if combo.update_released(key_action, event) {",
    ),
]
for old, new in repls:
    if old not in k:
        raise SystemExit(f"position-combo patch: keyboard anchor not found: {old}")
    k = k.replace(old, new)

keyboard.write_text(k)
PY

echo "Applied PG1KB position-based combo patch"

use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;

use crate::Keystroke;
use crate::Modifiers;

pub(super) fn keystroke(pressed: &ButtonInput<KeyCode>, key: KeyCode) -> Keystroke {
    Keystroke::new(Modifiers::from_pressed(pressed), key)
}

pub(super) const fn is_modifier(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
    )
}

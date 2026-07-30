//! Canonical keyboard keystrokes and their parser.

mod sequence;
mod sequence_matcher;

use std::fmt;
use std::str::FromStr;

use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bitflags::bitflags;
pub use sequence::EmptyKeystrokeSequenceError;
pub use sequence::KeystrokeSequence;
pub use sequence::KeystrokeSequenceParseError;
pub use sequence_matcher::MatchOutcome;
pub use sequence_matcher::SequenceMatcher;

bitflags! {
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    struct ModifierFlags: u8 {
        const CONTROL = 0b0001;
        const ALT = 0b0010;
        const SHIFT = 0b0100;
        const PLATFORM = 0b1000;
    }
}

/// Keyboard modifiers stored in their platform-canonical form.
///
/// The platform modifier is distinct from control on macOS and is normalized to control on every
/// other platform.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Modifiers(ModifierFlags);

impl Default for Modifiers {
    fn default() -> Self { Self::none() }
}

impl Modifiers {
    /// Returns the empty modifier set.
    #[must_use]
    pub const fn none() -> Self { Self(ModifierFlags::empty()) }

    /// Returns this set with control enabled.
    #[must_use]
    pub const fn with_control(self) -> Self {
        Self(ModifierFlags::from_bits_retain(
            self.0.bits() | ModifierFlags::CONTROL.bits(),
        ))
    }

    /// Returns this set with alt enabled.
    #[must_use]
    pub const fn with_alt(self) -> Self {
        Self(ModifierFlags::from_bits_retain(
            self.0.bits() | ModifierFlags::ALT.bits(),
        ))
    }

    /// Returns this set with shift enabled.
    #[must_use]
    pub const fn with_shift(self) -> Self {
        Self(ModifierFlags::from_bits_retain(
            self.0.bits() | ModifierFlags::SHIFT.bits(),
        ))
    }

    /// Returns this set with the platform shortcut modifier enabled.
    ///
    /// This is command on macOS and control on all other platforms.
    #[must_use]
    pub const fn with_platform(self) -> Self {
        match PlatformShortcutMode::current() {
            PlatformShortcutMode::Command => Self(ModifierFlags::from_bits_retain(
                self.0.bits() | ModifierFlags::PLATFORM.bits(),
            )),
            PlatformShortcutMode::Control => self.with_control(),
        }
    }

    /// Reports whether control is enabled.
    #[must_use]
    pub const fn has_control(self) -> bool { self.0.contains(ModifierFlags::CONTROL) }

    /// Reports whether alt is enabled.
    #[must_use]
    pub const fn has_alt(self) -> bool { self.0.contains(ModifierFlags::ALT) }

    /// Reports whether shift is enabled.
    #[must_use]
    pub const fn has_shift(self) -> bool { self.0.contains(ModifierFlags::SHIFT) }

    /// Reports whether the platform shortcut modifier is enabled.
    #[must_use]
    pub const fn has_platform(self) -> bool { self.0.contains(ModifierFlags::PLATFORM) }

    /// Creates canonical modifiers from the modifier keys currently held down.
    ///
    /// On macOS, physical Super keys are the platform shortcut modifier and Control keys remain
    /// control. On every other platform, physical Control keys are the platform shortcut modifier
    /// and physical Super keys do not contribute a modifier.
    #[must_use]
    pub fn from_pressed(pressed: &ButtonInput<KeyCode>) -> Self {
        let mut modifiers = Self::none();

        if modifier_pair_pressed(pressed, KeyCode::AltLeft, KeyCode::AltRight) {
            modifiers = modifiers.with_alt();
        }
        if modifier_pair_pressed(pressed, KeyCode::ShiftLeft, KeyCode::ShiftRight) {
            modifiers = modifiers.with_shift();
        }

        if cfg!(target_os = "macos") {
            if modifier_pair_pressed(pressed, KeyCode::SuperLeft, KeyCode::SuperRight) {
                modifiers = modifiers.with_platform();
            }
            if modifier_pair_pressed(pressed, KeyCode::ControlLeft, KeyCode::ControlRight) {
                modifiers = modifiers.with_control();
            }
        } else if modifier_pair_pressed(pressed, KeyCode::ControlLeft, KeyCode::ControlRight) {
            modifiers = modifiers.with_platform();
        }

        modifiers
    }

    fn insert(&mut self, modifier: Modifier) {
        match modifier {
            Modifier::Control => self.0.insert(ModifierFlags::CONTROL),
            Modifier::Alt => self.0.insert(ModifierFlags::ALT),
            Modifier::Shift => self.0.insert(ModifierFlags::SHIFT),
            Modifier::Platform => *self = self.with_platform(),
        }
    }
}

fn modifier_pair_pressed(pressed: &ButtonInput<KeyCode>, left: KeyCode, right: KeyCode) -> bool {
    pressed.pressed(left) || pressed.pressed(right)
}

/// A keyboard key and its canonical modifier set.
///
/// Parse a [`Keystroke`] from text such as `"platform-shift-p"`. The parser canonicalizes
/// modifier aliases and source ordering before the value is constructed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Keystroke {
    modifiers: Modifiers,
    key:       KeyCode,
}

impl Keystroke {
    /// Creates a keystroke from canonical modifiers and a physical key code.
    #[must_use]
    pub const fn new(modifiers: Modifiers, key: KeyCode) -> Self { Self { modifiers, key } }

    /// Returns this keystroke's canonical modifiers.
    #[must_use]
    pub const fn modifiers(self) -> Modifiers { self.modifiers }

    /// Returns this keystroke's physical key code.
    #[must_use]
    pub const fn key(self) -> KeyCode { self.key }
}

impl fmt::Display for Keystroke {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.has_platform() {
            formatter.write_str("platform-")?;
        }
        if self.modifiers.has_control() {
            formatter.write_str("ctrl-")?;
        }
        if self.modifiers.has_alt() {
            formatter.write_str("alt-")?;
        }
        if self.modifiers.has_shift() {
            formatter.write_str("shift-")?;
        }

        formatter.write_str(key_name(self.key).ok_or(fmt::Error)?)
    }
}

impl FromStr for Keystroke {
    type Err = KeystrokeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut modifiers = Modifiers::none();
        let mut remaining = input;

        loop {
            let Some((token, after_token)) = remaining.split_once('-') else {
                let offset = input.len() - remaining.len();
                let Some(key) = parse_key(remaining) else {
                    return Err(KeystrokeParseError::new(remaining, offset));
                };
                return Ok(Self::new(modifiers, key));
            };
            if after_token.is_empty() {
                return Err(KeystrokeParseError::new("", input.len()));
            }
            let token_offset = input.len() - remaining.len();
            let Some(modifier) = parse_modifier(token) else {
                return Err(KeystrokeParseError::new(token, token_offset));
            };
            modifiers.insert(modifier);
            remaining = after_token;

            if let Some((side, after_side)) = remaining.split_once('-')
                && matches!(side, "left" | "right")
            {
                remaining = after_side;
            }
        }
    }
}

/// A keystroke parse failure with the source token and its byte offset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeystrokeParseError {
    token:  String,
    offset: usize,
}

impl KeystrokeParseError {
    fn new(token: &str, offset: usize) -> Self {
        Self {
            token: token.to_owned(),
            offset,
        }
    }

    /// Returns the unrecognized source token.
    #[must_use]
    pub fn token(&self) -> &str { &self.token }

    /// Returns the offending token's byte offset in the parsed string.
    #[must_use]
    pub const fn offset(&self) -> usize { self.offset }

    pub(super) const fn at_offset(mut self, offset: usize) -> Self {
        self.offset += offset;
        self
    }
}

impl fmt::Display for KeystrokeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid keystroke token {:?} at byte offset {}",
            self.token, self.offset
        )
    }
}

impl std::error::Error for KeystrokeParseError {}

#[derive(Clone, Copy)]
pub(super) enum PlatformShortcutMode {
    Command,
    Control,
}

impl PlatformShortcutMode {
    pub(super) const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Command
        } else {
            Self::Control
        }
    }
}

#[derive(Clone, Copy)]
enum Modifier {
    Control,
    Alt,
    Shift,
    Platform,
}

fn parse_modifier(token: &str) -> Option<Modifier> {
    match token {
        "ctrl" | "control" | "ctrlleft" | "ctrlright" | "controlleft" | "controlright" => {
            Some(Modifier::Control)
        },
        "alt" | "opt" | "option" | "altleft" | "altright" | "optleft" | "optright"
        | "optionleft" | "optionright" => Some(Modifier::Alt),
        "shift" | "shiftleft" | "shiftright" => Some(Modifier::Shift),
        "cmd" | "command" | "super" | "win" | "platform" | "cmdleft" | "cmdright"
        | "commandleft" | "commandright" | "superleft" | "superright" | "winleft" | "winright"
        | "platformleft" | "platformright" => Some(Modifier::Platform),
        _ => None,
    }
}

const CANONICAL_KEY_MAPPINGS: &[(&str, KeyCode)] = &[
    ("a", KeyCode::KeyA),
    ("b", KeyCode::KeyB),
    ("c", KeyCode::KeyC),
    ("d", KeyCode::KeyD),
    ("e", KeyCode::KeyE),
    ("f", KeyCode::KeyF),
    ("g", KeyCode::KeyG),
    ("h", KeyCode::KeyH),
    ("i", KeyCode::KeyI),
    ("j", KeyCode::KeyJ),
    ("k", KeyCode::KeyK),
    ("l", KeyCode::KeyL),
    ("m", KeyCode::KeyM),
    ("n", KeyCode::KeyN),
    ("o", KeyCode::KeyO),
    ("p", KeyCode::KeyP),
    ("q", KeyCode::KeyQ),
    ("r", KeyCode::KeyR),
    ("s", KeyCode::KeyS),
    ("t", KeyCode::KeyT),
    ("u", KeyCode::KeyU),
    ("v", KeyCode::KeyV),
    ("w", KeyCode::KeyW),
    ("x", KeyCode::KeyX),
    ("y", KeyCode::KeyY),
    ("z", KeyCode::KeyZ),
    ("0", KeyCode::Digit0),
    ("1", KeyCode::Digit1),
    ("2", KeyCode::Digit2),
    ("3", KeyCode::Digit3),
    ("4", KeyCode::Digit4),
    ("5", KeyCode::Digit5),
    ("6", KeyCode::Digit6),
    ("7", KeyCode::Digit7),
    ("8", KeyCode::Digit8),
    ("9", KeyCode::Digit9),
    ("f1", KeyCode::F1),
    ("f2", KeyCode::F2),
    ("f3", KeyCode::F3),
    ("f4", KeyCode::F4),
    ("f5", KeyCode::F5),
    ("f6", KeyCode::F6),
    ("f7", KeyCode::F7),
    ("f8", KeyCode::F8),
    ("f9", KeyCode::F9),
    ("f10", KeyCode::F10),
    ("f11", KeyCode::F11),
    ("f12", KeyCode::F12),
    ("f13", KeyCode::F13),
    ("f14", KeyCode::F14),
    ("f15", KeyCode::F15),
    ("f16", KeyCode::F16),
    ("f17", KeyCode::F17),
    ("f18", KeyCode::F18),
    ("f19", KeyCode::F19),
    ("f20", KeyCode::F20),
    ("f21", KeyCode::F21),
    ("f22", KeyCode::F22),
    ("f23", KeyCode::F23),
    ("f24", KeyCode::F24),
    ("f25", KeyCode::F25),
    ("f26", KeyCode::F26),
    ("f27", KeyCode::F27),
    ("f28", KeyCode::F28),
    ("f29", KeyCode::F29),
    ("f30", KeyCode::F30),
    ("f31", KeyCode::F31),
    ("f32", KeyCode::F32),
    ("f33", KeyCode::F33),
    ("f34", KeyCode::F34),
    ("f35", KeyCode::F35),
    ("up", KeyCode::ArrowUp),
    ("down", KeyCode::ArrowDown),
    ("left", KeyCode::ArrowLeft),
    ("right", KeyCode::ArrowRight),
    ("escape", KeyCode::Escape),
    ("enter", KeyCode::Enter),
    ("tab", KeyCode::Tab),
    ("space", KeyCode::Space),
    ("backspace", KeyCode::Backspace),
    ("delete", KeyCode::Delete),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
    ("insert", KeyCode::Insert),
    ("capslock", KeyCode::CapsLock),
    ("contextmenu", KeyCode::ContextMenu),
    ("printscreen", KeyCode::PrintScreen),
    ("scrolllock", KeyCode::ScrollLock),
    ("pause", KeyCode::Pause),
    ("numlock", KeyCode::NumLock),
    ("backquote", KeyCode::Backquote),
    ("backslash", KeyCode::Backslash),
    ("bracketleft", KeyCode::BracketLeft),
    ("bracketright", KeyCode::BracketRight),
    ("comma", KeyCode::Comma),
    ("equal", KeyCode::Equal),
    ("minus", KeyCode::Minus),
    ("period", KeyCode::Period),
    ("quote", KeyCode::Quote),
    ("semicolon", KeyCode::Semicolon),
    ("slash", KeyCode::Slash),
];

fn parse_key(token: &str) -> Option<KeyCode> {
    if let Some((_, key)) = CANONICAL_KEY_MAPPINGS
        .iter()
        .find(|(name, _)| *name == token)
    {
        return Some(*key);
    }

    let canonical_name = match token {
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        "esc" => "escape",
        "return" => "enter",
        "del" => "delete",
        "pgup" => "pageup",
        "pgdown" => "pagedown",
        "ins" => "insert",
        "prtsc" => "printscreen",
        "grave" => "backquote",
        "lbracket" => "bracketleft",
        "rbracket" => "bracketright",
        "equals" => "equal",
        "dot" => "period",
        "apostrophe" => "quote",
        _ => return None,
    };

    if let Some((_, key)) = CANONICAL_KEY_MAPPINGS
        .iter()
        .find(|(name, _)| *name == canonical_name)
    {
        return Some(*key);
    }

    None
}

fn key_name(key: KeyCode) -> Option<&'static str> {
    CANONICAL_KEY_MAPPINGS
        .iter()
        .find_map(|(name, mapped_key)| (*mapped_key == key).then_some(*name))
}

#[cfg(test)]
mod tests {
    use bevy::input::ButtonInput;
    use bevy::input::keyboard::KeyCode;

    use super::CANONICAL_KEY_MAPPINGS;
    use super::Keystroke;
    use super::KeystrokeParseError;
    use super::Modifiers;

    fn parsed(input: &str) -> Result<Keystroke, KeystrokeParseError> { input.parse() }

    fn parse_error(input: &str) -> Result<KeystrokeParseError, KeystrokeParseError> {
        match parsed(input) {
            Ok(_) => Err(KeystrokeParseError::new(input, 0)),
            Err(error) => Ok(error),
        }
    }

    fn pressed(keys: &[KeyCode]) -> ButtonInput<KeyCode> {
        let mut pressed = ButtonInput::default();
        for key in keys {
            pressed.press(*key);
        }
        pressed
    }

    #[test]
    fn modifier_order_and_platform_aliases_are_canonical() -> Result<(), KeystrokeParseError> {
        let canonical = parsed("cmd-shift-p")?;

        assert_eq!(canonical, parsed("shift-cmd-p")?);
        assert_eq!(canonical, parsed("shift-super-p")?);
        assert_eq!(canonical, parsed("shift-win-p")?);
        assert_eq!(canonical, parsed("shift-platform-p")?);

        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn control_and_platform_are_distinct_on_macos() -> Result<(), KeystrokeParseError> {
        assert_ne!(parsed("ctrl-p")?, parsed("platform-p")?);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn control_and_platform_are_equal_off_macos() -> Result<(), KeystrokeParseError> {
        assert_eq!(parsed("ctrl-p")?, parsed("platform-p")?);

        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn constructed_platform_keystroke_equals_parsed_command_on_macos()
    -> Result<(), KeystrokeParseError> {
        let constructed = Keystroke::new(
            Modifiers::none().with_platform().with_shift(),
            KeyCode::KeyP,
        );
        let parsed = parsed("cmd-shift-p")?;

        assert_eq!(constructed, parsed);
        assert_eq!(parsed, constructed);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn constructed_platform_keystroke_equals_parsed_control_off_macos()
    -> Result<(), KeystrokeParseError> {
        let constructed = Keystroke::new(
            Modifiers::none().with_platform().with_shift(),
            KeyCode::KeyP,
        );
        let parsed = parsed("ctrl-shift-p")?;

        assert_eq!(constructed, parsed);
        assert_eq!(parsed, constructed);

        Ok(())
    }

    #[test]
    fn left_and_right_modifier_spellings_are_canonical() -> Result<(), KeystrokeParseError> {
        assert_eq!(parsed("ctrl-left-p")?, parsed("control-right-p")?);
        assert_eq!(parsed("alt-left-p")?, parsed("option-right-p")?);
        assert_eq!(parsed("shift-left-p")?, parsed("shiftright-p")?);
        assert_eq!(parsed("cmd-left-p")?, parsed("superright-p")?);

        Ok(())
    }

    #[test]
    fn pressed_control_matches_parsed_control() -> Result<(), KeystrokeParseError> {
        let keys = pressed(&[KeyCode::ControlLeft]);

        assert_eq!(
            Modifiers::from_pressed(&keys),
            parsed("ctrl-p")?.modifiers()
        );

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn pressed_control_matches_parsed_command_off_macos() -> Result<(), KeystrokeParseError> {
        let keys = pressed(&[KeyCode::ControlLeft]);

        assert_eq!(Modifiers::from_pressed(&keys), parsed("cmd-p")?.modifiers());

        Ok(())
    }

    #[test]
    fn pressed_alt_and_shift_match_parsed_modifiers() -> Result<(), KeystrokeParseError> {
        let keys = pressed(&[KeyCode::AltLeft, KeyCode::ShiftRight]);

        assert_eq!(
            Modifiers::from_pressed(&keys),
            parsed("alt-shift-p")?.modifiers()
        );

        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn pressed_super_matches_parsed_command_on_macos() -> Result<(), KeystrokeParseError> {
        let keys = pressed(&[KeyCode::SuperLeft]);

        assert_eq!(Modifiers::from_pressed(&keys), parsed("cmd-p")?.modifiers());

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn pressed_super_is_empty_off_macos() {
        let keys = pressed(&[KeyCode::SuperLeft]);

        assert_eq!(Modifiers::from_pressed(&keys), Modifiers::none());
    }

    #[test]
    fn display_round_trips_parsed_values() -> Result<(), KeystrokeParseError> {
        for &(name, key) in CANONICAL_KEY_MAPPINGS {
            let keystroke = parsed(name)?;
            assert_eq!(keystroke.key(), key);
            assert_eq!(keystroke, parsed(&keystroke.to_string())?);
        }

        Ok(())
    }

    #[test]
    fn invalid_tokens_preserve_their_offsets() -> Result<(), KeystrokeParseError> {
        let empty = parse_error("")?;
        assert_eq!(empty.token(), "");
        assert_eq!(empty.offset(), 0);

        let unknown_key = parse_error("cmd-unknown")?;
        assert_eq!(unknown_key.token(), "unknown");
        assert_eq!(unknown_key.offset(), 4);

        let unknown_modifier = parse_error("cmd-unknown-p")?;
        assert_eq!(unknown_modifier.token(), "unknown");
        assert_eq!(unknown_modifier.offset(), 4);

        let empty_leading_token = parse_error("-p")?;
        assert_eq!(empty_leading_token.token(), "");
        assert_eq!(empty_leading_token.offset(), 0);

        let missing_key = parse_error("ctrl")?;
        assert_eq!(missing_key.token(), "ctrl");
        assert_eq!(missing_key.offset(), 0);

        let trailing_separator = parse_error("p-")?;
        assert_eq!(trailing_separator.token(), "");
        assert_eq!(trailing_separator.offset(), 2);

        let trailing_separator_after_modifier = parse_error("cmd-p-")?;
        assert_eq!(trailing_separator_after_modifier.token(), "");
        assert_eq!(trailing_separator_after_modifier.offset(), 6);

        let trailing_separator_after_control = parse_error("ctrl-")?;
        assert_eq!(trailing_separator_after_control.token(), "");
        assert_eq!(trailing_separator_after_control.offset(), 5);

        Ok(())
    }

    #[test]
    fn side_token_is_a_key_only_when_no_key_follows() -> Result<(), KeystrokeParseError> {
        let control = Modifiers::none().with_control();

        assert_eq!(
            parsed("ctrl-left")?,
            Keystroke::new(control, KeyCode::ArrowLeft)
        );
        assert_eq!(
            parsed("ctrl-left-p")?,
            Keystroke::new(control, KeyCode::KeyP)
        );

        Ok(())
    }
}

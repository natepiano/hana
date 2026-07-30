//! Ordered non-empty keystroke sequences.

use std::fmt;
use std::str::FromStr;

use super::Keystroke;
use super::KeystrokeParseError;

/// An ordered, non-empty collection of keystrokes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeystrokeSequence {
    keystrokes: Vec<Keystroke>,
}

impl KeystrokeSequence {
    /// Creates a sequence when at least one keystroke is present.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyKeystrokeSequenceError`] when `keystrokes` is empty.
    pub fn new(keystrokes: Vec<Keystroke>) -> Result<Self, EmptyKeystrokeSequenceError> {
        if keystrokes.is_empty() {
            return Err(EmptyKeystrokeSequenceError);
        }

        Ok(Self { keystrokes })
    }

    /// Returns the keystrokes in source order.
    #[must_use]
    pub fn as_slice(&self) -> &[Keystroke] { &self.keystrokes }
}

impl TryFrom<Vec<Keystroke>> for KeystrokeSequence {
    type Error = EmptyKeystrokeSequenceError;

    fn try_from(keystrokes: Vec<Keystroke>) -> Result<Self, Self::Error> { Self::new(keystrokes) }
}

impl fmt::Display for KeystrokeSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, keystroke) in self.keystrokes.iter().enumerate() {
            if index != 0 {
                formatter.write_str(" ")?;
            }
            keystroke.fmt(formatter)?;
        }

        Ok(())
    }
}

impl FromStr for KeystrokeSequence {
    type Err = KeystrokeSequenceParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut keystrokes = Vec::new();
        let mut offset = 0;

        for token in input.split(' ') {
            let token_offset = offset;
            offset += token.len() + 1;

            if token.is_empty() {
                continue;
            }

            let keystroke = token.parse().map_err(|error: KeystrokeParseError| {
                KeystrokeSequenceParseError::Keystroke(error.at_offset(token_offset))
            })?;
            keystrokes.push(keystroke);
        }

        Self::new(keystrokes).map_err(KeystrokeSequenceParseError::Empty)
    }
}

/// An error returned when constructing an empty [`KeystrokeSequence`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmptyKeystrokeSequenceError;

impl fmt::Display for EmptyKeystrokeSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a keystroke sequence must contain at least one keystroke")
    }
}

impl std::error::Error for EmptyKeystrokeSequenceError {}

/// A sequence parse failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeystrokeSequenceParseError {
    /// The sequence contains no keystrokes.
    Empty(EmptyKeystrokeSequenceError),
    /// One source keystroke failed to parse.
    Keystroke(KeystrokeParseError),
}

impl fmt::Display for KeystrokeSequenceParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty(error) => error.fmt(formatter),
            Self::Keystroke(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for KeystrokeSequenceParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Empty(error) => Some(error),
            Self::Keystroke(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EmptyKeystrokeSequenceError;
    use super::KeystrokeSequence;
    use super::KeystrokeSequenceParseError;

    #[test]
    fn sequence_display_round_trips() -> Result<(), KeystrokeSequenceParseError> {
        let sequence: KeystrokeSequence = "cmd-k l".parse()?;

        assert_eq!(sequence.to_string().parse::<KeystrokeSequence>()?, sequence);

        Ok(())
    }

    #[test]
    fn empty_sequences_are_rejected() {
        assert_eq!(
            KeystrokeSequence::new(Vec::new()),
            Err(EmptyKeystrokeSequenceError)
        );
    }

    #[test]
    fn sequence_errors_use_the_full_source_offset() {
        let result = "cmd-k unknown-key".parse::<KeystrokeSequence>();

        assert!(matches!(
            result,
            Err(KeystrokeSequenceParseError::Keystroke(error))
                if error.token() == "unknown" && error.offset() == 6
        ));
    }
}

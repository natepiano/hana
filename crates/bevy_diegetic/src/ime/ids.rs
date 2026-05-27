//! Stable identifiers for IME sessions and panel fields.

use std::fmt::Display;
use std::fmt::Formatter;

/// Stable id for one active or recently completed IME session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImeSessionId(u64);

impl ImeSessionId {
    /// Creates a session id from a raw value.
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }

    /// Returns the raw id value.
    #[must_use]
    pub const fn value(self) -> u64 { self.0 }
}

impl Display for ImeSessionId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Stable id for one commit attempt inside an IME session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImeCommitAttemptId(u64);

impl ImeCommitAttemptId {
    /// Creates a commit attempt id from a raw value.
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }

    /// Returns the raw id value.
    #[must_use]
    pub const fn value(self) -> u64 { self.0 }
}

impl Display for ImeCommitAttemptId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Optional app/model revision reported after an applied value changes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImeValueRevision(u64);

impl ImeValueRevision {
    /// Creates a value revision from a raw value.
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }

    /// Returns the raw revision value.
    #[must_use]
    pub const fn value(self) -> u64 { self.0 }
}

/// Semantic field identity local to a diegetic panel or app-owned surface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PanelFieldId(String);

impl PanelFieldId {
    /// Creates a panel-local field id.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self { Self(value.into()) }

    /// Returns the field id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str { &self.0 }
}

impl Display for PanelFieldId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for PanelFieldId {
    fn from(value: &str) -> Self { Self::new(value) }
}

impl From<String> for PanelFieldId {
    fn from(value: String) -> Self { Self::new(value) }
}

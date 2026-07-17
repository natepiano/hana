/// Authored configuration for a panel button.
///
/// Attach it to an element with [`El::button`](crate::El::button). Runtime
/// button state and events are added by the button-behavior phase.
#[must_use]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Button {
    marker: (),
}

impl Button {
    /// Creates a button declaration with default behavior.
    pub const fn new() -> Self { Self { marker: () } }
}

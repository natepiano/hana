use std::time::Duration;

// sequence matching
/// Maximum elapsed time between keystrokes in one keymap sequence.
#[expect(
    dead_code,
    reason = "the keymap runtime consumes this policy in a later phase"
)]
pub(super) const SEQUENCE_TIMEOUT: Duration = Duration::from_secs(1);

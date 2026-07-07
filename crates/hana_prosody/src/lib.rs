//! Voice and transcription primitives for Hana.
//!
//! The crate owns microphone capture, temporary WAV encoding, and Apple Speech
//! transcription. Applications decide whether and where transcripts or audio
//! artifacts are written.

mod audio;
mod constants;
mod stt;

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub use audio::AudioInput;
pub use audio::AudioInputError;
pub use audio::AudioInputStatus;
pub use audio::write_wav;
pub use stt::PendingTranscription;
pub use stt::TranscriptionOutcome;
pub use stt::TranscriptionRequest;
pub use stt::spawn_transcription;

/// Current Unix timestamp in milliseconds.
#[must_use]
pub fn now_unix_millis() -> u64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    u64::try_from(duration.as_millis()).map_or(u64::MAX, |millis| millis)
}

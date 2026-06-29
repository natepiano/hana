//! Voice sidecar primitives for the Hana art-direction POC.
//!
//! The crate is intentionally split between reusable sidecar mechanics and the
//! Bevy example UI. The mechanics handle microphone samples, WAV output, JSONL
//! protocol writes, and optional transcription.

mod audio;
mod constants;
mod event_log;
mod stt;

pub use audio::AudioInput;
pub use audio::AudioInputError;
pub use audio::AudioInputStatus;
pub use audio::write_wav;
pub use event_log::RuntimeEvent;
pub use event_log::RuntimeLog;
pub use event_log::RuntimePaths;
pub use event_log::now_unix_millis;
pub use stt::PendingTranscription;
pub use stt::TranscriptionOutcome;
pub use stt::spawn_transcription;

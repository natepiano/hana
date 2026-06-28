//! Apple Speech transcription backend.

#[cfg(target_os = "macos")]
use std::env;
use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;

use speech::AuthorizationStatus;
use speech::SpeechError;
use speech::SpeechRecognizer;

#[cfg(target_os = "macos")]
const HANA_STT_LOCALE: &str = "HANA_STT_LOCALE";
#[cfg(target_os = "macos")]
const HANA_STT_REQUIRE_ON_DEVICE: &str = "HANA_STT_REQUIRE_ON_DEVICE";
#[cfg(target_os = "macos")]
const APPLE_CONTEXTUAL_STRINGS: &[&str] = &[
    "Hana",
    "Codex",
    "effects stack",
    "bloom",
    "glow",
    "emissive",
    "neon",
    "video",
    "shader",
    "camera",
];

/// A background transcription job.
#[derive(Debug)]
pub struct PendingTranscription {
    session_id: String,
    audio_path: PathBuf,
    receiver:   Mutex<Receiver<TranscriptionOutcome>>,
}

impl PendingTranscription {
    /// Session id being transcribed.
    #[must_use]
    pub fn session_id(&self) -> &str { &self.session_id }

    /// Audio path being transcribed.
    #[must_use]
    pub fn audio_path(&self) -> &Path { &self.audio_path }

    /// Tries to receive the finished transcription result.
    #[must_use]
    pub fn try_recv(&self) -> Option<TranscriptionOutcome> {
        let Ok(receiver) = self.receiver.lock() else {
            return Some(TranscriptionOutcome::Failed {
                session_id: self.session_id.clone(),
                audio_path: self.audio_path.clone(),
                error:      String::from("transcription receiver lock failed"),
            });
        };
        match receiver.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(TranscriptionOutcome::Failed {
                session_id: self.session_id.clone(),
                audio_path: self.audio_path.clone(),
                error:      String::from("transcription worker disconnected"),
            }),
        }
    }
}

/// Transcription result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptionOutcome {
    /// Transcription succeeded.
    Transcribed {
        /// Session id.
        session_id: String,
        /// Audio path.
        audio_path: PathBuf,
        /// Text produced by Apple Speech.
        text:       String,
        /// Transcription backend descriptor.
        backend:    String,
    },
    /// Transcription ran, but the window did not contain a usable utterance.
    Rejected {
        /// Session id.
        session_id: String,
        /// Audio path.
        audio_path: PathBuf,
        /// Rejection reason.
        reason:     String,
    },
    /// Transcription failed.
    Failed {
        /// Session id.
        session_id: String,
        /// Audio path.
        audio_path: PathBuf,
        /// Error message.
        error:      String,
    },
}

/// Starts transcription in a background thread.
#[must_use]
pub fn spawn_transcription(session_id: String, audio_path: PathBuf) -> PendingTranscription {
    let (sender, receiver) = mpsc::channel();
    let worker_session_id = session_id.clone();
    let worker_audio_path = audio_path.clone();
    thread::spawn(move || {
        let outcome = transcribe(worker_session_id, worker_audio_path);
        let _send_result = sender.send(outcome);
    });
    PendingTranscription {
        session_id,
        audio_path,
        receiver: Mutex::new(receiver),
    }
}

fn transcribe(session_id: String, audio_path: PathBuf) -> TranscriptionOutcome {
    match transcribe_with_apple_speech(&audio_path) {
        Ok(Transcript { text, backend }) => TranscriptionOutcome::Transcribed {
            session_id,
            audio_path,
            text,
            backend,
        },
        Err(TranscriptionError::NoSpeech(reason)) => TranscriptionOutcome::Rejected {
            session_id,
            audio_path,
            reason,
        },
        Err(error) => TranscriptionOutcome::Failed {
            session_id,
            audio_path,
            error: error.to_string(),
        },
    }
}

struct Transcript {
    text:    String,
    backend: String,
}

#[cfg(target_os = "macos")]
fn transcribe_with_apple_speech(audio_path: &Path) -> Result<Transcript, TranscriptionError> {
    AppleSpeechTranscriber::from_env().transcribe(audio_path)
}

#[cfg(not(target_os = "macos"))]
fn transcribe_with_apple_speech(_audio_path: &Path) -> Result<Transcript, TranscriptionError> {
    Err(TranscriptionError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
struct AppleSpeechTranscriber {
    locale:           Option<String>,
    recognition_mode: RecognitionMode,
}

#[cfg(target_os = "macos")]
impl AppleSpeechTranscriber {
    fn from_env() -> Self {
        let locale = env::var(HANA_STT_LOCALE)
            .ok()
            .filter(|locale| !locale.trim().is_empty());
        Self {
            locale,
            recognition_mode: recognition_mode_from_env(),
        }
    }

    fn transcribe(&self, audio_path: &Path) -> Result<Transcript, TranscriptionError> {
        use speech::prelude::*;

        let status = Self::request_authorization();
        if !status.is_authorized() {
            return Err(TranscriptionError::AppleSpeech(format!(
                "speech recognition not authorized: {status:?}"
            )));
        }

        let recognizer = self
            .recognizer()?
            .with_default_task_hint(TaskHint::Dictation)
            .with_callback_queue(CallbackQueue::named("hana-prosody-stt"));
        if !recognizer.is_available() {
            return Err(TranscriptionError::AppleSpeech(String::from(
                "Apple Speech recognizer is unavailable",
            )));
        }

        let locale = recognizer.locale_identifier()?;
        let on_device = recognizer.supports_on_device_recognition()?;
        if self.recognition_mode == RecognitionMode::OnDeviceRequired && !on_device {
            return Err(TranscriptionError::AppleSpeech(format!(
                "Apple Speech locale {locale} does not support on-device recognition"
            )));
        }

        let mut options = RecognitionRequestOptions::new()
            .with_task_hint(TaskHint::Dictation)
            .with_contextual_strings(APPLE_CONTEXTUAL_STRINGS.iter().copied())
            .with_should_report_partial_results(false)
            .with_adds_punctuation(true);
        if self.recognition_mode == RecognitionMode::OnDeviceRequired {
            options.set_requires_on_device_recognition(true);
        }

        let request = UrlRecognitionRequest::new(audio_path).with_options(options);
        let result = recognizer.recognize_request(&request)?;
        let text = result.transcript().trim().to_string();
        if !is_valid_transcript(&text) {
            return Err(TranscriptionError::NoSpeech(String::from(
                "candidate window did not contain a usable transcript",
            )));
        }
        let mode = match (self.recognition_mode, on_device) {
            (RecognitionMode::OnDeviceRequired, _) => "on-device",
            (RecognitionMode::SystemAllowed, true) => "system-allowed",
            (RecognitionMode::SystemAllowed, false) => "system",
        };
        Ok(Transcript {
            text,
            backend: format!("apple-speech:{locale}:{mode}"),
        })
    }

    fn request_authorization() -> AuthorizationStatus {
        let status = SpeechRecognizer::authorization_status();
        if status.is_authorized() {
            status
        } else {
            SpeechRecognizer::request_authorization()
        }
    }

    fn recognizer(&self) -> Result<SpeechRecognizer, TranscriptionError> {
        if let Some(locale) = &self.locale {
            return SpeechRecognizer::with_locale_checked(locale).ok_or_else(|| {
                TranscriptionError::AppleSpeech(format!(
                    "{HANA_STT_LOCALE} contains an interior NUL byte"
                ))
            });
        }
        Ok(SpeechRecognizer::new())
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecognitionMode {
    SystemAllowed,
    OnDeviceRequired,
}

#[derive(Debug)]
enum TranscriptionError {
    NoSpeech(String),
    #[cfg(target_os = "macos")]
    AppleSpeech(String),
    #[cfg(not(target_os = "macos"))]
    UnsupportedPlatform,
}

impl Display for TranscriptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSpeech(reason) => formatter.write_str(reason),
            #[cfg(target_os = "macos")]
            Self::AppleSpeech(error) => write!(formatter, "Apple Speech failed: {error}"),
            #[cfg(not(target_os = "macos"))]
            Self::UnsupportedPlatform => formatter.write_str("Apple Speech requires macOS"),
        }
    }
}

fn is_valid_transcript(text: &str) -> bool {
    let alphanumeric = text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count();
    alphanumeric >= 2 && text.chars().any(char::is_alphabetic)
}

impl Error for TranscriptionError {}

#[cfg(target_os = "macos")]
impl From<SpeechError> for TranscriptionError {
    fn from(error: SpeechError) -> Self { classify_speech_error(error.to_string()) }
}

#[cfg(target_os = "macos")]
fn classify_speech_error(message: String) -> TranscriptionError {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("no speech detected") || normalized.contains("no speech") {
        TranscriptionError::NoSpeech(message)
    } else {
        TranscriptionError::AppleSpeech(message)
    }
}

#[cfg(target_os = "macos")]
fn recognition_mode_from_env() -> RecognitionMode {
    if env::var(HANA_STT_REQUIRE_ON_DEVICE).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }) {
        RecognitionMode::OnDeviceRequired
    } else {
        RecognitionMode::SystemAllowed
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::TranscriptionError;
    #[cfg(target_os = "macos")]
    use super::classify_speech_error;
    use super::is_valid_transcript;

    #[test]
    fn accepts_short_word_commands() {
        assert!(is_valid_transcript("OK"));
        assert!(is_valid_transcript("rest"));
    }

    #[test]
    fn rejects_empty_or_punctuation_only_transcripts() {
        assert!(!is_valid_transcript(""));
        assert!(!is_valid_transcript("?"));
        assert!(!is_valid_transcript("..."));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_no_speech_errors_are_rejected_candidates() {
        let error = classify_speech_error(String::from("recognition failed: No speech detected"));

        assert!(matches!(error, TranscriptionError::NoSpeech(_)));
    }
}

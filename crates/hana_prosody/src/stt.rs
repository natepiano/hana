//! Apple Speech transcription backend.

#[cfg(target_os = "macos")]
use std::env;
use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;

#[cfg(target_os = "macos")]
use speech::AuthorizationStatus;
#[cfg(target_os = "macos")]
use speech::SpeechError;
#[cfg(target_os = "macos")]
use speech::SpeechRecognizer;

#[cfg(target_os = "macos")]
use crate::constants::APPLE_CONTEXTUAL_STRINGS;
#[cfg(target_os = "macos")]
use crate::constants::APPLE_SPEECH_CALLBACK_QUEUE;
#[cfg(target_os = "macos")]
use crate::constants::APPLE_SPEECH_RECOGNIZER_UNAVAILABLE;
use crate::constants::AUDIO_FILE_STEM_REPLACEMENT;
use crate::constants::AUDIO_FILE_STEM_SAFE_CHARACTERS;
use crate::constants::FALLBACK_AUDIO_FILE_STEM;
#[cfg(target_os = "macos")]
use crate::constants::HANA_STT_LOCALE;
#[cfg(target_os = "macos")]
use crate::constants::HANA_STT_REQUIRE_ON_DEVICE;
#[cfg(any(target_os = "macos", test))]
use crate::constants::MIN_TRANSCRIPT_ALPHANUMERICS;
#[cfg(target_os = "macos")]
use crate::constants::NO_SPEECH_ERROR_PATTERNS;
use crate::constants::TRANSCRIPTION_RECEIVER_LOCK_ERROR;
use crate::constants::TRANSCRIPTION_WORKER_DISCONNECTED_ERROR;
#[cfg(target_os = "macos")]
use crate::constants::TRUTHY_ENV_VALUES;
#[cfg(not(target_os = "macos"))]
use crate::constants::UNSUPPORTED_PLATFORM_ERROR;
#[cfg(target_os = "macos")]
use crate::constants::UNUSABLE_TRANSCRIPT_ERROR;
use crate::write_wav;

/// Audio samples and scratch location for one transcription request.
#[derive(Clone, Debug)]
pub struct TranscriptionRequest {
    /// Session id reported in the transcription outcome.
    pub session_id:  String,
    /// Sample rate in hertz.
    pub sample_rate: u32,
    /// Mono `f32` audio samples.
    pub samples:     Vec<f32>,
    /// Directory used for the temporary WAV required by Apple Speech.
    pub scratch_dir: PathBuf,
}

impl TranscriptionRequest {
    /// Creates a transcription request from in-memory samples.
    #[must_use]
    pub fn new(
        session_id: impl Into<String>,
        sample_rate: u32,
        samples: Vec<f32>,
        scratch_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            sample_rate,
            samples,
            scratch_dir: scratch_dir.into(),
        }
    }
}

/// A background transcription job.
#[derive(Debug)]
pub struct PendingTranscription {
    session_id: String,
    receiver:   Mutex<Receiver<TranscriptionOutcome>>,
}

impl PendingTranscription {
    /// Session id being transcribed.
    #[must_use]
    pub fn session_id(&self) -> &str { &self.session_id }

    /// Tries to receive the finished transcription result.
    #[must_use]
    pub fn try_recv(&self) -> Option<TranscriptionOutcome> {
        let Ok(receiver) = self.receiver.lock() else {
            return Some(TranscriptionOutcome::Failed {
                session_id: self.session_id.clone(),
                error:      String::from(TRANSCRIPTION_RECEIVER_LOCK_ERROR),
            });
        };
        match receiver.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(TranscriptionOutcome::Failed {
                session_id: self.session_id.clone(),
                error:      String::from(TRANSCRIPTION_WORKER_DISCONNECTED_ERROR),
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
        /// Text produced by Apple Speech.
        text:       String,
        /// Transcription backend descriptor.
        backend:    String,
    },
    /// Transcription ran, but the window did not contain a usable utterance.
    Rejected {
        /// Session id.
        session_id: String,
        /// Rejection reason.
        reason:     String,
    },
    /// Transcription failed.
    Failed {
        /// Session id.
        session_id: String,
        /// Error message.
        error:      String,
    },
}

/// Starts transcription in a background thread.
#[must_use]
pub fn spawn_transcription(request: TranscriptionRequest) -> PendingTranscription {
    let (sender, receiver) = mpsc::channel();
    let session_id = request.session_id.clone();
    thread::spawn(move || {
        let outcome = transcribe(request);
        let _send_result = sender.send(outcome);
    });
    PendingTranscription {
        session_id,
        receiver: Mutex::new(receiver),
    }
}

fn transcribe(request: TranscriptionRequest) -> TranscriptionOutcome {
    let session_id = request.session_id;
    let audio_path = request
        .scratch_dir
        .join(format!("{}.wav", audio_file_stem(&session_id)));
    if let Err(error) = write_wav(&audio_path, request.sample_rate, &request.samples) {
        return TranscriptionOutcome::Failed {
            session_id,
            error: format!("temporary WAV write failed: {error}"),
        };
    }

    match transcribe_with_apple_speech(&audio_path) {
        Ok(Transcript { text, backend }) => TranscriptionOutcome::Transcribed {
            session_id,
            text,
            backend,
        },
        #[cfg(target_os = "macos")]
        Err(TranscriptionError::NoSpeech(reason)) => {
            TranscriptionOutcome::Rejected { session_id, reason }
        },
        Err(error) => TranscriptionOutcome::Failed {
            session_id,
            error: error.to_string(),
        },
    }
    .with_temp_audio_removed(&audio_path)
}

trait TempAudioCleanup {
    fn with_temp_audio_removed(self, audio_path: &Path) -> Self;
}

impl TempAudioCleanup for TranscriptionOutcome {
    fn with_temp_audio_removed(self, audio_path: &Path) -> Self {
        match fs::remove_file(audio_path) {
            Ok(()) => self,
            Err(error) if error.kind() == ErrorKind::NotFound => self,
            Err(_error) => self,
        }
    }
}

fn audio_file_stem(session_id: &str) -> String {
    let stem: String = session_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric()
                || AUDIO_FILE_STEM_SAFE_CHARACTERS.contains(&character)
            {
                character
            } else {
                AUDIO_FILE_STEM_REPLACEMENT
            }
        })
        .collect();
    if stem.is_empty() {
        String::from(FALLBACK_AUDIO_FILE_STEM)
    } else {
        stem
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
const fn transcribe_with_apple_speech(
    _audio_path: &Path,
) -> Result<Transcript, TranscriptionError> {
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

        let authorization_status = Self::request_authorization();
        if !authorization_status.is_authorized() {
            return Err(TranscriptionError::AppleSpeech(format!(
                "speech recognition not authorized: {authorization_status:?}"
            )));
        }

        let speech_recognizer = self
            .recognizer()?
            .with_default_task_hint(TaskHint::Dictation)
            .with_callback_queue(CallbackQueue::named(APPLE_SPEECH_CALLBACK_QUEUE));
        if !speech_recognizer.is_available() {
            return Err(TranscriptionError::AppleSpeech(String::from(
                APPLE_SPEECH_RECOGNIZER_UNAVAILABLE,
            )));
        }

        let locale = speech_recognizer.locale_identifier()?;
        let on_device = speech_recognizer.supports_on_device_recognition()?;
        if self.recognition_mode == RecognitionMode::OnDeviceRequired && !on_device {
            return Err(TranscriptionError::AppleSpeech(format!(
                "Apple Speech locale {locale} does not support on-device recognition"
            )));
        }

        let mut recognition_request_options = RecognitionRequestOptions::new()
            .with_task_hint(TaskHint::Dictation)
            .with_contextual_strings(APPLE_CONTEXTUAL_STRINGS.iter().copied())
            .with_should_report_partial_results(false)
            .with_adds_punctuation(true);
        if self.recognition_mode == RecognitionMode::OnDeviceRequired {
            recognition_request_options.set_requires_on_device_recognition(true);
        }

        let url_recognition_request =
            UrlRecognitionRequest::new(audio_path).with_options(recognition_request_options);
        let detailed_recognition_result =
            speech_recognizer.recognize_request(&url_recognition_request)?;
        let text = detailed_recognition_result.transcript().trim().to_string();
        if !is_valid_transcript(&text) {
            return Err(TranscriptionError::NoSpeech(String::from(
                UNUSABLE_TRANSCRIPT_ERROR,
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
        let authorization_status = SpeechRecognizer::authorization_status();
        if authorization_status.is_authorized() {
            authorization_status
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
    #[cfg(target_os = "macos")]
    NoSpeech(String),
    #[cfg(target_os = "macos")]
    AppleSpeech(String),
    #[cfg(not(target_os = "macos"))]
    UnsupportedPlatform,
}

impl Display for TranscriptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_os = "macos")]
            Self::NoSpeech(reason) => formatter.write_str(reason),
            #[cfg(target_os = "macos")]
            Self::AppleSpeech(error) => write!(formatter, "Apple Speech failed: {error}"),
            #[cfg(not(target_os = "macos"))]
            Self::UnsupportedPlatform => formatter.write_str(UNSUPPORTED_PLATFORM_ERROR),
        }
    }
}

#[cfg(any(target_os = "macos", test))]
fn is_valid_transcript(text: &str) -> bool {
    let alphanumeric = text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count();
    alphanumeric >= MIN_TRANSCRIPT_ALPHANUMERICS && text.chars().any(char::is_alphabetic)
}

impl Error for TranscriptionError {}

#[cfg(target_os = "macos")]
impl From<SpeechError> for TranscriptionError {
    fn from(speech_error: SpeechError) -> Self { classify_speech_error(speech_error.to_string()) }
}

#[cfg(target_os = "macos")]
fn classify_speech_error(message: String) -> TranscriptionError {
    let normalized = message.to_ascii_lowercase();
    if NO_SPEECH_ERROR_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        TranscriptionError::NoSpeech(message)
    } else {
        TranscriptionError::AppleSpeech(message)
    }
}

#[cfg(target_os = "macos")]
fn recognition_mode_from_env() -> RecognitionMode {
    if env::var(HANA_STT_REQUIRE_ON_DEVICE)
        .is_ok_and(|value| TRUTHY_ENV_VALUES.contains(&value.trim().to_ascii_lowercase().as_str()))
    {
        RecognitionMode::OnDeviceRequired
    } else {
        RecognitionMode::SystemAllowed
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::TranscriptionError;
    use super::audio_file_stem;
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

    #[test]
    fn audio_file_stem_rejects_path_separators() {
        assert_eq!(audio_file_stem("../voice/1"), ".._voice_1");
        assert_eq!(audio_file_stem(""), "recording");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_no_speech_errors_are_rejected_recordings() {
        let error = classify_speech_error(String::from("recognition failed: No speech detected"));

        assert!(matches!(error, TranscriptionError::NoSpeech(_)));
    }
}

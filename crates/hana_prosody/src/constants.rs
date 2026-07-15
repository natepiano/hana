use std::time::Duration;

// audio
pub(crate) const AUDIO_READY_CHANNEL_CAPACITY: usize = 1;
pub(crate) const AUDIO_READY_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const AUDIO_THREAD_POLL: Duration = Duration::from_millis(50);
pub(crate) const DEFAULT_INPUT_DEVICE_NAME: &str = "default input";
pub(crate) const NO_DEFAULT_INPUT_ERROR: &str = "no default input device";
pub(crate) const PCM_SAMPLE_MAX: f32 = 1.0;
pub(crate) const PCM_SAMPLE_MIN: f32 = -1.0;
pub(crate) const UNSIGNED_SAMPLE_CENTER: f32 = 0.5;
pub(crate) const UNSIGNED_SAMPLE_SPAN: f32 = 2.0;
pub(crate) const WAV_BITS_PER_SAMPLE: u16 = 16;
pub(crate) const WAV_CHANNELS: u16 = 1;

// stt
#[cfg(target_os = "macos")]
pub(crate) const APPLE_CONTEXTUAL_STRINGS: &[&str] = &[
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
#[cfg(target_os = "macos")]
pub(crate) const APPLE_SPEECH_CALLBACK_QUEUE: &str = "hana-prosody-stt";
#[cfg(target_os = "macos")]
pub(crate) const APPLE_SPEECH_RECOGNIZER_UNAVAILABLE: &str =
    "Apple Speech recognizer is unavailable";
pub(crate) const AUDIO_FILE_STEM_REPLACEMENT: char = '_';
pub(crate) const AUDIO_FILE_STEM_SAFE_CHARACTERS: &[char] = &['-', '_', '.'];
pub(crate) const FALLBACK_AUDIO_FILE_STEM: &str = "recording";
#[cfg(target_os = "macos")]
pub(crate) const HANA_STT_LOCALE: &str = "HANA_STT_LOCALE";
#[cfg(target_os = "macos")]
pub(crate) const HANA_STT_REQUIRE_ON_DEVICE: &str = "HANA_STT_REQUIRE_ON_DEVICE";
#[cfg(any(target_os = "macos", test))]
pub(crate) const MIN_TRANSCRIPT_ALPHANUMERICS: usize = 2;
#[cfg(target_os = "macos")]
pub(crate) const NO_SPEECH_ERROR_PATTERNS: &[&str] = &["no speech detected", "no speech"];
pub(crate) const TRANSCRIPTION_RECEIVER_LOCK_ERROR: &str = "transcription receiver lock failed";
pub(crate) const TRANSCRIPTION_WORKER_DISCONNECTED_ERROR: &str =
    "transcription worker disconnected";
#[cfg(target_os = "macos")]
pub(crate) const TRUTHY_ENV_VALUES: &[&str] = &["1", "true", "yes", "on"];
#[cfg(not(target_os = "macos"))]
pub(crate) const UNSUPPORTED_PLATFORM_ERROR: &str = "Apple Speech requires macOS";
#[cfg(target_os = "macos")]
pub(crate) const UNUSABLE_TRANSCRIPT_ERROR: &str = "recording did not contain a usable transcript";

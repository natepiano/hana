use std::time::Duration;

// audio
pub(crate) const AUDIO_READY_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const AUDIO_THREAD_POLL: Duration = Duration::from_millis(50);

// event log
pub(crate) const DEFAULT_RUNTIME_DIR: &str = "../hana/run/art";

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
pub(crate) const HANA_STT_LOCALE: &str = "HANA_STT_LOCALE";
#[cfg(target_os = "macos")]
pub(crate) const HANA_STT_REQUIRE_ON_DEVICE: &str = "HANA_STT_REQUIRE_ON_DEVICE";

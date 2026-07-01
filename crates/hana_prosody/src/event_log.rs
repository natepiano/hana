//! Runtime JSONL protocol writes.

use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;

use crate::constants::DEFAULT_RUNTIME_DIR;

/// Runtime file locations used by the sidecar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimePaths {
    root:      PathBuf,
    inbox:     PathBuf,
    audio_dir: PathBuf,
}

impl RuntimePaths {
    /// Resolves runtime paths from `HANA_ART_RUN_DIR` or the default sibling
    /// Hana checkout path.
    #[must_use]
    pub fn from_env_or_default() -> Self {
        let root = env::var_os("HANA_ART_RUN_DIR").map_or_else(default_runtime_dir, PathBuf::from);
        Self::new(root)
    }

    /// Creates runtime paths rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            inbox: root.join("inbox.jsonl"),
            audio_dir: root.join("audio"),
            root,
        }
    }

    /// Creates all required directories.
    ///
    /// # Errors
    ///
    /// Returns any filesystem error from directory creation.
    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(&self.audio_dir)
    }

    /// Runtime root.
    #[must_use]
    pub fn root(&self) -> &Path { &self.root }

    /// Inbox JSONL path.
    #[must_use]
    pub fn inbox(&self) -> &Path { &self.inbox }

    /// Audio directory path.
    #[must_use]
    pub fn audio_dir(&self) -> &Path { &self.audio_dir }

    /// Returns the WAV path for `session_id`.
    #[must_use]
    pub fn audio_path(&self, session_id: &str) -> PathBuf {
        self.audio_dir.join(format!("{session_id}.wav"))
    }
}

/// Small append-only JSONL writer with sequence allocation.
#[derive(Debug)]
pub struct RuntimeLog {
    paths: RuntimePaths,
    seq:   u64,
}

impl RuntimeLog {
    /// Creates a log writer and prepares the runtime directories.
    ///
    /// # Errors
    ///
    /// Returns any filesystem error from runtime directory creation.
    pub fn new(paths: RuntimePaths) -> io::Result<Self> {
        paths.prepare()?;
        normalize_inbox(&paths.inbox)?;
        let seq = max_transcript_seq(&paths.inbox)?;
        Ok(Self { paths, seq })
    }

    /// Runtime paths.
    #[must_use]
    pub const fn paths(&self) -> &RuntimePaths { &self.paths }

    /// Allocates the next sequence number.
    pub const fn next_seq(&mut self) -> u64 {
        self.seq = self.seq.saturating_add(1);
        self.seq
    }

    /// Appends an event to the inbox.
    ///
    /// # Errors
    ///
    /// Returns any serialization or append error.
    pub fn append_inbox(&self, event: &RuntimeEvent) -> io::Result<()> {
        append_json_line(&self.paths.inbox, event)
    }
}

/// JSONL event written by the POC sidecar.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeEvent {
    /// STT produced text.
    TranscriptCommitted {
        /// Session id.
        session_id:         String,
        /// Sequence number.
        seq:                u64,
        /// Unix timestamp in milliseconds.
        created_at_unix_ms: u64,
        /// Transcribed text.
        text:               String,
        /// WAV file path.
        audio_path:         String,
        /// Transcription backend.
        backend:            String,
    },
}

/// Appends a single newline-delimited JSON event.
///
/// # Errors
///
/// Returns serialization or filesystem errors.
fn append_json_line(path: &Path, event: &RuntimeEvent) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event).map_err(io::Error::other)?;
    file.write_all(b"\n")
}

fn normalize_inbox(path: &Path) -> io::Result<()> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let transcript_lines: Vec<&str> = contents
        .lines()
        .filter(|line| is_transcript_line(line))
        .collect();
    if transcript_lines.len() == contents.lines().count() {
        return Ok(());
    }
    let mut normalized = transcript_lines.join("\n");
    if !normalized.is_empty() {
        normalized.push('\n');
    }
    fs::write(path, normalized)
}

fn is_transcript_line(line: &str) -> bool { transcript_seq(line).is_some() }

fn max_transcript_seq(path: &Path) -> io::Result<u64> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };

    Ok(contents
        .lines()
        .filter_map(transcript_seq)
        .max()
        .unwrap_or(0))
}

fn transcript_seq(line: &str) -> Option<u64> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return None;
    };
    value
        .get("kind")
        .and_then(Value::as_str)
        .filter(|kind| *kind == "transcript_committed")?;
    Some(value.get("seq").and_then(Value::as_u64).unwrap_or(0))
}

/// Current Unix timestamp in milliseconds.
#[must_use]
pub fn now_unix_millis() -> u64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    u64::try_from(duration.as_millis()).map_or(u64::MAX, |millis| millis)
}

fn default_runtime_dir() -> PathBuf {
    env::current_dir().map_or_else(
        |_| PathBuf::from(DEFAULT_RUNTIME_DIR),
        |current| default_runtime_dir_from(&current),
    )
}

fn default_runtime_dir_from(current: &Path) -> PathBuf {
    for ancestor in current.ancestors() {
        if ancestor
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "bevy_hana")
            && let Some(parent) = ancestor.parent()
        {
            return parent.join("hana").join("run").join("art");
        }
    }
    PathBuf::from(DEFAULT_RUNTIME_DIR)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering;

    use super::RuntimeLog;
    use super::RuntimePaths;
    use super::default_runtime_dir_from;
    use super::max_transcript_seq;
    use super::normalize_inbox;

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn derives_expected_paths() {
        let paths = RuntimePaths::new("run/art");

        assert_eq!(paths.inbox().to_string_lossy(), "run/art/inbox.jsonl");
        assert_eq!(paths.audio_dir().to_string_lossy(), "run/art/audio");
    }

    #[test]
    fn resolves_sibling_hana_runtime_from_workspace_subdir() {
        let current = Path::new("/Users/example/rust/bevy_hana/crates/hana_prosody");

        assert_eq!(
            default_runtime_dir_from(current),
            PathBuf::from("/Users/example/rust/hana/run/art")
        );
    }

    #[test]
    fn normalizes_inbox_to_committed_transcripts() -> Result<(), Box<dyn std::error::Error>> {
        let dir = temp_test_dir();
        std::fs::create_dir_all(&dir)?;
        let inbox = dir.join("inbox.jsonl");
        std::fs::write(
            &inbox,
            concat!(
                "{\"kind\":\"listen_armed\",\"session_id\":\"voice-1\"}\n",
                "{\"kind\":\"transcript_committed\",\"text\":\"make it glow\"}\n",
                "not json\n"
            ),
        )?;

        normalize_inbox(&inbox)?;

        assert_eq!(
            std::fs::read_to_string(&inbox)?,
            "{\"kind\":\"transcript_committed\",\"text\":\"make it glow\"}\n"
        );
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn runtime_log_next_seq_continues_existing_inbox() -> Result<(), Box<dyn std::error::Error>> {
        let dir = temp_test_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("inbox.jsonl"),
            concat!(
                "{\"kind\":\"transcript_committed\",\"seq\":3,\"text\":\"first\"}\n",
                "{\"kind\":\"transcript_committed\",\"seq\":8,\"text\":\"latest\"}\n"
            ),
        )?;

        let mut log = RuntimeLog::new(RuntimePaths::new(&dir))?;

        assert_eq!(max_transcript_seq(&dir.join("inbox.jsonl"))?, 8);
        assert_eq!(log.next_seq(), 9);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    fn temp_test_dir() -> PathBuf {
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "hana_prosody_event_log_test_{}_{}",
            std::process::id(),
            counter
        ))
    }
}

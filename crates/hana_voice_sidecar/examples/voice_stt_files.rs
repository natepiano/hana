//! Run Apple Speech over named WAV fixtures.
//!
//! Usage:
//! `cargo run -p hana_voice_sidecar --example voice_stt_files -- wav ...`

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use hana_voice_sidecar::TranscriptionOutcome;
use hana_voice_sidecar::spawn_transcription;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

fn main() {
    let paths: Vec<PathBuf> = env::args().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("usage: voice_stt_files wav ...");
        return;
    }

    for path in paths {
        transcribe_path(&path);
    }
}

fn transcribe_path(path: &Path) {
    let expected = expected_text(path);
    let pending = spawn_transcription(expected.clone(), path.to_path_buf());
    loop {
        if let Some(outcome) = pending.try_recv() {
            print_outcome(&expected, outcome);
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn print_outcome(expected: &str, outcome: TranscriptionOutcome) {
    match outcome {
        TranscriptionOutcome::Transcribed { text, backend, .. } => {
            let status = if normalized(&text) == normalized(expected) {
                "match"
            } else {
                "different"
            };
            println!("{status} | expected={expected:?} | actual={text:?} | {backend}");
        },
        TranscriptionOutcome::Rejected { reason, .. } => {
            println!("rejected | expected={expected:?} | {reason}");
        },
        TranscriptionOutcome::Failed { error, .. } => {
            println!("failed | expected={expected:?} | {error}");
        },
    }
}

fn expected_text(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map_or_else(String::new, |stem| stem.replace('_', " "))
}

fn normalized(text: &str) -> String {
    let text: String = text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if text == "okay" {
        String::from("ok")
    } else {
        text
    }
}

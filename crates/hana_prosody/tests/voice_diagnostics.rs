//! Diagnostic tests for generated speech fixtures and Apple Speech.

#[cfg(target_os = "macos")]
use std::error::Error;
#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::io;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;
#[cfg(target_os = "macos")]
use std::time::Instant;

#[cfg(target_os = "macos")]
use hana_prosody::PendingTranscription;
#[cfg(target_os = "macos")]
use hana_prosody::TranscriptionOutcome;
#[cfg(target_os = "macos")]
use hana_prosody::TranscriptionRequest;
#[cfg(target_os = "macos")]
use hana_prosody::spawn_transcription;

#[cfg(target_os = "macos")]
const FIXTURE_DIR_PREFIX: &str = "hana_prosody_voice_diagnostics";
#[cfg(target_os = "macos")]
const FIXTURE_PHRASES: &[&str] = &["test", "testing", "reset", "okay", "rest"];
#[cfg(target_os = "macos")]
const POLL_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(target_os = "macos")]
const STT_TIMEOUT: Duration = Duration::from_secs(20);

#[test]
fn fixture_file_names_round_trip_to_expected_text() {
    let path = Path::new("/tmp/make_me_neon.wav");

    assert_eq!(slugify("Make me neon"), "make_me_neon");
    assert_eq!(expected_text(path), "make me neon");
    assert_eq!(normalized("Okay"), normalized("OK"));
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires macOS say, afconvert, Apple Speech authorization, and generated audio fixtures"]
fn generated_say_fixtures_transcribe_with_apple_speech() -> Result<(), Box<dyn Error>> {
    let root = FixtureRoot::new("stt")?;
    for phrase in FIXTURE_PHRASES {
        let wav_path = generate_fixture(root.path(), phrase)?;
        let (sample_rate, samples) = read_wav_samples(&wav_path)?;
        let pending = spawn_transcription(TranscriptionRequest::new(
            *phrase,
            sample_rate,
            samples,
            root.path(),
        ));
        let outcome = wait_for_transcription(&pending)?;

        assert_transcript_matches(phrase, outcome)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct FixtureRoot {
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlugBoundary {
    Separator,
    Word,
}

#[cfg(target_os = "macos")]
impl FixtureRoot {
    fn new(label: &str) -> Result<Self, Box<dyn Error>> {
        let path = std::env::temp_dir().join(format!(
            "{FIXTURE_DIR_PREFIX}_{label}_{}",
            std::process::id()
        ));
        let _cleanup_result = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path { &self.path }
}

#[cfg(target_os = "macos")]
impl Drop for FixtureRoot {
    fn drop(&mut self) { let _cleanup_result = fs::remove_dir_all(&self.path); }
}

#[cfg(target_os = "macos")]
fn generate_fixture(out_dir: &Path, phrase: &str) -> Result<PathBuf, Box<dyn Error>> {
    let wav_path = out_dir.join(format!("{}.wav", slugify(phrase)));
    let aiff_path = wav_path.with_extension("aiff");

    let mut say = Command::new("say");
    say.arg("-o").arg(&aiff_path).arg(phrase);
    run_command(&mut say, "say", phrase)?;

    let mut afconvert = Command::new("afconvert");
    afconvert
        .args(["-f", "WAVE", "-d", "LEI16@48000", "-c", "1"])
        .arg(&aiff_path)
        .arg(&wav_path);
    run_command(&mut afconvert, "afconvert", phrase)?;

    fs::remove_file(aiff_path)?;
    Ok(wav_path)
}

#[cfg(target_os = "macos")]
fn run_command(command: &mut Command, tool: &str, phrase: &str) -> Result<(), Box<dyn Error>> {
    let status = command.status().map_err(|error| {
        io::Error::other(format!(
            "{tool} failed to start for phrase {phrase:?}: {error}"
        ))
    })?;
    if !status.success() {
        return Err(
            io::Error::other(format!("{tool} exited with {status} for phrase {phrase:?}")).into(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_wav_samples(path: &Path) -> Result<(u32, Vec<f32>), Box<dyn Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(io::Error::other(format!(
            "unsupported fixture WAV format: {:?} {} bits",
            spec.sample_format, spec.bits_per_sample
        ))
        .into());
    }

    let mut samples = Vec::new();
    for sample in reader.samples::<i16>() {
        samples.push(f32::from(sample?) / f32::from(i16::MAX));
    }
    Ok((spec.sample_rate, samples))
}

fn slugify(phrase: &str) -> String {
    let mut slug = String::new();
    let mut boundary = SlugBoundary::Separator;
    for character in phrase.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            boundary = SlugBoundary::Word;
        } else if boundary == SlugBoundary::Word {
            slug.push('_');
            boundary = SlugBoundary::Separator;
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }
    if slug.is_empty() {
        String::from("speech")
    } else {
        slug
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

#[cfg(target_os = "macos")]
fn wait_for_transcription(
    pending: &PendingTranscription,
) -> Result<TranscriptionOutcome, Box<dyn Error>> {
    let started = Instant::now();
    loop {
        if let Some(outcome) = pending.try_recv() {
            return Ok(outcome);
        }
        if started.elapsed() > STT_TIMEOUT {
            return Err(io::Error::other(format!(
                "Apple Speech timed out after {STT_TIMEOUT:?} for {}",
                pending.session_id()
            ))
            .into());
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(target_os = "macos")]
fn assert_transcript_matches(
    expected: &str,
    outcome: TranscriptionOutcome,
) -> Result<(), Box<dyn Error>> {
    match outcome {
        TranscriptionOutcome::Transcribed { text, backend, .. } => {
            if normalized(&text) == normalized(expected) {
                return Ok(());
            }
            Err(io::Error::other(format!(
                "Apple Speech returned {text:?} for {expected:?} via {backend}"
            ))
            .into())
        },
        TranscriptionOutcome::Rejected { reason, .. } => {
            Err(io::Error::other(format!("Apple Speech rejected {expected:?}: {reason}")).into())
        },
        TranscriptionOutcome::Failed { error, .. } => {
            Err(io::Error::other(format!("Apple Speech failed for {expected:?}: {error}")).into())
        },
    }
}

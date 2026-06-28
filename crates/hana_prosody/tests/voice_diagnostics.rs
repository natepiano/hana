//! Diagnostic tests for generated speech fixtures and the voice session loop.

use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use bevy_kana::ToF32;
use hana_prosody::PendingTranscription;
use hana_prosody::SessionConfig;
use hana_prosody::SessionEvent;
use hana_prosody::TranscriptionOutcome;
use hana_prosody::VoiceSession;
use hana_prosody::spawn_transcription;

const DEFAULT_CHUNK_MS: u64 = 16;
const DEFAULT_TAIL_MS: u64 = 1_500;
const FIXTURE_DIR_PREFIX: &str = "hana_prosody_voice_diagnostics";
const FIXTURE_PHRASES: &[&str] = &["test", "testing", "reset", "okay", "rest"];
const I24_MAX: f32 = 8_388_607.0;
const I32_MAX: f32 = 2_147_483_647.0;
const POLL_INTERVAL: Duration = Duration::from_millis(50);
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
#[ignore = "requires macOS say, afconvert, and generated audio fixtures"]
fn generated_say_fixtures_commit_through_voice_session() -> Result<(), Box<dyn Error>> {
    let root = FixtureRoot::new("vad")?;
    for phrase in FIXTURE_PHRASES {
        let wav_path = generate_fixture(root.path(), phrase)?;
        let (sample_rate, samples) = read_wav(&wav_path)?;
        let summary = replay_samples(sample_rate, &samples);

        assert!(
            summary.speech_starts > 0,
            "fixture {phrase:?} never started speech"
        );
        assert!(
            summary.commits > 0,
            "fixture {phrase:?} never committed audio"
        );
        assert!(
            summary.max_voice_probability > 0.0,
            "fixture {phrase:?} never produced VAD activity"
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires macOS say, afconvert, Apple Speech authorization, and generated audio fixtures"]
fn generated_say_fixtures_transcribe_with_apple_speech() -> Result<(), Box<dyn Error>> {
    let root = FixtureRoot::new("stt")?;
    for phrase in FIXTURE_PHRASES {
        let wav_path = generate_fixture(root.path(), phrase)?;
        let pending = spawn_transcription((*phrase).to_string(), wav_path);
        let outcome = wait_for_transcription(&pending)?;

        assert_transcript_matches(phrase, outcome)?;
    }
    Ok(())
}

#[derive(Debug)]
struct FixtureRoot {
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlugBoundary {
    Separator,
    Word,
}

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
#[derive(Debug)]
struct ReplaySummary {
    speech_starts:         usize,
    commits:               usize,
    probes:                usize,
    max_voice_probability: f32,
}

#[cfg(target_os = "macos")]
fn replay_samples(sample_rate: u32, samples: &[f32]) -> ReplaySummary {
    let chunk_samples = chunk_samples(sample_rate, DEFAULT_CHUNK_MS);
    let mut session = VoiceSession::new(SessionConfig::default(), sample_rate);
    let _event = session.arm("fixture");
    let mut summary = ReplaySummary {
        speech_starts:         0,
        commits:               0,
        probes:                0,
        max_voice_probability: 0.0,
    };

    for chunk in samples.chunks(chunk_samples) {
        let events = session.process_samples(chunk);
        update_summary(&mut summary, &events, &session);
    }

    let tail_samples = silent_samples(sample_rate, DEFAULT_TAIL_MS);
    for chunk in tail_samples.chunks(chunk_samples) {
        let events = session.process_samples(chunk);
        update_summary(&mut summary, &events, &session);
    }

    summary
}

#[cfg(target_os = "macos")]
fn update_summary(summary: &mut ReplaySummary, events: &[SessionEvent], session: &VoiceSession) {
    summary.max_voice_probability = summary
        .max_voice_probability
        .max(session.snapshot().vad_probability);
    for event in events {
        match event {
            SessionEvent::SpeechStarted { .. } => {
                summary.speech_starts = summary.speech_starts.saturating_add(1);
            },
            SessionEvent::AudioCommitted(_) => {
                summary.commits = summary.commits.saturating_add(1);
            },
            SessionEvent::CandidateReady(_) => {
                summary.probes = summary.probes.saturating_add(1);
            },
            SessionEvent::ListenArmed { .. }
            | SessionEvent::SpeechSettling { .. }
            | SessionEvent::SpeechTooShort { .. } => {},
        }
    }
}

#[cfg(target_os = "macos")]
fn read_wav(path: &Path) -> Result<(u32, Vec<f32>), Box<dyn Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let interleaved = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => reader
            .samples::<i16>()
            .map(|sample| sample.map(|sample| f32::from(sample) / f32::from(i16::MAX)))
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let max = int_sample_scale(spec.bits_per_sample);
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|sample| sample.to_f32() / max))
                .collect::<Result<Vec<_>, _>>()?
        },
    };

    if channels == 1 {
        return Ok((spec.sample_rate, interleaved));
    }

    let samples = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len().to_f32())
        .collect();
    Ok((spec.sample_rate, samples))
}

#[cfg(target_os = "macos")]
fn int_sample_scale(bits_per_sample: u16) -> f32 {
    match bits_per_sample {
        24 => I24_MAX,
        32 => I32_MAX,
        _ => f32::from(i16::MAX),
    }
}

#[cfg(target_os = "macos")]
fn chunk_samples(sample_rate: u32, chunk_ms: u64) -> usize {
    let samples = u64::from(sample_rate)
        .saturating_mul(chunk_ms.max(1))
        .saturating_div(1_000)
        .max(1);
    usize::try_from(samples).map_or(usize::MAX, |value| value)
}

#[cfg(target_os = "macos")]
fn silent_samples(sample_rate: u32, millis: u64) -> Vec<f32> {
    let samples = u64::from(sample_rate)
        .saturating_mul(millis)
        .saturating_div(1_000);
    vec![0.0; usize::try_from(samples).map_or(usize::MAX, |value| value)]
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
                pending.audio_path().display()
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

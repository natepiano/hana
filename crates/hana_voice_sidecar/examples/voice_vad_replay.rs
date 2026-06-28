//! Offline VAD/session replay for recorded sidecar WAVs.
//!
//! Usage:
//! `cargo run -p hana_voice_sidecar --example voice_vad_replay -- [--chunk-ms 16] [--tail-ms 1500]
//! [wav ...]`

use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use bevy_kana::ToF32;
use hana_voice_sidecar::RuntimePaths;
use hana_voice_sidecar::SessionConfig;
use hana_voice_sidecar::SessionEvent;
use hana_voice_sidecar::VoiceSession;

const DEFAULT_CHUNK_MS: u64 = 16;
const DEFAULT_TAIL_MS: u64 = 1_500;

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::from_env()?;
    let paths = replay_paths(&options)?;
    if paths.is_empty() {
        eprintln!("no WAV files found");
        return Ok(());
    }

    for path in paths {
        replay_path(&path, options.chunk_ms, options.tail_ms)?;
    }

    Ok(())
}

#[derive(Debug)]
struct Options {
    chunk_ms: u64,
    tail_ms:  u64,
    paths:    Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommitStatus {
    Pending,
    Committed,
}

impl CommitStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Pending => "no",
            Self::Committed => "yes",
        }
    }
}

impl Options {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        let mut chunk_ms = DEFAULT_CHUNK_MS;
        let mut tail_ms = DEFAULT_TAIL_MS;
        let mut paths = Vec::new();
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--chunk-ms" => {
                    let Some(value) = args.next() else {
                        return Err("--chunk-ms requires a value".into());
                    };
                    chunk_ms = value.parse()?;
                },
                "--tail-ms" => {
                    let Some(value) = args.next() else {
                        return Err("--tail-ms requires a value".into());
                    };
                    tail_ms = value.parse()?;
                },
                _ => paths.push(PathBuf::from(arg)),
            }
        }
        Ok(Self {
            chunk_ms,
            tail_ms,
            paths,
        })
    }
}

fn replay_paths(options: &Options) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    if !options.paths.is_empty() {
        return Ok(options.paths.clone());
    }

    let audio_dir = RuntimePaths::from_env_or_default()
        .audio_dir()
        .to_path_buf();
    let mut paths = Vec::new();
    for entry in fs::read_dir(audio_dir)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn replay_path(path: &Path, chunk_ms: u64, tail_ms: u64) -> Result<(), Box<dyn Error>> {
    let (sample_rate, samples) = read_wav(path)?;
    let chunk_samples = chunk_samples(sample_rate, chunk_ms);
    let mut session = VoiceSession::new(SessionConfig::default(), sample_rate);
    let _event = session.arm("replay");
    let mut max_voice = 0.0_f32;
    let mut commit_status = CommitStatus::Pending;

    println!(
        "\n{} | rate={} Hz | duration={} ms | chunk={} ms | tail={} ms",
        path.display(),
        sample_rate,
        duration_ms(samples.len(), sample_rate),
        chunk_ms,
        tail_ms
    );

    for (index, chunk) in samples.chunks(chunk_samples).enumerate() {
        let elapsed_ms = duration_ms(index.saturating_mul(chunk_samples), sample_rate);
        let events = session.process_samples(chunk);
        let snapshot = session.snapshot();
        max_voice = max_voice.max(snapshot.vad_probability);
        for event in events {
            print_event(elapsed_ms, &event);
            if matches!(event, SessionEvent::AudioCommitted(_)) {
                commit_status = CommitStatus::Committed;
            }
        }
    }
    let mut elapsed_ms = duration_ms(samples.len(), sample_rate);
    let tail_samples = silent_samples(sample_rate, tail_ms);
    for chunk in tail_samples.chunks(chunk_samples) {
        let events = session.process_samples(chunk);
        let snapshot = session.snapshot();
        max_voice = max_voice.max(snapshot.vad_probability);
        for event in events {
            print_event(elapsed_ms, &event);
            if matches!(event, SessionEvent::AudioCommitted(_)) {
                commit_status = CommitStatus::Committed;
            }
        }
        elapsed_ms = elapsed_ms.saturating_add(duration_ms(chunk.len(), sample_rate));
    }

    let snapshot = session.snapshot();
    println!(
        "final={} | committed={} | speech={} ms | silence={} ms | recorded={} ms | max_voice={:.2}",
        snapshot.phase.label(),
        commit_status.label(),
        snapshot.speech_ms,
        snapshot.silence_ms,
        snapshot.recorded_ms,
        max_voice
    );

    Ok(())
}

fn print_event(elapsed_ms: u64, event: &SessionEvent) {
    match event {
        SessionEvent::ListenArmed { session_id } => {
            println!("{elapsed_ms:>6} ms | armed | {session_id}");
        },
        SessionEvent::SpeechStarted { session_id } => {
            println!("{elapsed_ms:>6} ms | speech started | {session_id}");
        },
        SessionEvent::SpeechSettling { session_id } => {
            println!("{elapsed_ms:>6} ms | settling | {session_id}");
        },
        SessionEvent::AudioCommitted(committed) => {
            println!(
                "{elapsed_ms:>6} ms | committed | speech={} ms | silence={} ms | recorded={} ms",
                committed.speech_duration_ms, committed.silence_ms, committed.recorded_duration_ms
            );
        },
        SessionEvent::CandidateReady(committed) => {
            println!(
                "{elapsed_ms:>6} ms | probe | {} | speech={} ms | recorded={} ms",
                committed.session_id, committed.speech_duration_ms, committed.recorded_duration_ms
            );
        },
        SessionEvent::SpeechTooShort { session_id } => {
            println!("{elapsed_ms:>6} ms | too short | {session_id}");
        },
    }
}

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

fn int_sample_scale(bits_per_sample: u16) -> f32 {
    match bits_per_sample {
        24 => 8_388_607.0,
        32 => 2_147_483_647.0,
        _ => f32::from(i16::MAX),
    }
}

fn chunk_samples(sample_rate: u32, chunk_ms: u64) -> usize {
    let samples = u64::from(sample_rate)
        .saturating_mul(chunk_ms.max(1))
        .saturating_div(1_000)
        .max(1);
    usize::try_from(samples).map_or(usize::MAX, |value| value)
}

fn silent_samples(sample_rate: u32, millis: u64) -> Vec<f32> {
    let samples = u64::from(sample_rate)
        .saturating_mul(millis)
        .saturating_div(1_000);
    vec![0.0; usize::try_from(samples).map_or(usize::MAX, |value| value)]
}

fn duration_ms(samples: usize, sample_rate: u32) -> u64 {
    let samples = u64::try_from(samples).map_or(u64::MAX, |value| value);
    samples.saturating_mul(1_000) / u64::from(sample_rate.max(1))
}

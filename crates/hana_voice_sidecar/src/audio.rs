//! Microphone capture and WAV writing.

use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::SyncSender;
use std::thread;
use std::time::Duration;

use bevy_kana::ToI32;
use cpal::BuildStreamError;
use cpal::Device;
use cpal::SampleFormat;
use cpal::Stream;
use cpal::StreamConfig;
use cpal::SupportedStreamConfig;
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use hound::WavSpec;

const AUDIO_READY_TIMEOUT: Duration = Duration::from_secs(5);
const AUDIO_THREAD_POLL: Duration = Duration::from_millis(50);

/// A running default-input microphone capture thread.
pub struct AudioInput {
    samples: Mutex<Receiver<Vec<f32>>>,
    errors:  Mutex<Receiver<String>>,
    stop:    Sender<()>,
    status:  AudioInputStatus,
}

impl AudioInput {
    /// Opens the default input device and starts streaming mono `f32` samples.
    ///
    /// # Errors
    ///
    /// Returns [`AudioInputError`] when the host has no default input, the
    /// default format is unsupported by this POC, or the stream fails to start.
    pub fn open_default() -> Result<Self, AudioInputError> {
        let (sample_tx, sample_rx) = mpsc::channel();
        let (error_tx, error_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);

        let ready_error_tx = ready_tx.clone();
        thread::spawn(move || {
            let result = run_audio_thread(sample_tx, error_tx.clone(), stop_rx, ready_tx);
            if let Err(error) = result {
                let message = error.to_string();
                let _send_error_result = error_tx.send(message.clone());
                let _send_ready_result = ready_error_tx.send(Err(message));
            }
        });

        let status = match ready_rx.recv_timeout(AUDIO_READY_TIMEOUT) {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => return Err(AudioInputError::Stream(error)),
            Err(error) => return Err(AudioInputError::Stream(error.to_string())),
        };

        Ok(Self {
            samples: Mutex::new(sample_rx),
            errors: Mutex::new(error_rx),
            stop: stop_tx,
            status,
        })
    }

    /// Returns the input-device status captured at stream startup.
    #[must_use]
    pub const fn status(&self) -> &AudioInputStatus { &self.status }

    /// Drains all currently buffered microphone samples.
    #[must_use]
    pub fn drain_samples(&self) -> Vec<f32> {
        let Ok(receiver) = self.samples.lock() else {
            return Vec::new();
        };
        let mut samples = Vec::new();
        while let Ok(mut chunk) = receiver.try_recv() {
            samples.append(&mut chunk);
        }
        samples
    }

    /// Drains all asynchronous stream errors reported by the audio callback.
    #[must_use]
    pub fn drain_errors(&self) -> Vec<String> {
        let Ok(receiver) = self.errors.lock() else {
            return Vec::new();
        };
        let mut errors = Vec::new();
        while let Ok(error) = receiver.try_recv() {
            errors.push(error);
        }
        errors
    }
}

impl Drop for AudioInput {
    fn drop(&mut self) { let _send_result = self.stop.send(()); }
}

/// Immutable description of the input stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioInputStatus {
    /// User-visible device name.
    pub device_name: String,
    /// Stream sample rate in hertz.
    pub sample_rate: u32,
    /// Source channel count before downmixing to mono.
    pub channels:    u16,
}

/// Audio input setup failure.
#[derive(Debug)]
pub enum AudioInputError {
    /// No default input device was reported by the host.
    NoDefaultInput,
    /// The default device could not report a supported config.
    DefaultConfig(String),
    /// The default config uses a sample format this POC does not yet handle.
    UnsupportedFormat(SampleFormat),
    /// Stream construction or startup failed.
    Stream(String),
}

impl Display for AudioInputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDefaultInput => formatter.write_str("no default input device"),
            Self::DefaultConfig(error) => {
                write!(formatter, "default input config failed: {error}")
            },
            Self::UnsupportedFormat(format) => {
                write!(formatter, "unsupported input sample format: {format:?}")
            },
            Self::Stream(error) => write!(formatter, "audio stream failed: {error}"),
        }
    }
}

impl Error for AudioInputError {}

/// Writes a mono 16-bit PCM WAV file.
///
/// # Errors
///
/// Returns any file creation, sample writing, or finalize error from `hound`.
pub fn write_wav(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<(), hound::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in samples {
        writer.write_sample(to_pcm_i16(*sample))?;
    }
    writer.finalize()
}

fn to_pcm_i16(sample: f32) -> i16 {
    let sample = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX))
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
        .to_i32();
    i16::try_from(sample).unwrap_or(if sample < 0 { i16::MIN } else { i16::MAX })
}

fn run_audio_thread(
    sample_tx: Sender<Vec<f32>>,
    error_tx: Sender<String>,
    stop_rx: Receiver<()>,
    ready_tx: SyncSender<Result<AudioInputStatus, String>>,
) -> Result<(), AudioInputError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(AudioInputError::NoDefaultInput)?;
    let device_name = device.description().map_or_else(
        |_| String::from("default input"),
        |description| description.name().to_string(),
    );
    let supported = device
        .default_input_config()
        .map_err(|error| AudioInputError::DefaultConfig(error.to_string()))?;
    let status = AudioInputStatus {
        device_name,
        sample_rate: supported.sample_rate(),
        channels: supported.channels(),
    };
    let stream = build_stream(&device, &supported, sample_tx, error_tx)?;
    stream
        .play()
        .map_err(|error| AudioInputError::Stream(error.to_string()))?;
    let _send_ready_result = ready_tx.send(Ok(status));

    while stop_rx.recv_timeout(AUDIO_THREAD_POLL).is_err() {}
    Ok(())
}

fn build_stream(
    device: &Device,
    supported: &SupportedStreamConfig,
    sample_tx: Sender<Vec<f32>>,
    error_tx: Sender<String>,
) -> Result<Stream, AudioInputError> {
    let config = StreamConfig::from(supported.clone());
    let channels = usize::from(config.channels);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_f32_stream(device, &config, channels, sample_tx, error_tx),
        SampleFormat::I16 => build_i16_stream(device, &config, channels, sample_tx, error_tx),
        SampleFormat::U16 => build_u16_stream(device, &config, channels, sample_tx, error_tx),
        format => return Err(AudioInputError::UnsupportedFormat(format)),
    };
    stream.map_err(|error| AudioInputError::Stream(error.to_string()))
}

fn build_f32_stream(
    device: &Device,
    config: &StreamConfig,
    channels: usize,
    sample_tx: Sender<Vec<f32>>,
    error_tx: Sender<String>,
) -> Result<Stream, BuildStreamError> {
    device.build_input_stream(
        config,
        move |data: &[f32], _| {
            let _send_result = sample_tx.send(downmix_f32(data, channels));
        },
        move |error| {
            let _send_result = error_tx.send(error.to_string());
        },
        None,
    )
}

fn build_i16_stream(
    device: &Device,
    config: &StreamConfig,
    channels: usize,
    sample_tx: Sender<Vec<f32>>,
    error_tx: Sender<String>,
) -> Result<Stream, BuildStreamError> {
    device.build_input_stream(
        config,
        move |data: &[i16], _| {
            let _send_result = sample_tx.send(downmix_i16(data, channels));
        },
        move |error| {
            let _send_result = error_tx.send(error.to_string());
        },
        None,
    )
}

fn build_u16_stream(
    device: &Device,
    config: &StreamConfig,
    channels: usize,
    sample_tx: Sender<Vec<f32>>,
    error_tx: Sender<String>,
) -> Result<Stream, BuildStreamError> {
    device.build_input_stream(
        config,
        move |data: &[u16], _| {
            let _send_result = sample_tx.send(downmix_u16(data, channels));
        },
        move |error| {
            let _send_result = error_tx.send(error.to_string());
        },
        None,
    )
}

fn downmix_f32(data: &[f32], channels: usize) -> Vec<f32> {
    downmix(data, channels, |sample| *sample)
}

fn downmix_i16(data: &[i16], channels: usize) -> Vec<f32> {
    downmix(data, channels, |sample| {
        f32::from(*sample) / f32::from(i16::MAX)
    })
}

fn downmix_u16(data: &[u16], channels: usize) -> Vec<f32> {
    downmix(data, channels, |sample| {
        (f32::from(*sample) / f32::from(u16::MAX) - 0.5) * 2.0
    })
}

fn downmix<T>(data: &[T], channels: usize, convert: impl Fn(&T) -> f32) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    let mut mono = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0.0;
        for sample in frame {
            sum += convert(sample);
        }
        let divisor = u16::try_from(frame.len()).map_or(1.0, f32::from);
        mono.push(sum / divisor);
    }
    mono
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::downmix_i16;
    use super::write_wav;

    #[test]
    fn downmixes_stereo_to_mono() {
        let mixed = downmix_i16(&[i16::MAX, 0, 0, i16::MAX], 2);

        assert_eq!(mixed.len(), 2);
        assert!((mixed[0] - 0.5).abs() < 0.001);
        assert!((mixed[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn write_wav_recreates_missing_parent_directory() {
        let root = std::env::temp_dir()
            .join("hana_voice_sidecar_write_wav_test")
            .join(std::process::id().to_string());
        let path = root.join("audio").join("voice.wav");
        let _cleanup = fs::remove_dir_all(&root);

        let result = write_wav(&path, 48_000, &[0.0, 0.25, -0.25]);
        assert!(result.is_ok(), "WAV write failed: {result:?}");

        assert!(path.exists());
        let _cleanup = fs::remove_dir_all(&root);
    }
}

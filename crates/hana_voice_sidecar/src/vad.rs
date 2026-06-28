//! Voice activity detector adapter.

use std::fmt;
use std::fmt::Formatter;

use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;
use earshot::Detector;

const EARSHOT_SAMPLE_RATE: u32 = 16_000;
const EARSHOT_FRAME_SAMPLES: usize = 256;

/// One VAD frame decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VadFrame {
    /// Number of source samples covered by this frame.
    pub(crate) source_samples: usize,
    /// Voice probability or fallback energy score.
    pub(crate) probability:    f32,
}

/// Streaming VAD engine.
pub(crate) enum VadEngine {
    Earshot(EarshotEngine),
    Energy(EnergyVad),
}

impl fmt::Debug for VadEngine {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Earshot(engine) => formatter.debug_tuple("Earshot").field(engine).finish(),
            Self::Energy(engine) => formatter.debug_tuple("Energy").field(engine).finish(),
        }
    }
}

impl VadEngine {
    /// Creates the best available VAD engine for `sample_rate`.
    pub(crate) fn new(sample_rate: u32) -> Self {
        if sample_rate >= EARSHOT_SAMPLE_RATE {
            Self::Earshot(EarshotEngine::new(sample_rate))
        } else {
            Self::Energy(EnergyVad)
        }
    }

    /// Resets buffered frame state.
    pub(crate) fn reset(&mut self) {
        match self {
            Self::Earshot(engine) => engine.reset(),
            Self::Energy(_engine) => EnergyVad::reset(),
        }
    }

    /// Processes source-rate mono samples and returns available VAD frames.
    pub(crate) fn process(&mut self, samples: &[f32]) -> Vec<VadFrame> {
        match self {
            Self::Earshot(engine) => engine.process(samples),
            Self::Energy(_engine) => EnergyVad::process(samples),
        }
    }
}

/// Earshot-backed streaming VAD.
pub(crate) struct EarshotEngine {
    detector:             Box<Detector>,
    sample_rate:          u32,
    source_frame_samples: usize,
    pending:              Vec<f32>,
}

impl fmt::Debug for EarshotEngine {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EarshotEngine")
            .field("sample_rate", &self.sample_rate)
            .field("source_frame_samples", &self.source_frame_samples)
            .field("pending_samples", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl EarshotEngine {
    fn new(sample_rate: u32) -> Self {
        let source_frame_samples = source_frame_samples(sample_rate);
        Self {
            detector: Detector::default_boxed(),
            sample_rate,
            source_frame_samples,
            pending: Vec::with_capacity(source_frame_samples.saturating_mul(2)),
        }
    }

    fn reset(&mut self) {
        self.detector.reset();
        self.pending.clear();
    }

    fn process(&mut self, samples: &[f32]) -> Vec<VadFrame> {
        self.pending.extend(samples.iter().copied());
        let mut frames = Vec::new();
        while self.pending.len() >= self.source_frame_samples {
            let source: Vec<f32> = self.pending.drain(..self.source_frame_samples).collect();
            let frame = resample_to_earshot_frame(&source);
            frames.push(VadFrame {
                source_samples: self.source_frame_samples,
                probability:    self.detector.predict_f32(&frame),
            });
        }
        frames
    }
}

/// Deterministic low-rate fallback used by tests and unusual devices.
#[derive(Debug)]
pub(crate) struct EnergyVad;

impl EnergyVad {
    const fn reset() {}

    fn process(samples: &[f32]) -> Vec<VadFrame> {
        if samples.is_empty() {
            return Vec::new();
        }
        vec![VadFrame {
            source_samples: samples.len(),
            probability:    rms(samples),
        }]
    }
}

fn source_frame_samples(sample_rate: u32) -> usize {
    let samples = u64::from(sample_rate).saturating_mul(u64::from(EARSHOT_FRAME_SAMPLES.to_u32()))
        / u64::from(EARSHOT_SAMPLE_RATE);
    usize::try_from(samples.max(1)).map_or(usize::MAX, |value| value)
}

fn resample_to_earshot_frame(source: &[f32]) -> [f32; EARSHOT_FRAME_SAMPLES] {
    let mut frame = [0.0; EARSHOT_FRAME_SAMPLES];
    if source.is_empty() {
        return frame;
    }
    if source.len() == 1 {
        frame.fill(source[0].clamp(-1.0, 1.0));
        return frame;
    }
    let last_source_index = (source.len() - 1).to_f32();
    let last_frame_index = (EARSHOT_FRAME_SAMPLES - 1).to_f32();
    for (index, sample) in frame.iter_mut().enumerate() {
        let source_position = index.to_f32() * last_source_index / last_frame_index;
        let lower = source_position.floor().to_usize();
        let upper = lower.saturating_add(1).min(source.len() - 1);
        let fraction = source_position - lower.to_f32();
        *sample = source[lower]
            .mul_add(1.0 - fraction, source[upper] * fraction)
            .clamp(-1.0, 1.0);
    }
    frame
}

fn rms(samples: &[f32]) -> f32 {
    let energy: f32 = samples.iter().map(|sample| sample * sample).sum();
    (energy / samples.len().to_f32()).sqrt()
}

#[cfg(test)]
mod tests {
    use bevy_kana::ToF32;

    use super::EARSHOT_FRAME_SAMPLES;
    use super::VadEngine;
    use super::resample_to_earshot_frame;
    use super::source_frame_samples;

    #[test]
    fn source_frame_matches_common_mac_sample_rate() {
        assert_eq!(source_frame_samples(48_000), 768);
    }

    #[test]
    fn resamples_source_frame_to_earshot_size() {
        let source: Vec<f32> = (0..768).map(|value| value.to_f32() / 768.0).collect();
        let frame = resample_to_earshot_frame(&source);

        assert_eq!(frame.len(), EARSHOT_FRAME_SAMPLES);
        assert!(frame[0] <= frame[1]);
        assert!(frame[254] <= frame[255]);
    }

    #[test]
    fn low_sample_rates_use_energy_fallback() {
        let mut vad = VadEngine::new(1_000);
        let frames = vad.process(&[0.25; 100]);

        assert_eq!(frames.len(), 1);
        assert!(frames[0].probability > 0.24);
    }
}

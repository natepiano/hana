//! Utterance state machine and VAD.

use std::collections::VecDeque;

use bevy_kana::ToF32;

use crate::vad::VadEngine;

/// Voice session configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionConfig {
    /// VAD probability required before a frame can begin speech.
    pub speech_probability_threshold:         f32,
    /// VAD probability required to keep speech active after it begins.
    pub speech_release_probability_threshold: f32,
    /// Sustained speech required before leaving the armed phase.
    pub speech_start_ms:                      u64,
    /// Time low-confidence release frames may hold active speech open.
    pub speech_hangover_ms:                   u64,
    /// Audio retained before detected speech starts.
    pub pre_roll_ms:                          u64,
    /// Silence required before the utterance is committed.
    pub silence_commit_ms:                    u64,
    /// Active audio window age that triggers an STT probe even without silence.
    pub candidate_probe_ms:                   u64,
    /// Additional active audio required before another STT probe.
    pub candidate_probe_interval_ms:          u64,
    /// Hard cap for one utterance.
    pub max_utterance_ms:                     u64,
    /// Minimum speech duration before committing.
    pub min_speech_ms:                        u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            speech_probability_threshold:         0.36,
            speech_release_probability_threshold: 0.24,
            speech_start_ms:                      64,
            speech_hangover_ms:                   160,
            pre_roll_ms:                          650,
            silence_commit_ms:                    850,
            candidate_probe_ms:                   1_800,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     30_000,
            min_speech_ms:                        64,
        }
    }
}

/// Current session phase.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionPhase {
    /// Nothing is armed.
    #[default]
    Idle,
    /// A button press armed the microphone and the session is waiting for
    /// speech to cross the VAD threshold.
    Armed,
    /// Speech is active.
    Listening,
    /// Speech stopped and the silence timer is running.
    Settling,
    /// Audio is committed and transcription is running.
    Transcribing,
    /// The utterance completed.
    Complete,
    /// The utterance failed.
    Error,
}

impl SessionPhase {
    /// User-visible phase label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Armed => "Armed",
            Self::Listening => "Listening",
            Self::Settling => "Settling",
            Self::Transcribing => "Transcribing",
            Self::Complete => "Complete",
            Self::Error => "Error",
        }
    }
}

/// Event emitted by the VAD state machine.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    /// A new session was armed.
    ListenArmed {
        /// Session id.
        session_id: String,
    },
    /// Speech crossed the threshold.
    SpeechStarted {
        /// Session id.
        session_id: String,
    },
    /// Speech fell below the threshold and the silence timer began.
    SpeechSettling {
        /// Session id.
        session_id: String,
    },
    /// Candidate audio window is complete and can be written/transcribed.
    AudioCommitted(CommittedUtterance),
    /// A provisional audio window is ready for STT while capture continues.
    CandidateReady(CommittedUtterance),
    /// Speech ended before the minimum duration.
    SpeechTooShort {
        /// Session id.
        session_id: String,
    },
}

/// Candidate utterance audio.
#[derive(Clone, Debug, PartialEq)]
pub struct CommittedUtterance {
    /// Session id.
    pub session_id:           String,
    /// Mono `f32` samples.
    pub samples:              Vec<f32>,
    /// Sample rate in hertz.
    pub sample_rate:          u32,
    /// Speech duration excluding trailing silence.
    pub speech_duration_ms:   u64,
    /// Silence that caused commit.
    pub silence_ms:           u64,
    /// Full recorded duration including pre-roll and trailing silence.
    pub recorded_duration_ms: u64,
}

/// Read-only UI snapshot of a voice session.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceSessionSnapshot {
    /// Current session phase.
    pub phase:                SessionPhase,
    /// Active session id, if any.
    pub session_id:           Option<String>,
    /// Last RMS level observed.
    pub rms:                  f32,
    /// Current silence window in milliseconds.
    pub silence_ms:           u64,
    /// Speech duration in milliseconds.
    pub speech_ms:            u64,
    /// Full recording duration in milliseconds.
    pub recorded_ms:          u64,
    /// Last VAD voice probability.
    pub vad_probability:      f32,
    /// Current VAD speech-start probability.
    pub vad_gate_probability: f32,
    /// Last committed transcript or status text.
    pub transcript:           Option<String>,
    /// Last error message.
    pub error:                Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VadDecision {
    start_samples:   usize,
    release_samples: usize,
    start:           VadSignal,
    release:         VadSignal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VadSignal {
    Inactive,
    Active,
}

impl VadSignal {
    const fn from_samples(samples: usize) -> Self {
        if samples > 0 {
            Self::Active
        } else {
            Self::Inactive
        }
    }
}

/// Stateful VAD session.
#[derive(Debug)]
pub struct VoiceSession {
    config:               SessionConfig,
    sample_rate:          u32,
    phase:                SessionPhase,
    session_id:           Option<String>,
    pre_roll:             VecDeque<f32>,
    samples:              Vec<f32>,
    silence_samples:      usize,
    speech_samples:       usize,
    speech_start_samples: usize,
    next_probe_samples:   usize,
    candidate_count:      u32,
    last_rms:             f32,
    last_vad_probability: f32,
    vad:                  VadEngine,
    transcript:           Option<String>,
    error:                Option<String>,
}

impl VoiceSession {
    /// Creates a new idle session state machine.
    #[must_use]
    pub fn new(config: SessionConfig, sample_rate: u32) -> Self {
        Self {
            config,
            sample_rate,
            phase: SessionPhase::Idle,
            session_id: None,
            pre_roll: VecDeque::new(),
            samples: Vec::new(),
            silence_samples: 0,
            speech_samples: 0,
            speech_start_samples: 0,
            next_probe_samples: 0,
            candidate_count: 0,
            last_rms: 0.0,
            last_vad_probability: 0.0,
            vad: VadEngine::new(sample_rate),
            transcript: None,
            error: None,
        }
    }

    /// Current phase.
    #[must_use]
    pub const fn phase(&self) -> SessionPhase { self.phase }

    /// Arms a new listening session, interrupting any previous one.
    #[must_use]
    pub fn arm(&mut self, session_id: impl Into<String>) -> SessionEvent {
        let session_id = session_id.into();
        self.phase = SessionPhase::Armed;
        self.session_id = Some(session_id.clone());
        self.clear_audio_window();
        self.transcript = None;
        self.error = None;
        SessionEvent::ListenArmed { session_id }
    }

    /// Stops any active listening window and returns to idle.
    pub fn stop(&mut self) {
        self.phase = SessionPhase::Idle;
        self.session_id = None;
        self.clear_audio_window();
        self.vad.reset();
    }

    fn clear_audio_window(&mut self) {
        self.pre_roll.clear();
        self.samples.clear();
        self.silence_samples = 0;
        self.speech_samples = 0;
        self.speech_start_samples = 0;
        self.next_probe_samples = 0;
        self.candidate_count = 0;
        self.last_rms = 0.0;
        self.last_vad_probability = 0.0;
    }

    /// Processes new mono samples and returns any resulting state events.
    #[must_use]
    pub fn process_samples(&mut self, samples: &[f32]) -> Vec<SessionEvent> {
        if samples.is_empty() {
            return Vec::new();
        }
        self.last_rms = rms(samples);
        if !matches!(
            self.phase,
            SessionPhase::Armed | SessionPhase::Listening | SessionPhase::Settling
        ) {
            return Vec::new();
        }

        let mut events = Vec::new();
        let Some(decision) = self.vad_decision(samples) else {
            self.record_without_vad_frames(samples);
            return events;
        };
        self.apply_vad_decision(samples, decision, &mut events);
        self.commit_ready_audio(&mut events);
        events
    }

    fn vad_decision(&mut self, samples: &[f32]) -> Option<VadDecision> {
        let frames = self.vad.process(samples);
        if frames.is_empty() {
            return None;
        }
        if let Some(probability) = frames
            .iter()
            .map(|frame| frame.probability)
            .max_by(f32::total_cmp)
        {
            self.last_vad_probability = probability;
        }
        let start_samples = frames
            .iter()
            .filter(|frame| frame.probability >= self.config.speech_probability_threshold)
            .map(|frame| frame.source_samples)
            .sum();
        let release_samples = frames
            .iter()
            .filter(|frame| frame.probability >= self.config.speech_release_probability_threshold)
            .map(|frame| frame.source_samples)
            .sum();
        Some(VadDecision {
            start_samples,
            release_samples,
            start: VadSignal::from_samples(start_samples),
            release: VadSignal::from_samples(release_samples),
        })
    }

    fn record_without_vad_frames(&mut self, samples: &[f32]) {
        match self.phase {
            SessionPhase::Armed => self.push_pre_roll(samples),
            SessionPhase::Listening | SessionPhase::Settling => {
                self.samples.extend_from_slice(samples);
            },
            _ => {},
        }
    }

    fn apply_vad_decision(
        &mut self,
        samples: &[f32],
        decision: VadDecision,
        events: &mut Vec<SessionEvent>,
    ) {
        match (self.phase, decision.start, decision.release) {
            (SessionPhase::Armed, VadSignal::Inactive, _) => {
                self.speech_start_samples = 0;
                self.push_pre_roll(samples);
            },
            (SessionPhase::Armed, VadSignal::Active, _) => {
                self.speech_start_samples = self
                    .speech_start_samples
                    .saturating_add(decision.start_samples);
                self.push_pre_roll(samples);
                if self.speech_start_ms() >= self.config.speech_start_ms {
                    self.phase = SessionPhase::Listening;
                    self.samples.extend(self.pre_roll.drain(..));
                    self.speech_samples = self.speech_start_samples;
                    self.speech_start_samples = 0;
                    if let Some(session_id) = self.session_id.clone() {
                        events.push(SessionEvent::SpeechStarted { session_id });
                    }
                }
            },
            (SessionPhase::Listening, VadSignal::Active, _) => {
                self.samples.extend_from_slice(samples);
                self.speech_samples = self.speech_samples.saturating_add(decision.release_samples);
                self.silence_samples = 0;
            },
            (SessionPhase::Listening, VadSignal::Inactive, _) => {
                self.samples.extend_from_slice(samples);
                self.speech_samples = self.speech_samples.saturating_add(decision.release_samples);
                self.silence_samples = self.silence_samples.saturating_add(samples.len());
                if self.silence_ms() >= self.config.speech_hangover_ms {
                    self.phase = SessionPhase::Settling;
                    if let Some(session_id) = self.session_id.clone() {
                        events.push(SessionEvent::SpeechSettling { session_id });
                    }
                }
            },
            (SessionPhase::Settling, VadSignal::Active, _) => {
                self.phase = SessionPhase::Listening;
                self.samples.extend_from_slice(samples);
                self.speech_samples = self.speech_samples.saturating_add(decision.release_samples);
                self.silence_samples = 0;
                if let Some(session_id) = self.session_id.clone() {
                    events.push(SessionEvent::SpeechStarted { session_id });
                }
            },
            (SessionPhase::Settling, VadSignal::Inactive, _) => {
                self.samples.extend_from_slice(samples);
                self.speech_samples = self.speech_samples.saturating_add(decision.release_samples);
                self.silence_samples = self.silence_samples.saturating_add(samples.len());
            },
            _ => {},
        }
    }

    fn commit_ready_audio(&mut self, events: &mut Vec<SessionEvent>) {
        if self.phase == SessionPhase::Settling
            && self.silence_ms() >= self.config.silence_commit_ms
        {
            events.push(self.commit());
            return;
        }

        if matches!(self.phase, SessionPhase::Listening | SessionPhase::Settling)
            && self.config.candidate_probe_ms > 0
            && self.audio_window_ms() >= self.config.candidate_probe_ms
        {
            self.push_candidate_probe(events);
        }

        if matches!(self.phase, SessionPhase::Listening | SessionPhase::Settling)
            && self.audio_window_ms() >= self.config.max_utterance_ms
        {
            self.phase = SessionPhase::Settling;
            self.silence_samples = self.ms_to_samples(self.config.silence_commit_ms);
            events.push(self.commit());
        }
    }

    fn push_candidate_probe(&mut self, events: &mut Vec<SessionEvent>) {
        if self.next_probe_samples == 0 {
            self.next_probe_samples = self.ms_to_samples(self.config.candidate_probe_ms);
        }
        if self.samples.len() < self.next_probe_samples {
            return;
        }
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        self.candidate_count = self.candidate_count.saturating_add(1);
        let interval_ms = self.config.candidate_probe_interval_ms.max(1);
        self.next_probe_samples = self
            .next_probe_samples
            .saturating_add(self.ms_to_samples(interval_ms));
        events.push(SessionEvent::CandidateReady(CommittedUtterance {
            session_id:           format!("{session_id}-probe-{}", self.candidate_count),
            samples:              self.samples.clone(),
            sample_rate:          self.sample_rate,
            speech_duration_ms:   self.speech_ms(),
            silence_ms:           self.silence_ms(),
            recorded_duration_ms: self.audio_window_ms(),
        }));
    }

    /// Marks the session as waiting on transcription.
    pub const fn mark_transcribing(&mut self) { self.phase = SessionPhase::Transcribing; }

    /// Marks the session as complete with transcript or status text.
    pub fn mark_complete(&mut self, transcript: impl Into<String>) {
        self.phase = SessionPhase::Complete;
        self.transcript = Some(transcript.into());
    }

    /// Marks the session as failed.
    pub fn mark_error(&mut self, error: impl Into<String>) {
        self.phase = SessionPhase::Error;
        self.error = Some(error.into());
    }

    /// Returns a UI snapshot.
    #[must_use]
    pub fn snapshot(&self) -> VoiceSessionSnapshot {
        VoiceSessionSnapshot {
            phase:                self.phase,
            session_id:           self.session_id.clone(),
            rms:                  self.last_rms,
            silence_ms:           self.silence_ms(),
            speech_ms:            self.speech_ms(),
            recorded_ms:          self.audio_window_ms(),
            vad_probability:      self.last_vad_probability,
            vad_gate_probability: self.config.speech_probability_threshold,
            transcript:           self.transcript.clone(),
            error:                self.error.clone(),
        }
    }

    fn push_pre_roll(&mut self, samples: &[f32]) {
        let max = self.ms_to_samples(self.config.pre_roll_ms.max(self.config.speech_start_ms));
        self.pre_roll.extend(samples.iter().copied());
        while self.pre_roll.len() > max {
            let _discarded = self.pre_roll.pop_front();
        }
    }

    fn commit(&mut self) -> SessionEvent {
        let Some(session_id) = self.session_id.clone() else {
            self.phase = SessionPhase::Idle;
            return SessionEvent::SpeechTooShort {
                session_id: String::new(),
            };
        };
        if self.speech_ms() < self.config.min_speech_ms {
            self.phase = SessionPhase::Idle;
            return SessionEvent::SpeechTooShort { session_id };
        }
        self.phase = SessionPhase::Complete;
        SessionEvent::AudioCommitted(CommittedUtterance {
            session_id,
            samples: self.samples.clone(),
            sample_rate: self.sample_rate,
            speech_duration_ms: self.speech_ms(),
            silence_ms: self.silence_ms(),
            recorded_duration_ms: self.audio_window_ms(),
        })
    }

    fn ms_to_samples(&self, millis: u64) -> usize {
        let samples = millis.saturating_mul(u64::from(self.sample_rate)) / 1_000;
        usize::try_from(samples).map_or(usize::MAX, |value| value)
    }

    fn silence_ms(&self) -> u64 { samples_to_ms(self.silence_samples, self.sample_rate) }
    fn speech_ms(&self) -> u64 { samples_to_ms(self.speech_samples, self.sample_rate) }
    fn speech_start_ms(&self) -> u64 { samples_to_ms(self.speech_start_samples, self.sample_rate) }
    fn audio_window_ms(&self) -> u64 { samples_to_ms(self.samples.len(), self.sample_rate) }
}

fn samples_to_ms(samples: usize, sample_rate: u32) -> u64 {
    let samples = u64::try_from(samples).map_or(u64::MAX, |value| value);
    samples.saturating_mul(1_000) / u64::from(sample_rate.max(1))
}

fn rms(samples: &[f32]) -> f32 {
    let energy: f32 = samples.iter().map(|sample| sample * sample).sum();
    (energy / samples.len().to_f32()).sqrt()
}

#[cfg(test)]
mod tests {
    use super::SessionConfig;
    use super::SessionEvent;
    use super::SessionPhase;
    use super::VoiceSession;

    #[test]
    fn commits_after_speech_and_silence() {
        let config = SessionConfig {
            speech_probability_threshold:         0.1,
            speech_release_probability_threshold: 0.05,
            speech_start_ms:                      50,
            speech_hangover_ms:                   20,
            pre_roll_ms:                          100,
            silence_commit_ms:                    100,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        50,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-1");

        assert!(session.process_samples(&[0.0; 50]).is_empty());
        let events = session.process_samples(&[0.5; 80]);
        assert!(matches!(
            events.first(),
            Some(SessionEvent::SpeechStarted { .. })
        ));
        let events = session.process_samples(&[0.0; 100]);

        assert_eq!(session.phase(), SessionPhase::Complete);
        assert!(matches!(
            events.last(),
            Some(SessionEvent::AudioCommitted(committed)) if committed.speech_duration_ms == 80
        ));
    }

    #[test]
    fn returns_to_listening_when_speech_resumes_before_commit() {
        let config = SessionConfig {
            speech_probability_threshold:         0.1,
            speech_release_probability_threshold: 0.05,
            speech_start_ms:                      50,
            speech_hangover_ms:                   50,
            pre_roll_ms:                          100,
            silence_commit_ms:                    200,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        50,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-2");

        let _events = session.process_samples(&[0.5; 80]);
        let _events = session.process_samples(&[0.0; 100]);
        let _events = session.process_samples(&[0.5; 50]);

        assert_eq!(session.phase(), SessionPhase::Listening);
    }

    #[test]
    fn ignores_background_noise_below_vad_gate() {
        let config = SessionConfig {
            speech_probability_threshold:         0.45,
            speech_release_probability_threshold: 0.25,
            speech_start_ms:                      64,
            speech_hangover_ms:                   160,
            pre_roll_ms:                          400,
            silence_commit_ms:                    900,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        64,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-noise");

        for _step in 0..50 {
            assert!(session.process_samples(&[0.12; 20]).is_empty());
        }

        let snapshot = session.snapshot();
        assert_eq!(session.phase(), SessionPhase::Armed);
        assert!(snapshot.vad_probability < snapshot.vad_gate_probability);
    }

    #[test]
    fn requires_sustained_audio_before_speech_starts() {
        let config = SessionConfig {
            speech_probability_threshold:         0.1,
            speech_release_probability_threshold: 0.05,
            speech_start_ms:                      200,
            speech_hangover_ms:                   20,
            pre_roll_ms:                          200,
            silence_commit_ms:                    100,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        50,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-start");

        assert!(session.process_samples(&[0.5; 100]).is_empty());
        assert_eq!(session.phase(), SessionPhase::Armed);
        let events = session.process_samples(&[0.5; 100]);

        assert!(matches!(
            events.first(),
            Some(SessionEvent::SpeechStarted { .. })
        ));
        assert_eq!(session.phase(), SessionPhase::Listening);
    }

    #[test]
    fn captures_short_command() {
        let config = SessionConfig {
            speech_probability_threshold:         0.02,
            speech_release_probability_threshold: 0.01,
            speech_start_ms:                      120,
            speech_hangover_ms:                   20,
            pre_roll_ms:                          400,
            silence_commit_ms:                    100,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        120,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-short");

        for _step in 0..20 {
            assert!(session.process_samples(&[0.006; 20]).is_empty());
        }
        assert!(session.process_samples(&[0.030; 60]).is_empty());
        let events = session.process_samples(&[0.030; 60]);
        assert!(matches!(
            events.first(),
            Some(SessionEvent::SpeechStarted { .. })
        ));
        let events = session.process_samples(&[0.0; 100]);

        assert!(matches!(
            events.last(),
            Some(SessionEvent::AudioCommitted(committed)) if committed.speech_duration_ms == 120
        ));
    }

    #[test]
    fn max_utterance_commits_even_if_release_gate_stays_active() {
        let config = SessionConfig {
            speech_probability_threshold:         0.1,
            speech_release_probability_threshold: 0.01,
            speech_start_ms:                      50,
            speech_hangover_ms:                   100,
            pre_roll_ms:                          100,
            silence_commit_ms:                    100,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     300,
            min_speech_ms:                        50,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-cap");

        assert!(
            session
                .process_samples(&[0.5; 80])
                .iter()
                .any(|event| { matches!(event, SessionEvent::SpeechStarted { .. }) })
        );
        let events = session.process_samples(&[0.02; 300]);

        assert_eq!(session.phase(), SessionPhase::Complete);
        assert!(matches!(
            events.last(),
            Some(SessionEvent::AudioCommitted(committed)) if committed.recorded_duration_ms >= 300
        ));
    }

    #[test]
    fn one_vad_frame_keeps_active_speech_alive() {
        let config = SessionConfig {
            speech_probability_threshold:         0.1,
            speech_release_probability_threshold: 0.05,
            speech_start_ms:                      16,
            speech_hangover_ms:                   100,
            pre_roll_ms:                          100,
            silence_commit_ms:                    100,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        16,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-frame");

        assert!(
            session
                .process_samples(&[0.5; 16])
                .iter()
                .any(|event| { matches!(event, SessionEvent::SpeechStarted { .. }) })
        );
        assert!(session.process_samples(&[0.5; 16]).is_empty());

        assert_eq!(session.phase(), SessionPhase::Listening);
    }

    #[test]
    fn release_gate_alone_cannot_hold_listening_forever() {
        let config = SessionConfig {
            speech_probability_threshold:         0.5,
            speech_release_probability_threshold: 0.1,
            speech_start_ms:                      50,
            speech_hangover_ms:                   50,
            pre_roll_ms:                          100,
            silence_commit_ms:                    100,
            candidate_probe_ms:                   5_000,
            candidate_probe_interval_ms:          1_000,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        50,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-release");

        assert!(
            session
                .process_samples(&[0.6; 60])
                .iter()
                .any(|event| { matches!(event, SessionEvent::SpeechStarted { .. }) })
        );
        assert!(session.process_samples(&[0.2; 40]).is_empty());
        let events = session.process_samples(&[0.2; 10]);

        assert_eq!(session.phase(), SessionPhase::Settling);
        assert!(
            events
                .iter()
                .any(|event| { matches!(event, SessionEvent::SpeechSettling { .. }) })
        );

        let events = session.process_samples(&[0.2; 50]);

        assert!(matches!(
            events.last(),
            Some(SessionEvent::AudioCommitted(committed)) if committed.recorded_duration_ms >= 160
        ));
    }

    #[test]
    fn emits_candidate_probe_while_background_keeps_listening_active() {
        let config = SessionConfig {
            speech_probability_threshold:         0.5,
            speech_release_probability_threshold: 0.1,
            speech_start_ms:                      50,
            speech_hangover_ms:                   100,
            pre_roll_ms:                          100,
            silence_commit_ms:                    100,
            candidate_probe_ms:                   150,
            candidate_probe_interval_ms:          100,
            max_utterance_ms:                     5_000,
            min_speech_ms:                        50,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-probe");

        let _events = session.process_samples(&[0.6; 80]);
        let events = session.process_samples(&[0.6; 100]);

        assert_eq!(session.phase(), SessionPhase::Listening);
        assert!(matches!(
            events.last(),
            Some(SessionEvent::CandidateReady(committed))
                if committed.session_id == "session-probe-probe-1"
        ));
    }
}

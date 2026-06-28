//! Utterance state machine and VAD.

use std::collections::VecDeque;

use bevy_kana::ToF32;

const NOISE_FLOOR_ALPHA: f32 = 0.05;
const NOISE_FLOOR_START_MULTIPLIER: f32 = 2.6;
const NOISE_FLOOR_RELEASE_MULTIPLIER: f32 = 1.7;
const RELEASE_THRESHOLD_RATIO: f32 = 0.55;

/// Voice session configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionConfig {
    /// Absolute RMS floor required before a chunk can begin speech.
    pub speech_rms_threshold: f32,
    /// Sustained speech required before leaving the armed phase.
    pub speech_start_ms:      u64,
    /// Audio retained before detected speech starts.
    pub pre_roll_ms:          u64,
    /// Silence required before the utterance is committed.
    pub silence_commit_ms:    u64,
    /// Hard cap for one utterance.
    pub max_utterance_ms:     u64,
    /// Minimum speech duration before committing.
    pub min_speech_ms:        u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            speech_rms_threshold: 0.015,
            speech_start_ms:      120,
            pre_roll_ms:          400,
            silence_commit_ms:    900,
            max_utterance_ms:     30_000,
            min_speech_ms:        120,
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
    pub phase:       SessionPhase,
    /// Active session id, if any.
    pub session_id:  Option<String>,
    /// Last RMS level observed.
    pub rms:         f32,
    /// Current silence window in milliseconds.
    pub silence_ms:  u64,
    /// Speech duration in milliseconds.
    pub speech_ms:   u64,
    /// Full recording duration in milliseconds.
    pub recorded_ms: u64,
    /// Estimated ambient noise floor.
    pub noise_rms:   f32,
    /// Current speech-start gate.
    pub gate_rms:    f32,
    /// Last committed transcript or status text.
    pub transcript:  Option<String>,
    /// Last error message.
    pub error:       Option<String>,
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
    recorded_samples:     usize,
    last_rms:             f32,
    noise_floor_rms:      f32,
    transcript:           Option<String>,
    error:                Option<String>,
}

impl VoiceSession {
    /// Creates a new idle session state machine.
    #[must_use]
    pub const fn new(config: SessionConfig, sample_rate: u32) -> Self {
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
            recorded_samples: 0,
            last_rms: 0.0,
            noise_floor_rms: 0.0,
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
    }

    fn clear_audio_window(&mut self) {
        self.pre_roll.clear();
        self.samples.clear();
        self.silence_samples = 0;
        self.speech_samples = 0;
        self.speech_start_samples = 0;
        self.recorded_samples = 0;
        self.last_rms = 0.0;
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
        self.recorded_samples = self.recorded_samples.saturating_add(samples.len());
        let start_gate = self.speech_start_gate();
        let release_gate = self.speech_release_gate();
        let starts_speech = self.last_rms >= start_gate;
        let continues_speech = self.last_rms >= release_gate;

        match (self.phase, starts_speech, continues_speech) {
            (SessionPhase::Armed, false, _) => {
                self.speech_start_samples = 0;
                self.push_pre_roll(samples);
                self.update_noise_floor();
            },
            (SessionPhase::Armed, true, _) => {
                self.speech_start_samples = self.speech_start_samples.saturating_add(samples.len());
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
            (SessionPhase::Listening, _, true) => {
                self.samples.extend_from_slice(samples);
                self.speech_samples = self.speech_samples.saturating_add(samples.len());
                self.silence_samples = 0;
            },
            (SessionPhase::Listening, _, false) => {
                self.phase = SessionPhase::Settling;
                self.samples.extend_from_slice(samples);
                self.silence_samples = self.silence_samples.saturating_add(samples.len());
                self.update_noise_floor();
                if let Some(session_id) = self.session_id.clone() {
                    events.push(SessionEvent::SpeechSettling { session_id });
                }
            },
            (SessionPhase::Settling, _, true) => {
                self.phase = SessionPhase::Listening;
                self.samples.extend_from_slice(samples);
                self.speech_samples = self.speech_samples.saturating_add(samples.len());
                self.silence_samples = 0;
            },
            (SessionPhase::Settling, false, _) => {
                self.samples.extend_from_slice(samples);
                self.silence_samples = self.silence_samples.saturating_add(samples.len());
                self.update_noise_floor();
            },
            _ => {},
        }

        if matches!(self.phase, SessionPhase::Listening | SessionPhase::Settling)
            && self.recorded_ms() >= self.config.max_utterance_ms
        {
            self.silence_samples = self.ms_to_samples(self.config.silence_commit_ms);
        }

        if self.phase == SessionPhase::Settling
            && self.silence_ms() >= self.config.silence_commit_ms
        {
            events.push(self.commit());
        }

        events
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
            phase:       self.phase,
            session_id:  self.session_id.clone(),
            rms:         self.last_rms,
            silence_ms:  self.silence_ms(),
            speech_ms:   self.speech_ms(),
            recorded_ms: self.recorded_ms(),
            noise_rms:   self.noise_floor_rms,
            gate_rms:    self.speech_start_gate(),
            transcript:  self.transcript.clone(),
            error:       self.error.clone(),
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
            recorded_duration_ms: self.recorded_ms(),
        })
    }

    fn ms_to_samples(&self, millis: u64) -> usize {
        let samples = millis.saturating_mul(u64::from(self.sample_rate)) / 1_000;
        usize::try_from(samples).map_or(usize::MAX, |value| value)
    }

    fn silence_ms(&self) -> u64 { samples_to_ms(self.silence_samples, self.sample_rate) }
    fn speech_ms(&self) -> u64 { samples_to_ms(self.speech_samples, self.sample_rate) }
    fn speech_start_ms(&self) -> u64 { samples_to_ms(self.speech_start_samples, self.sample_rate) }
    fn recorded_ms(&self) -> u64 { samples_to_ms(self.recorded_samples, self.sample_rate) }

    const fn speech_start_gate(&self) -> f32 {
        self.config.speech_rms_threshold.max(
            self.noise_floor_rms
                .mul_add(NOISE_FLOOR_START_MULTIPLIER, 0.0),
        )
    }

    fn speech_release_gate(&self) -> f32 {
        let absolute = self.config.speech_rms_threshold * RELEASE_THRESHOLD_RATIO;
        let adaptive = self.noise_floor_rms * NOISE_FLOOR_RELEASE_MULTIPLIER;
        absolute.max(adaptive)
    }

    fn update_noise_floor(&mut self) {
        if self.last_rms <= 0.0 {
            return;
        }
        if self.noise_floor_rms <= 0.0 {
            self.noise_floor_rms = self.last_rms;
        } else {
            self.noise_floor_rms = self
                .noise_floor_rms
                .mul_add(1.0 - NOISE_FLOOR_ALPHA, self.last_rms * NOISE_FLOOR_ALPHA);
        }
    }
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
            speech_rms_threshold: 0.1,
            speech_start_ms:      50,
            pre_roll_ms:          100,
            silence_commit_ms:    100,
            max_utterance_ms:     5_000,
            min_speech_ms:        50,
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
            speech_rms_threshold: 0.1,
            speech_start_ms:      50,
            pre_roll_ms:          100,
            silence_commit_ms:    200,
            max_utterance_ms:     5_000,
            min_speech_ms:        50,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-2");

        let _events = session.process_samples(&[0.5; 80]);
        let _events = session.process_samples(&[0.0; 100]);
        let _events = session.process_samples(&[0.5; 50]);

        assert_eq!(session.phase(), SessionPhase::Listening);
    }

    #[test]
    fn ignores_background_noise_near_the_old_gate() {
        let config = SessionConfig {
            speech_rms_threshold: 0.020,
            speech_start_ms:      220,
            pre_roll_ms:          400,
            silence_commit_ms:    900,
            max_utterance_ms:     5_000,
            min_speech_ms:        350,
        };
        let mut session = VoiceSession::new(config, 1_000);
        let _event = session.arm("session-noise");

        for _step in 0..50 {
            assert!(session.process_samples(&[0.012; 20]).is_empty());
        }

        let snapshot = session.snapshot();
        assert_eq!(session.phase(), SessionPhase::Armed);
        assert!(snapshot.noise_rms >= 0.011);
        assert!(snapshot.gate_rms >= 0.030);
    }

    #[test]
    fn requires_sustained_audio_before_speech_starts() {
        let config = SessionConfig {
            speech_rms_threshold: 0.1,
            speech_start_ms:      200,
            pre_roll_ms:          200,
            silence_commit_ms:    100,
            max_utterance_ms:     5_000,
            min_speech_ms:        50,
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
    fn captures_short_command_after_noise_floor_calibration() {
        let config = SessionConfig {
            speech_rms_threshold: 0.015,
            speech_start_ms:      120,
            pre_roll_ms:          400,
            silence_commit_ms:    100,
            max_utterance_ms:     5_000,
            min_speech_ms:        120,
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
}

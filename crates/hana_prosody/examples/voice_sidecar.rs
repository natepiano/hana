//! Bevy feedback UI for the Hana voice sidecar POC.
//!
//! Press space to toggle continuous transcription. While enabled, the sidecar
//! probes active audio windows, lets Apple Speech validate them, and commits the
//! best stable transcript JSONL for the agent.

use std::collections::VecDeque;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelBuildError;
use bevy_diegetic::PanelFieldId;
use bevy_diegetic::PanelText;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlign;
use bevy_diegetic::TextStyle;
use bevy_diegetic::TextWrap;
use bevy_diegetic::Unit;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::CubeFacePanelStyle;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::Face;
use fairy_dust::TitleBar;
use fairy_dust::cube_face_panel_material;
use fairy_dust::cube_face_transform;
use fairy_dust::screen_panel_frame;
use fairy_dust::screen_panel_material;
use hana_prosody::AudioInput;
use hana_prosody::CommittedUtterance;
use hana_prosody::PendingTranscription;
use hana_prosody::RuntimeEvent;
use hana_prosody::RuntimeLog;
use hana_prosody::RuntimePaths;
use hana_prosody::SessionConfig;
use hana_prosody::SessionEvent;
use hana_prosody::TranscriptionOutcome;
use hana_prosody::VoiceSession;
use hana_prosody::now_unix_millis;
use hana_prosody::spawn_transcription;
use hana_prosody::write_wav;

const TITLE: &str = "Hana Prosody";
const SPACE_CONTROL: &str = "Space Toggle";
const PROMPT_TEXT: &str = "Tell me how you want to look";
const MIC_READY: &str = "Mic ready";
const CUBE_SIZE: f32 = 1.0;
const HOME_PITCH: f32 = 0.45;
const HOME_YAW: f32 = 0.25;
const HOME_MARGIN: f32 = 0.62;

const FACE_PROMPT_NAME: &str = "voice prompt face";
const STATUS_PANEL_WIDTH: f32 = 460.0;
const STATUS_PANEL_GAP: f32 = 8.0;
const STATUS_ROW_GAP: f32 = 10.0;
const STATUS_LABEL_WIDTH: f32 = 116.0;
const STATUS_DIVIDER_HEIGHT: f32 = 1.5;
const MAX_QUEUED_TRANSCRIPTIONS: usize = 3;
const TENTATIVE_STABILITY_MS: u64 = 1_600;
const COMPLETED_ROOTS_MAX: usize = 16;

const TEXT_COLOR: Color = Color::srgb(0.92, 0.94, 0.98);
const MUTED_COLOR: Color = Color::srgb(0.68, 0.72, 0.78);
const TRANSCRIPT_COLOR: Color = Color::srgb(0.88, 0.86, 0.72);
const ACCENT_COLOR: Color = Color::srgb(0.18, 0.70, 0.92);
const FACE_PROMPT_COLOR: Color = Color::linear_rgb(3.6, 5.4, 9.0);

const FIELD_LOOP: &str = "loop";
const FIELD_CAPTURE: &str = "capture";
const FIELD_EVENT: &str = "event";
const FIELD_TRANSCRIPT: &str = "transcript";
const FIELD_MIC: &str = "mic";
const FIELD_GATE: &str = "gate";

fn main() {
    let runtime = SidecarRuntime::new(RuntimePaths::from_env_or_default());

    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_hdr()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
        .size(CUBE_SIZE)
        .color(Color::srgb(0.42, 0.50, 0.78))
        .transform(Transform::from_translation(
            fairy_dust::example_cube_on_ground(0.1),
        ))
        .insert((CameraHomeTarget, VoiceCube))
        .with_orbit_cam_preset_bundle(
            |_| {},
            OrbitCamPreset::blender_like(),
            Tonemapping::AcesFitted,
        )
        .with_bloom()
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title(TITLE)
                .with_anchor(Anchor::TopLeft)
                .control(SPACE_CONTROL),
        )
        .insert_resource(runtime)
        .add_systems(Startup, spawn_status_panel)
        .add_systems(PostStartup, spawn_cube_prompt_faces)
        .add_systems(
            Update,
            (
                drain_audio,
                poll_transcription,
                promote_tentative_transcript,
                refresh_feedback,
            )
                .chain(),
        )
        .with_shortcut(KeyCode::Space, toggle_listening_loop)
        .run();
}

#[derive(Resource)]
struct SidecarRuntime {
    audio:           Result<AudioInput, String>,
    log:             Result<RuntimeLog, String>,
    session:         VoiceSession,
    pending:         Vec<PendingTranscriptionJob>,
    queued:          VecDeque<QueuedTranscription>,
    tentative:       Option<TentativeTranscript>,
    completed_roots: VecDeque<String>,
    loop_state:      ListeningLoop,
    last_event:      String,
    last_text:       Option<String>,
    last_status:     String,
}

struct QueuedTranscription {
    session_id:      String,
    root_session_id: String,
    audio_path:      PathBuf,
    purpose:         TranscriptionPurpose,
}

struct PendingTranscriptionJob {
    transcription:   PendingTranscription,
    root_session_id: String,
    purpose:         TranscriptionPurpose,
}

struct TentativeTranscript {
    root_session_id: String,
    text:            String,
    backend:         String,
    audio_path:      String,
    deadline_ms:     u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TranscriptionPurpose {
    Probe,
    Final,
}

impl SidecarRuntime {
    fn new(paths: RuntimePaths) -> Self {
        let audio = AudioInput::open_default().map_err(|error| error.to_string());
        let sample_rate = audio
            .as_ref()
            .map_or(48_000, |input| input.status().sample_rate);
        let log = RuntimeLog::new(paths).map_err(|error| error.to_string());
        let last_status = match &audio {
            Ok(_input) => String::from(MIC_READY),
            Err(error) => format!("Mic unavailable: {error}"),
        };
        Self {
            audio,
            log,
            session: VoiceSession::new(SessionConfig::default(), sample_rate),
            pending: Vec::new(),
            queued: VecDeque::new(),
            tentative: None,
            completed_roots: VecDeque::new(),
            loop_state: ListeningLoop::Off,
            last_event: String::from("Press space to start"),
            last_text: None,
            last_status,
        }
    }

    fn next_seq(&mut self) -> u64 { self.log.as_mut().map_or(0, RuntimeLog::next_seq) }

    fn append_inbox(&mut self, event: RuntimeEvent) -> InboxAppend {
        if let Ok(log) = &self.log
            && let Err(error) = log.append_inbox(&event)
        {
            self.last_status = format!("Inbox write failed: {error}");
            return InboxAppend::Skipped;
        }
        if self.log.is_ok() {
            InboxAppend::Written
        } else {
            InboxAppend::Skipped
        }
    }

    fn paths(&self) -> Option<&RuntimePaths> { self.log.as_ref().ok().map(RuntimeLog::paths) }

    fn transcription_work_count(&self) -> usize { self.pending.len() + self.queued.len() }

    fn has_transcription_work(&self) -> bool { self.transcription_work_count() > 0 }

    fn is_completed_root(&self, root_session_id: &str) -> bool {
        self.completed_roots
            .iter()
            .any(|completed| completed == root_session_id)
    }

    fn mark_root_completed(&mut self, root_session_id: String) {
        if self.is_completed_root(&root_session_id) {
            return;
        }
        self.completed_roots.push_back(root_session_id);
        while self.completed_roots.len() > COMPLETED_ROOTS_MAX {
            let _old = self.completed_roots.pop_front();
        }
    }

    fn clear_error_text(&mut self) {
        if self
            .last_text
            .as_deref()
            .is_some_and(|text| text.starts_with("Error:"))
        {
            self.last_text = None;
        }
    }

    fn clear_transcription_status(&mut self) {
        if self.last_status.starts_with("Transcription failed:") {
            self.last_status = String::from(MIC_READY);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ListeningLoop {
    Off,
    On,
}

impl ListeningLoop {
    const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InboxAppend {
    Written,
    Skipped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateAudioWrite {
    Written,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TentativePromotion {
    Applied,
    Skipped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusValueRole {
    Standard,
    Transcript,
}

#[derive(Component)]
struct VoiceCube;

#[derive(Component)]
struct VoiceStatusPanel;

#[derive(Component)]
struct VoicePromptFace;

fn spawn_status_panel(mut commands: Commands) {
    match status_panel() {
        Ok(panel) => {
            commands.spawn((VoiceStatusPanel, panel, Transform::default()));
        },
        Err(error) => {
            error!("voice_sidecar: failed to build status panel: {error}");
        },
    }
}

fn spawn_cube_prompt_faces(mut commands: Commands, cubes: Query<Entity, With<VoiceCube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };
    commands.entity(cube).with_children(|parent| {
        for face in [
            Face::Front,
            Face::Back,
            Face::Left,
            Face::Right,
            Face::Top,
            Face::Bottom,
        ] {
            match prompt_face_panel() {
                Ok(panel) => {
                    parent.spawn((
                        Name::new(FACE_PROMPT_NAME),
                        VoicePromptFace,
                        panel,
                        cube_face_transform(face, CUBE_SIZE),
                    ));
                },
                Err(error) => {
                    error!("voice_sidecar: failed to build prompt face panel: {error}");
                },
            }
        }
    });
}

fn toggle_listening_loop(mut runtime: ResMut<SidecarRuntime>) {
    if runtime.loop_state == ListeningLoop::On {
        runtime.loop_state = ListeningLoop::Off;
        runtime.session.stop();
        runtime.last_event = if runtime.has_transcription_work() {
            format!(
                "Stopped; finishing {} transcript(s)",
                runtime.transcription_work_count()
            )
        } else {
            String::from("Stopped")
        };
        return;
    }

    runtime.loop_state = ListeningLoop::On;
    runtime.last_event = String::from("Continuous listening on");
    arm_next_session(&mut runtime, "keyboard_space");
}

fn arm_next_session(runtime: &mut SidecarRuntime, source: &'static str) {
    let session_id = format!("voice-{}", now_unix_millis());
    let SessionEvent::ListenArmed { session_id } = runtime.session.arm(session_id) else {
        return;
    };
    runtime.clear_error_text();
    runtime.last_event = match source {
        "keyboard_space" => String::from("Listening. Speak anytime."),
        _ => String::from("Listening for the next utterance"),
    };
    debug!(session_id = %session_id, source = %source, "listen armed");
}

fn drain_audio(mut runtime: ResMut<SidecarRuntime>) {
    let (errors, samples) = {
        let Ok(audio) = &runtime.audio else {
            return;
        };
        (audio.drain_errors(), audio.drain_samples())
    };
    for error in errors {
        warn!(error = %error, "audio stream error");
        runtime.last_status = format!("Audio stream error: {error}");
    }
    let events = runtime.session.process_samples(&samples);
    for event in events {
        handle_session_event(&mut runtime, event);
    }
}

fn handle_session_event(runtime: &mut SidecarRuntime, event: SessionEvent) {
    match event {
        SessionEvent::ListenArmed { .. } => {},
        SessionEvent::SpeechStarted { session_id } => {
            runtime.clear_error_text();
            runtime.clear_transcription_status();
            runtime.last_event = String::from("Listening");
            debug!(session_id = %session_id, "speech started");
        },
        SessionEvent::SpeechSettling { session_id } => {
            runtime.last_event = String::from("Settling");
            debug!(
                session_id = %session_id,
                silence_commit_ms = SessionConfig::default().silence_commit_ms,
                "speech settling"
            );
        },
        SessionEvent::AudioCommitted(committed) => {
            match write_candidate_audio(runtime, committed, TranscriptionPurpose::Final) {
                CandidateAudioWrite::Written if runtime.loop_state == ListeningLoop::On => {
                    arm_next_session(runtime, "continuous");
                },
                CandidateAudioWrite::Written => {
                    runtime.session.mark_transcribing();
                },
                CandidateAudioWrite::Failed => {},
            }
        },
        SessionEvent::CandidateReady(committed) => {
            let _write_status =
                write_candidate_audio(runtime, committed, TranscriptionPurpose::Probe);
        },
        SessionEvent::SpeechTooShort { session_id } => {
            runtime.last_event = format!("Too quiet or too brief: {session_id}");
            debug!(session_id = %session_id, "speech too short");
            if runtime.loop_state == ListeningLoop::On {
                arm_next_session(runtime, "continuous");
            }
        },
    }
}

fn write_candidate_audio(
    runtime: &mut SidecarRuntime,
    committed: CommittedUtterance,
    purpose: TranscriptionPurpose,
) -> CandidateAudioWrite {
    let Some(paths) = runtime.paths() else {
        runtime
            .session
            .mark_error("runtime log is unavailable; audio was not written");
        return CandidateAudioWrite::Failed;
    };
    let audio_path = paths.audio_path(&committed.session_id);
    match write_wav(&audio_path, committed.sample_rate, &committed.samples) {
        Ok(()) => {
            let root_session_id = root_session_id(&committed.session_id);
            runtime.last_event = match purpose {
                TranscriptionPurpose::Probe => String::from("Trying candidate"),
                TranscriptionPurpose::Final => String::from("Candidate window"),
            };
            debug!(
                session_id = %committed.session_id,
                root_session_id = %root_session_id,
                audio_path = %audio_path.display(),
                speech_duration_ms = committed.speech_duration_ms,
                silence_ms = committed.silence_ms,
                recorded_duration_ms = committed.recorded_duration_ms,
                purpose = ?purpose,
                "candidate audio committed"
            );
            queue_transcription(
                runtime,
                committed.session_id,
                root_session_id,
                audio_path,
                purpose,
            );
            CandidateAudioWrite::Written
        },
        Err(error) => {
            warn!(error = %error, "WAV write failed");
            runtime
                .session
                .mark_error(format!("WAV write failed: {error}"));
            CandidateAudioWrite::Failed
        },
    }
}

fn poll_transcription(mut runtime: ResMut<SidecarRuntime>) {
    let mut outcomes = Vec::new();
    let mut index = 0;
    while index < runtime.pending.len() {
        if let Some(outcome) = runtime.pending[index].transcription.try_recv() {
            let job = runtime.pending.swap_remove(index);
            outcomes.push((job.root_session_id, job.purpose, outcome));
        } else {
            index += 1;
        }
    }

    for (root_session_id, purpose, outcome) in outcomes {
        handle_transcription_outcome(&mut runtime, &root_session_id, purpose, outcome);
    }
    start_next_transcription(&mut runtime);
}

fn handle_transcription_outcome(
    runtime: &mut SidecarRuntime,
    root_session_id: &str,
    purpose: TranscriptionPurpose,
    outcome: TranscriptionOutcome,
) {
    match outcome {
        TranscriptionOutcome::Transcribed {
            session_id,
            audio_path,
            text,
            backend,
        } => {
            if purpose == TranscriptionPurpose::Probe {
                handle_probe_transcript(
                    runtime,
                    root_session_id,
                    session_id,
                    audio_path,
                    text,
                    backend,
                );
            } else {
                commit_transcript(runtime, session_id, audio_path, text, backend);
            }
        },
        TranscriptionOutcome::Failed {
            session_id,
            audio_path,
            error,
        } => {
            if purpose == TranscriptionPurpose::Probe {
                runtime.clear_error_text();
                runtime.last_status = format!("Candidate failed: {error}");
                remove_candidate_audio(runtime, &audio_path);
            } else {
                match promote_tentative_for_root(runtime, root_session_id, "candidate fallback") {
                    TentativePromotion::Applied => {},
                    TentativePromotion::Skipped => {
                        if runtime.loop_state == ListeningLoop::Off
                            && !runtime.has_transcription_work()
                        {
                            runtime.last_text = Some(format!("Error: {error}"));
                            runtime.session.mark_error(error.clone());
                        } else {
                            runtime.clear_error_text();
                            runtime.last_status = format!("Transcription failed: {error}");
                        }
                        runtime.last_event = String::from("Transcription failed");
                    },
                }
            }
            warn!(
                session_id = %session_id,
                audio_path = %audio_path.display(),
                error = %error,
                "transcription failed"
            );
        },
        TranscriptionOutcome::Rejected {
            session_id,
            audio_path,
            reason,
        } => {
            if purpose == TranscriptionPurpose::Probe {
                runtime.last_event = String::from("Candidate ignored; listening");
                runtime.clear_error_text();
                remove_candidate_audio(runtime, &audio_path);
            } else {
                match promote_tentative_for_root(runtime, root_session_id, "candidate fallback") {
                    TentativePromotion::Applied => {},
                    TentativePromotion::Skipped => {
                        runtime.last_event = String::from("Candidate ignored; listening");
                        runtime.clear_error_text();
                        if runtime.loop_state == ListeningLoop::Off
                            && !runtime.has_transcription_work()
                        {
                            runtime.session.mark_complete(reason.clone());
                        }
                    },
                }
            }
            debug!(
                session_id = %session_id,
                audio_path = %audio_path.display(),
                reason = %reason,
                "candidate transcription rejected"
            );
            if purpose == TranscriptionPurpose::Final {
                remove_candidate_audio(runtime, &audio_path);
            }
        },
    }
}

fn handle_probe_transcript(
    runtime: &mut SidecarRuntime,
    root_session_id: &str,
    session_id: String,
    audio_path: PathBuf,
    text: String,
    backend: String,
) {
    if runtime.is_completed_root(root_session_id) {
        remove_candidate_audio(runtime, &audio_path);
        return;
    }
    runtime.clear_transcription_status();
    let deadline_ms = now_unix_millis().saturating_add(TENTATIVE_STABILITY_MS);
    let should_replace = runtime
        .tentative
        .as_ref()
        .filter(|tentative| tentative.root_session_id == root_session_id)
        .is_none_or(|tentative| is_better_transcript(&text, &tentative.text));
    if should_replace {
        runtime.tentative = Some(TentativeTranscript {
            root_session_id: root_session_id.to_owned(),
            text: text.clone(),
            backend,
            audio_path: audio_path.to_string_lossy().into_owned(),
            deadline_ms,
        });
        runtime.last_text = Some(format!("Tentative: {text}"));
        runtime.last_event = String::from("Candidate understood; listening");
    } else {
        runtime.last_event = String::from("Candidate unchanged; waiting");
    }
    debug!(
        session_id = %session_id,
        root_session_id = %root_session_id,
        text = %text,
        "candidate transcript accepted"
    );
    remove_candidate_audio(runtime, &audio_path);
}

fn commit_transcript(
    runtime: &mut SidecarRuntime,
    session_id: String,
    audio_path: PathBuf,
    text: String,
    backend: String,
) {
    let root = root_session_id(&session_id);
    runtime.tentative = runtime
        .tentative
        .take()
        .filter(|tentative| tentative.root_session_id != root);
    runtime.mark_root_completed(root);
    runtime.clear_transcription_status();
    runtime.last_event = String::from("Transcript committed");
    runtime.last_text = Some(text.clone());
    if runtime.loop_state == ListeningLoop::Off && !runtime.has_transcription_work() {
        runtime.session.mark_complete(text.clone());
    }
    let event = RuntimeEvent::TranscriptCommitted {
        session_id,
        seq: runtime.next_seq(),
        created_at_unix_ms: now_unix_millis(),
        text,
        audio_path: audio_path.to_string_lossy().into_owned(),
        backend,
    };
    if runtime.append_inbox(event) == InboxAppend::Written {
        remove_transcribed_audio(runtime, &audio_path);
    }
}

fn promote_tentative_transcript(mut runtime: ResMut<SidecarRuntime>) {
    let Some(tentative) = &runtime.tentative else {
        return;
    };
    if now_unix_millis() < tentative.deadline_ms {
        return;
    }
    let root_session_id = tentative.root_session_id.clone();
    let _promotion = promote_tentative_for_root(&mut runtime, &root_session_id, "candidate stable");
}

fn promote_tentative_for_root(
    runtime: &mut SidecarRuntime,
    root_session_id: &str,
    reason: &'static str,
) -> TentativePromotion {
    let Some(tentative) = runtime.tentative.take() else {
        return TentativePromotion::Skipped;
    };
    if tentative.root_session_id != root_session_id {
        runtime.tentative = Some(tentative);
        return TentativePromotion::Skipped;
    }
    runtime.mark_root_completed(root_session_id.to_owned());
    runtime.clear_transcription_status();
    runtime.last_event = format!("Transcript committed from {reason}");
    runtime.last_text = Some(tentative.text.clone());
    let event = RuntimeEvent::TranscriptCommitted {
        session_id:         root_session_id.to_owned(),
        seq:                runtime.next_seq(),
        created_at_unix_ms: now_unix_millis(),
        text:               tentative.text.clone(),
        audio_path:         tentative.audio_path,
        backend:            tentative.backend,
    };
    let _append = runtime.append_inbox(event);
    if current_session_root(runtime).as_deref() == Some(root_session_id) {
        if runtime.loop_state == ListeningLoop::On {
            runtime.session.stop();
            arm_next_session(runtime, "continuous");
        } else {
            runtime.session.mark_complete(tentative.text);
        }
    }
    TentativePromotion::Applied
}

fn root_session_id(session_id: &str) -> String {
    session_id
        .split_once("-probe-")
        .map_or(session_id, |(root, _suffix)| root)
        .to_owned()
}

fn current_session_root(runtime: &SidecarRuntime) -> Option<String> {
    runtime
        .session
        .snapshot()
        .session_id
        .as_deref()
        .map(root_session_id)
}

fn is_better_transcript(candidate: &str, current: &str) -> bool {
    transcript_score(candidate) > transcript_score(current)
}

fn transcript_score(text: &str) -> (usize, usize) {
    let normalized = normalized_transcript(text);
    let words = normalized.split_whitespace().count();
    let characters = normalized
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    (words, characters)
}

fn normalized_transcript(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let normalized: String = word
                .chars()
                .filter(|character| character.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            if normalized == "okay" {
                String::from("ok")
            } else {
                normalized
            }
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn queue_transcription(
    runtime: &mut SidecarRuntime,
    session_id: String,
    root_session_id: String,
    audio_path: PathBuf,
    purpose: TranscriptionPurpose,
) {
    if purpose == TranscriptionPurpose::Probe && runtime.is_completed_root(&root_session_id) {
        remove_candidate_audio(runtime, &audio_path);
        return;
    }
    let job = QueuedTranscription {
        session_id,
        root_session_id,
        audio_path,
        purpose,
    };
    if runtime.pending.is_empty() {
        runtime.last_event = match purpose {
            TranscriptionPurpose::Probe => String::from("Trying candidate"),
            TranscriptionPurpose::Final => String::from("Candidate window"),
        };
        spawn_transcription_job(runtime, job);
        return;
    }
    if runtime.queued.len() < MAX_QUEUED_TRANSCRIPTIONS {
        runtime.last_event = format!("Candidate queued ({})", runtime.queued.len() + 1);
        debug!(
            session_id = %job.session_id,
            root_session_id = %job.root_session_id,
            audio_path = %job.audio_path.display(),
            queued = runtime.queued.len() + 1,
            purpose = ?job.purpose,
            "candidate queued behind active transcription"
        );
        runtime.queued.push_back(job);
        return;
    }
    if purpose == TranscriptionPurpose::Final
        && let Some(evicted) = evict_queued_probe(runtime)
    {
        remove_candidate_audio(runtime, &evicted.audio_path);
        runtime.last_event = String::from("Final candidate queued");
        runtime.queued.push_back(job);
        return;
    }
    warn!(
        session_id = %job.session_id,
        root_session_id = %job.root_session_id,
        audio_path = %job.audio_path.display(),
        queued = runtime.queued.len(),
        purpose = ?job.purpose,
        "dropping candidate because transcription queue is full"
    );
    runtime.last_event = String::from("Candidate dropped; STT busy");
    remove_candidate_audio(runtime, &job.audio_path);
}

fn evict_queued_probe(runtime: &mut SidecarRuntime) -> Option<QueuedTranscription> {
    let position = runtime
        .queued
        .iter()
        .position(|job| job.purpose == TranscriptionPurpose::Probe)?;
    runtime.queued.remove(position)
}

fn start_next_transcription(runtime: &mut SidecarRuntime) {
    if !runtime.pending.is_empty() {
        return;
    }
    let Some(job) = next_queued_job(runtime) else {
        return;
    };
    runtime.last_event = format!(
        "Transcribing queued candidate; {} queued",
        runtime.queued.len()
    );
    spawn_transcription_job(runtime, job);
}

fn next_queued_job(runtime: &mut SidecarRuntime) -> Option<QueuedTranscription> {
    while let Some(job) = runtime.queued.pop_front() {
        if job.purpose == TranscriptionPurpose::Probe
            && runtime.is_completed_root(&job.root_session_id)
        {
            remove_candidate_audio(runtime, &job.audio_path);
            continue;
        }
        return Some(job);
    }
    None
}

fn spawn_transcription_job(runtime: &mut SidecarRuntime, job: QueuedTranscription) {
    runtime.pending.push(PendingTranscriptionJob {
        transcription:   spawn_transcription(job.session_id, job.audio_path),
        root_session_id: job.root_session_id,
        purpose:         job.purpose,
    });
}

fn remove_transcribed_audio(runtime: &mut SidecarRuntime, audio_path: &Path) {
    match fs::remove_file(audio_path) {
        Ok(()) => {
            runtime.last_event = String::from("Transcript committed; WAV removed");
        },
        Err(error) if error.kind() == ErrorKind::NotFound => {
            runtime.last_event = String::from("Transcript committed; WAV already removed");
        },
        Err(error) => {
            warn!(
                audio_path = %audio_path.display(),
                error = %error,
                "WAV cleanup failed"
            );
            runtime.last_status = format!("WAV cleanup failed: {error}");
        },
    }
}

fn remove_candidate_audio(runtime: &mut SidecarRuntime, audio_path: &Path) {
    match fs::remove_file(audio_path) {
        Ok(()) => {},
        Err(error) if error.kind() == ErrorKind::NotFound => {},
        Err(error) => {
            warn!(
                audio_path = %audio_path.display(),
                error = %error,
                "candidate WAV cleanup failed"
            );
            runtime.last_status = format!("Candidate WAV cleanup failed: {error}");
        },
    }
}

fn refresh_feedback(
    runtime: Res<SidecarRuntime>,
    panel: Single<Entity, With<VoiceStatusPanel>>,
    mut panel_text: PanelText,
) {
    let snapshot = runtime.session.snapshot();
    let mode = runtime.loop_state.label();
    let mut event = format!(
        "{} | speech {} ms | silence {} ms | pending {}",
        runtime.last_event,
        snapshot.speech_ms,
        snapshot.silence_ms,
        runtime.transcription_work_count()
    );
    if runtime.last_status != MIC_READY {
        event = format!("{event} | {}", runtime.last_status);
    }
    let transcript = runtime
        .last_text
        .clone()
        .or(snapshot.transcript)
        .or(snapshot.error)
        .unwrap_or_else(|| String::from("none yet"));
    let level = format!("{:.3}", snapshot.rms);
    let gate = format!(
        "voice {:.2} / start {:.2}",
        snapshot.vad_probability, snapshot.vad_gate_probability
    );
    set_status_field(&mut panel_text, *panel, FIELD_LOOP, mode);
    set_status_field(
        &mut panel_text,
        *panel,
        FIELD_CAPTURE,
        snapshot.phase.label(),
    );
    set_status_field(&mut panel_text, *panel, FIELD_EVENT, &event);
    set_status_field(&mut panel_text, *panel, FIELD_TRANSCRIPT, &transcript);
    set_status_field(&mut panel_text, *panel, FIELD_MIC, &level);
    set_status_field(&mut panel_text, *panel, FIELD_GATE, &gate);
}

fn set_status_field(panel_text: &mut PanelText, panel: Entity, field: &str, text: &str) {
    panel_text.set_text(panel, &PanelFieldId::named(field), text);
}

fn prompt_face_panel() -> Result<DiegeticPanel, PanelBuildError> {
    let mut style = CubeFacePanelStyle::for_cube(CUBE_SIZE);
    style.size *= 1.02;
    style.padding *= 0.18;
    style.title_size *= 1.85;
    style.color = FACE_PROMPT_COLOR;
    let transparent = cube_face_panel_material();
    DiegeticPanel::world()
        .size(style.size, style.size)
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .material(transparent)
        .text_material(prompt_text_material())
        .with_tree(prompt_face_tree(style))
        .build()
}

fn prompt_text_material() -> StandardMaterial {
    let mut material = cube_face_panel_material();
    material.base_color = FACE_PROMPT_COLOR;
    material.emissive = FACE_PROMPT_COLOR.into();
    material
}

fn prompt_face_tree(style: CubeFacePanelStyle) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(style.size))
            .height(Sizing::fixed(style.size))
            .padding(Padding::all(style.padding))
            .alignment(AlignX::Center, AlignY::Center)
            .clip(),
    );
    builder.text(
        PROMPT_TEXT,
        TextStyle::new(style.title_size)
            .with_color(style.color)
            .with_align(TextAlign::Center)
            .with_shadow_mode(GlyphShadowMode::None)
            .wrap(TextWrap::Words),
    );
    builder.build()
}

fn status_panel() -> Result<DiegeticPanel, PanelBuildError> {
    let unlit = screen_panel_material();
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(status_panel_tree())
        .build()
}

fn status_panel_tree() -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::fixed(STATUS_PANEL_WIDTH),
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .gap(STATUS_PANEL_GAP),
                |builder| {
                    builder.text("Voice session", status_title_style());
                    status_divider(builder);
                    status_row(builder, "Loop", FIELD_LOOP, "Off");
                    status_row(builder, "Capture", FIELD_CAPTURE, "Idle");
                    status_row(builder, "Event", FIELD_EVENT, "Press space to start");
                    status_row(builder, "Transcript", FIELD_TRANSCRIPT, "none yet");
                    status_row(builder, "Mic level", FIELD_MIC, "0.000");
                    status_row(builder, "Gate", FIELD_GATE, "voice 0.00 / start 0.36");
                },
            );
        },
    );
    builder.build()
}

fn status_row(builder: &mut LayoutBuilder, label: &'static str, field: &'static str, value: &str) {
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(STATUS_ROW_GAP)
            .align_y(AlignY::Top),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(STATUS_LABEL_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(label, status_label_style());
                },
            );
            builder.with(
                El::new().width(Sizing::GROW).height(Sizing::FIT),
                |builder| {
                    builder.text_id(
                        PanelFieldId::named(field),
                        value,
                        status_value_style(status_value_role(field)),
                    );
                },
            );
        },
    );
}

fn status_divider(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(STATUS_DIVIDER_HEIGHT))
            .background(ACCENT_COLOR),
        |_| {},
    );
}

fn status_title_style() -> TextStyle {
    TextStyle::new(18.0)
        .with_color(TEXT_COLOR)
        .bold()
        .with_shadow_mode(GlyphShadowMode::None)
}

fn status_label_style() -> TextStyle {
    TextStyle::new(13.0)
        .with_color(MUTED_COLOR)
        .no_wrap()
        .with_shadow_mode(GlyphShadowMode::None)
}

fn status_value_role(field: &str) -> StatusValueRole {
    match field {
        FIELD_TRANSCRIPT => StatusValueRole::Transcript,
        _ => StatusValueRole::Standard,
    }
}

fn status_value_style(role: StatusValueRole) -> TextStyle {
    let color = match role {
        StatusValueRole::Transcript => TRANSCRIPT_COLOR,
        StatusValueRole::Standard => TEXT_COLOR,
    };
    TextStyle::new(13.0)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
        .wrap(TextWrap::Words)
}

//! Bevy feedback UI for the Hana voice sidecar POC.
//!
//! Press space to start recording. Press space again to stop recording and send
//! that captured audio window to Apple Speech.

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
use bevy_kana::ToF32;
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
use hana_prosody::PendingTranscription;
use hana_prosody::RuntimeEvent;
use hana_prosody::RuntimeLog;
use hana_prosody::RuntimePaths;
use hana_prosody::TranscriptionOutcome;
use hana_prosody::now_unix_millis;
use hana_prosody::spawn_transcription;
use hana_prosody::write_wav;

const TITLE: &str = "Hana Prosody";
const SPACE_CONTROL: &str = "Space Record/Send";
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

const TEXT_COLOR: Color = Color::srgb(0.92, 0.94, 0.98);
const MUTED_COLOR: Color = Color::srgb(0.68, 0.72, 0.78);
const TRANSCRIPT_COLOR: Color = Color::srgb(0.88, 0.86, 0.72);
const ACCENT_COLOR: Color = Color::srgb(0.18, 0.70, 0.92);
const FACE_PROMPT_COLOR: Color = Color::linear_rgb(3.6, 5.4, 9.0);

const FIELD_STATE: &str = "state";
const FIELD_RECORDING: &str = "recording";
const FIELD_EVENT: &str = "event";
const FIELD_TRANSCRIPT: &str = "transcript";
const FIELD_MIC: &str = "mic";
const FIELD_QUEUE: &str = "queue";

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
            (drain_audio, poll_transcription, refresh_feedback).chain(),
        )
        .with_shortcut(KeyCode::Space, toggle_recording)
        .run();
}

#[derive(Resource)]
struct SidecarRuntime {
    audio:       Result<AudioInput, String>,
    log:         Result<RuntimeLog, String>,
    sample_rate: u32,
    recording:   Option<RecordingSession>,
    pending:     Vec<PendingTranscriptionJob>,
    queued:      VecDeque<QueuedTranscription>,
    mic_rms:     f32,
    last_event:  String,
    last_text:   Option<String>,
    last_status: String,
}

struct RecordingSession {
    session_id: String,
    samples:    Vec<f32>,
}

struct QueuedTranscription {
    session_id: String,
    audio_path: PathBuf,
}

struct PendingTranscriptionJob {
    transcription: PendingTranscription,
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
            sample_rate,
            recording: None,
            pending: Vec::new(),
            queued: VecDeque::new(),
            mic_rms: 0.0,
            last_event: String::from("Press space to record"),
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
        if self.last_status.starts_with("Transcription failed:")
            || self.last_status.starts_with("No transcript:")
        {
            self.last_status = String::from(MIC_READY);
        }
    }

    fn recording_ms(&self) -> u64 {
        self.recording.as_ref().map_or(0, |recording| {
            duration_ms(recording.samples.len(), self.sample_rate)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InterfacePhase {
    Ready,
    Recording,
    Transcribing,
}

impl InterfacePhase {
    const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Recording => "Recording",
            Self::Transcribing => "Transcribing",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AudioDrainMode {
    IgnoreSamples,
    RecordSamples,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InboxAppend {
    Written,
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

fn toggle_recording(mut runtime: ResMut<SidecarRuntime>) {
    if runtime.recording.is_some() {
        send_recording(&mut runtime);
    } else {
        start_recording(&mut runtime);
    }
}

fn start_recording(runtime: &mut SidecarRuntime) {
    if let Err(error) = &runtime.audio {
        runtime.last_status = format!("Mic unavailable: {error}");
        runtime.last_event = String::from("Recording unavailable");
        return;
    }

    drain_microphone(runtime, AudioDrainMode::IgnoreSamples);
    runtime.clear_error_text();
    runtime.clear_transcription_status();

    let session_id = format!("voice-{}", now_unix_millis());
    runtime.recording = Some(RecordingSession {
        session_id: session_id.clone(),
        samples:    Vec::new(),
    });
    runtime.last_event = String::from("Recording; press space to send");
    debug!(session_id = %session_id, "manual recording started");
}

fn send_recording(runtime: &mut SidecarRuntime) {
    drain_microphone(runtime, AudioDrainMode::RecordSamples);
    let Some(recording) = runtime.recording.take() else {
        return;
    };
    let recorded_ms = duration_ms(recording.samples.len(), runtime.sample_rate);
    if recording.samples.is_empty() {
        runtime.last_event = String::from("No audio captured");
        return;
    }

    let Some(paths) = runtime.paths() else {
        runtime.last_status = String::from("Runtime log unavailable; audio was not written");
        runtime.last_event = String::from("Recording could not be sent");
        return;
    };
    let audio_path = paths.audio_path(&recording.session_id);
    match write_wav(&audio_path, runtime.sample_rate, &recording.samples) {
        Ok(()) => {
            runtime.last_event = format!("Sent {recorded_ms} ms recording");
            debug!(
                session_id = %recording.session_id,
                audio_path = %audio_path.display(),
                recorded_ms,
                "manual recording sent"
            );
            queue_transcription(runtime, recording.session_id, audio_path);
        },
        Err(error) => {
            warn!(error = %error, "WAV write failed");
            runtime.last_status = format!("WAV write failed: {error}");
            runtime.last_event = String::from("Recording could not be sent");
        },
    }
}

fn drain_audio(mut runtime: ResMut<SidecarRuntime>) {
    drain_microphone(&mut runtime, AudioDrainMode::RecordSamples);
}

fn drain_microphone(runtime: &mut SidecarRuntime, mode: AudioDrainMode) {
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
    if !samples.is_empty() {
        runtime.mic_rms = rms(&samples);
    }
    if mode == AudioDrainMode::RecordSamples
        && let Some(recording) = &mut runtime.recording
    {
        recording.samples.extend_from_slice(&samples);
    }
}

fn poll_transcription(mut runtime: ResMut<SidecarRuntime>) {
    let mut outcomes = Vec::new();
    let mut index = 0;
    while index < runtime.pending.len() {
        if let Some(outcome) = runtime.pending[index].transcription.try_recv() {
            let _job = runtime.pending.swap_remove(index);
            outcomes.push(outcome);
        } else {
            index += 1;
        }
    }

    for outcome in outcomes {
        handle_transcription_outcome(&mut runtime, outcome);
    }
    start_next_transcription(&mut runtime);
}

fn handle_transcription_outcome(runtime: &mut SidecarRuntime, outcome: TranscriptionOutcome) {
    match outcome {
        TranscriptionOutcome::Transcribed {
            session_id,
            audio_path,
            text,
            backend,
        } => {
            commit_transcript(runtime, session_id, audio_path, text, backend);
        },
        TranscriptionOutcome::Failed {
            session_id,
            audio_path,
            error,
        } => {
            runtime.last_event = String::from("Transcription failed");
            runtime.last_status = format!("Transcription failed: {error}");
            runtime.last_text = Some(format!("Error: {error}"));
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
            runtime.last_event = String::from("No speech detected");
            runtime.last_status = format!("No transcript: {reason}");
            runtime.clear_error_text();
            debug!(
                session_id = %session_id,
                audio_path = %audio_path.display(),
                reason = %reason,
                "recording transcription rejected"
            );
            remove_discarded_audio(runtime, &audio_path);
        },
    }
}

fn commit_transcript(
    runtime: &mut SidecarRuntime,
    session_id: String,
    audio_path: PathBuf,
    text: String,
    backend: String,
) {
    runtime.clear_transcription_status();
    runtime.last_event = String::from("Transcript committed");
    runtime.last_text = Some(text.clone());
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

fn queue_transcription(runtime: &mut SidecarRuntime, session_id: String, audio_path: PathBuf) {
    let job = QueuedTranscription {
        session_id,
        audio_path,
    };
    if runtime.pending.is_empty() {
        runtime.last_event = String::from("Transcribing recording");
        spawn_transcription_job(runtime, job);
        return;
    }
    if runtime.queued.len() < MAX_QUEUED_TRANSCRIPTIONS {
        runtime.last_event = format!("Recording queued ({})", runtime.queued.len() + 1);
        debug!(
            session_id = %job.session_id,
            audio_path = %job.audio_path.display(),
            queued = runtime.queued.len() + 1,
            "recording queued behind active transcription"
        );
        runtime.queued.push_back(job);
        return;
    }
    warn!(
        session_id = %job.session_id,
        audio_path = %job.audio_path.display(),
        queued = runtime.queued.len(),
        "dropping recording because transcription queue is full"
    );
    runtime.last_event = String::from("Recording dropped; STT busy");
    remove_discarded_audio(runtime, &job.audio_path);
}

fn start_next_transcription(runtime: &mut SidecarRuntime) {
    if !runtime.pending.is_empty() {
        return;
    }
    let Some(job) = runtime.queued.pop_front() else {
        return;
    };
    runtime.last_event = format!(
        "Transcribing queued recording; {} queued",
        runtime.queued.len()
    );
    spawn_transcription_job(runtime, job);
}

fn spawn_transcription_job(runtime: &mut SidecarRuntime, job: QueuedTranscription) {
    runtime.pending.push(PendingTranscriptionJob {
        transcription: spawn_transcription(job.session_id, job.audio_path),
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

fn remove_discarded_audio(runtime: &mut SidecarRuntime, audio_path: &Path) {
    match fs::remove_file(audio_path) {
        Ok(()) => {},
        Err(error) if error.kind() == ErrorKind::NotFound => {},
        Err(error) => {
            warn!(
                audio_path = %audio_path.display(),
                error = %error,
                "discarded WAV cleanup failed"
            );
            runtime.last_status = format!("Discarded WAV cleanup failed: {error}");
        },
    }
}

fn refresh_feedback(
    runtime: Res<SidecarRuntime>,
    panel: Single<Entity, With<VoiceStatusPanel>>,
    mut panel_text: PanelText,
) {
    let phase = interface_phase(&runtime).label();
    let mut event = format!(
        "{} | pending {}",
        runtime.last_event,
        runtime.transcription_work_count()
    );
    if runtime.last_status != MIC_READY {
        event = format!("{event} | {}", runtime.last_status);
    }
    let transcript = runtime
        .last_text
        .clone()
        .unwrap_or_else(|| String::from("none yet"));
    let recording = format!("{} ms", runtime.recording_ms());
    let level = format!("{:.3}", runtime.mic_rms);
    let queue = format!(
        "{} active / {} queued",
        runtime.pending.len(),
        runtime.queued.len()
    );
    set_status_field(&mut panel_text, *panel, FIELD_STATE, phase);
    set_status_field(&mut panel_text, *panel, FIELD_RECORDING, &recording);
    set_status_field(&mut panel_text, *panel, FIELD_EVENT, &event);
    set_status_field(&mut panel_text, *panel, FIELD_TRANSCRIPT, &transcript);
    set_status_field(&mut panel_text, *panel, FIELD_MIC, &level);
    set_status_field(&mut panel_text, *panel, FIELD_QUEUE, &queue);
}

fn interface_phase(runtime: &SidecarRuntime) -> InterfacePhase {
    if runtime.recording.is_some() {
        InterfacePhase::Recording
    } else if runtime.has_transcription_work() {
        InterfacePhase::Transcribing
    } else {
        InterfacePhase::Ready
    }
}

fn duration_ms(samples: usize, sample_rate: u32) -> u64 {
    let samples = u64::try_from(samples).map_or(u64::MAX, |value| value);
    samples.saturating_mul(1_000) / u64::from(sample_rate.max(1))
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples.iter().map(|sample| sample * sample).sum::<f32>();
    (sum / samples.len().to_f32()).sqrt()
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
                    status_row(builder, "State", FIELD_STATE, "Ready");
                    status_row(builder, "Recording", FIELD_RECORDING, "0 ms");
                    status_row(builder, "Event", FIELD_EVENT, "Press space to record");
                    status_row(builder, "Transcript", FIELD_TRANSCRIPT, "none yet");
                    status_row(builder, "Mic level", FIELD_MIC, "0.000");
                    status_row(builder, "Queue", FIELD_QUEUE, "0 active / 0 queued");
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

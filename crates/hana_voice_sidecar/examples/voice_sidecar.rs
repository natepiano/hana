//! Bevy feedback UI for the Hana voice sidecar POC.
//!
//! Press space to toggle continuous transcription. While enabled, the sidecar
//! proposes candidate windows after pauses, lets Apple Speech validate them, and
//! appends committed transcript JSONL for the agent.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

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
use hana_voice_sidecar::AudioInput;
use hana_voice_sidecar::PendingTranscription;
use hana_voice_sidecar::RuntimeEvent;
use hana_voice_sidecar::RuntimeLog;
use hana_voice_sidecar::RuntimePaths;
use hana_voice_sidecar::SessionConfig;
use hana_voice_sidecar::SessionEvent;
use hana_voice_sidecar::TranscriptionOutcome;
use hana_voice_sidecar::VoiceSession;
use hana_voice_sidecar::now_unix_millis;
use hana_voice_sidecar::spawn_transcription;
use hana_voice_sidecar::write_wav;

const TITLE: &str = "Hana Voice Sidecar";
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
            (drain_audio, poll_transcription, refresh_feedback).chain(),
        )
        .with_shortcut(KeyCode::Space, toggle_listening_loop)
        .run();
}

#[derive(Resource)]
struct SidecarRuntime {
    audio:       Result<AudioInput, String>,
    log:         Result<RuntimeLog, String>,
    session:     VoiceSession,
    pending:     Vec<PendingTranscription>,
    loop_state:  ListeningLoop,
    last_event:  String,
    last_text:   Option<String>,
    last_status: String,
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
        runtime.last_event = if runtime.pending.is_empty() {
            String::from("Stopped")
        } else {
            format!("Stopped; finishing {} transcript(s)", runtime.pending.len())
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
            let Some(paths) = runtime.paths() else {
                runtime
                    .session
                    .mark_error("runtime log is unavailable; audio was not written");
                return;
            };
            let audio_path = paths.audio_path(&committed.session_id);
            match write_wav(&audio_path, committed.sample_rate, &committed.samples) {
                Ok(()) => {
                    runtime.last_event = String::from("Candidate window");
                    debug!(
                        session_id = %committed.session_id,
                        audio_path = %audio_path.display(),
                        speech_duration_ms = committed.speech_duration_ms,
                        silence_ms = committed.silence_ms,
                        recorded_duration_ms = committed.recorded_duration_ms,
                        "candidate audio committed"
                    );
                    runtime
                        .pending
                        .push(spawn_transcription(committed.session_id, audio_path));
                    if runtime.loop_state == ListeningLoop::On {
                        arm_next_session(runtime, "continuous");
                    } else {
                        runtime.session.mark_transcribing();
                    }
                },
                Err(error) => {
                    warn!(error = %error, "WAV write failed");
                    runtime
                        .session
                        .mark_error(format!("WAV write failed: {error}"));
                },
            }
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

fn poll_transcription(mut runtime: ResMut<SidecarRuntime>) {
    let mut outcomes = Vec::new();
    let mut index = 0;
    while index < runtime.pending.len() {
        if let Some(outcome) = runtime.pending[index].try_recv() {
            outcomes.push(outcome);
            let _finished = runtime.pending.swap_remove(index);
        } else {
            index += 1;
        }
    }

    for outcome in outcomes {
        handle_transcription_outcome(&mut runtime, outcome);
    }
}

fn handle_transcription_outcome(runtime: &mut SidecarRuntime, outcome: TranscriptionOutcome) {
    match outcome {
        TranscriptionOutcome::Transcribed {
            session_id,
            audio_path,
            text,
            backend,
        } => {
            runtime.last_event = String::from("Transcript committed");
            runtime.last_text = Some(text.clone());
            if runtime.loop_state == ListeningLoop::Off && runtime.pending.is_empty() {
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
        },
        TranscriptionOutcome::Failed {
            session_id,
            audio_path,
            error,
        } => {
            runtime.last_event = String::from("Transcription failed");
            runtime.last_text = Some(format!("Error: {error}"));
            if runtime.loop_state == ListeningLoop::Off && runtime.pending.is_empty() {
                runtime.session.mark_error(error.clone());
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
            runtime.last_event = String::from("Candidate ignored; listening");
            if runtime.loop_state == ListeningLoop::Off && runtime.pending.is_empty() {
                runtime.session.mark_complete(reason.clone());
            }
            debug!(
                session_id = %session_id,
                audio_path = %audio_path.display(),
                reason = %reason,
                "candidate transcription rejected"
            );
            remove_candidate_audio(runtime, &audio_path);
        },
    }
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
        runtime.pending.len()
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
        "noise {:.3} / start {:.3}",
        snapshot.noise_rms, snapshot.gate_rms
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
                    status_row(builder, "Gate", FIELD_GATE, "noise 0.000 / start 0.020");
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

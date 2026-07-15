//! Bevy feedback UI for the Hana voice sidecar POC.
//!
//! Press space to start recording. Press space again to stop recording and send
//! that captured audio window to Apple Speech.

use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;
use std::process;

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use bevy_remote::BrpError;
use bevy_remote::BrpResult;
use bevy_remote::RemoteMethodSystemId;
use bevy_remote::RemoteMethods;
use bevy_remote::error_codes;
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
use hana_diegetic::AlignX;
use hana_diegetic::AlignY;
use hana_diegetic::DiegeticPanel;
use hana_diegetic::El;
use hana_diegetic::Fit;
use hana_diegetic::GlyphShadowMode;
use hana_diegetic::LayoutBuilder;
use hana_diegetic::LayoutTree;
use hana_diegetic::Padding;
use hana_diegetic::PanelBuildError;
use hana_diegetic::PanelElementId;
use hana_diegetic::PanelText;
use hana_diegetic::Sizing;
use hana_diegetic::Text;
use hana_diegetic::TextAlign;
use hana_diegetic::TextStyle;
use hana_diegetic::TextWrap;
use hana_diegetic::Unit;
use hana_prosody::AudioInput;
use hana_prosody::PendingTranscription;
use hana_prosody::TranscriptionOutcome;
use hana_prosody::TranscriptionRequest;
use hana_prosody::now_unix_millis;
use hana_prosody::spawn_transcription;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_json::json;

// audio
const FALLBACK_SAMPLE_RATE: u32 = 48_000;
const MIC_READY: &str = "Mic ready";
const MILLISECONDS_PER_SECOND: u64 = 1_000;
const MIN_SAMPLE_RATE: u32 = 1;

// cube
const CUBE_COLOR: Color = Color::srgb(0.42, 0.50, 0.78);
const CUBE_GROUND_OFFSET: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const FACE_PROMPT_COLOR: Color = Color::linear_rgb(3.6, 5.4, 9.0);
const FACE_PROMPT_NAME: &str = "voice prompt face";
const FACE_PROMPT_PADDING_MULTIPLIER: f32 = 0.18;
const FACE_PROMPT_SIZE_MULTIPLIER: f32 = 1.02;
const FACE_PROMPT_TITLE_SIZE_MULTIPLIER: f32 = 1.85;
const PROMPT_TEXT: &str = "Tell me how you want to look";

// interface copy
const ERROR_PREFIX: &str = "Error:";
const EVENT_LABEL: &str = "Event";
const MIC_LEVEL_LABEL: &str = "Mic level";
const MIC_LEVEL_READY: &str = "0.000";
const NO_TRANSCRIPT_PLACEHOLDER: &str = "none yet";
const QUEUE_LABEL: &str = "Queue";
const QUEUE_READY: &str = "0 active / 0 queued";
const READY_EVENT: &str = "Press space to record";
const RECORDING_LABEL: &str = "Recording";
const RECORDING_READY: &str = "0 ms";
const SPACE_CONTROL: &str = "Space Record/Send";
const STATE_LABEL: &str = "State";
const STATE_READY: &str = "Ready";
const STATUS_PANEL_TITLE: &str = "Voice session";
const TITLE: &str = "Hana Prosody";
const TRANSCRIPT_LABEL: &str = "Transcript";

// layout
const ACCENT_COLOR: Color = Color::srgb(0.18, 0.70, 0.92);
const HOME_MARGIN: f32 = 0.62;
const HOME_PITCH: f32 = 0.45;
const HOME_YAW: f32 = 0.25;
const MUTED_COLOR: Color = Color::srgb(0.68, 0.72, 0.78);
const STATUS_DIVIDER_HEIGHT: f32 = 1.5;
const STATUS_LABEL_FONT_SIZE: f32 = 13.0;
const STATUS_LABEL_WIDTH: f32 = 116.0;
const STATUS_PANEL_GAP: f32 = 8.0;
const STATUS_PANEL_WIDTH: f32 = 460.0;
const STATUS_ROW_GAP: f32 = 10.0;
const STATUS_TITLE_FONT_SIZE: f32 = 18.0;
const STATUS_VALUE_FONT_SIZE: f32 = 13.0;
const TEXT_COLOR: Color = Color::srgb(0.92, 0.94, 0.98);
const TRANSCRIPT_COLOR: Color = Color::srgb(0.88, 0.86, 0.72);

// runtime
const AUDIO_DIRECTORY_NAME: &str = "audio";
const BRP_ACTIVE_RECORDING_ERROR: &str = "cannot change voice runtime while recording is active";
const BRP_INVALID_RUNTIME_PARAMS_ERROR: &str = "runtime params must be an object";
const BRP_MISSING_SET_RUNTIME_PARAMS_ERROR: &str = "missing set_runtime params";
const BRP_PENDING_TRANSCRIPTION_ERROR: &str =
    "cannot change voice runtime while transcription work is pending";
const CHANGED_FIELD: &str = "changed";
const DEFAULT_RUNTIME_DIRECTORY: &str = "hana_prosody";
const FIELD_EVENT: &str = "event";
const FIELD_MIC: &str = "mic";
const FIELD_QUEUE: &str = "queue";
const FIELD_RECORDING: &str = "recording";
const FIELD_STATE: &str = "state";
const FIELD_TRANSCRIPT: &str = "transcript";
const HANA_ART_RUN_DIR_ENV: &str = "HANA_ART_RUN_DIR";
const RUNTIME_CONFIGURED: bool = true;
const RUNTIME_CONFIGURED_FIELD: &str = "configured";
const RUNTIME_KIND: &str = "hana_voice_runtime";
const RUNTIME_KIND_FIELD: &str = "kind";
const RUNTIME_PENDING_TRANSCRIPTIONS_FIELD: &str = "pending_transcriptions";
const RUNTIME_QUEUED_TRANSCRIPTIONS_FIELD: &str = "queued_transcriptions";
const RUNTIME_RECORDING_FIELD: &str = "recording";
const RUNTIME_ROOT_FIELD: &str = "root";
const RUNTIME_SCRATCH_DIRECTORY_FIELD: &str = "scratch_dir";
const RUNTIME_SET_PATHS_METHOD: &str = "hana_runtime/set_paths";
const RUNTIME_VERSION: u8 = 1;
const RUNTIME_VERSION_FIELD: &str = "version";
const VOICE_RUNTIME_METHOD: &str = "hana_voice/runtime";

// transcription
const MAX_QUEUED_TRANSCRIPTIONS: usize = 3;
const NO_AUDIO_CAPTURED_EVENT: &str = "No audio captured";
const NO_SPEECH_EVENT: &str = "No speech detected";
const NO_TRANSCRIPT_PREFIX: &str = "No transcript:";
const RECORDING_ACTIVE_EVENT: &str = "Recording; press space to send";
const RECORDING_DROPPED_EVENT: &str = "Recording dropped; STT busy";
const RECORDING_UNAVAILABLE_EVENT: &str = "Recording unavailable";
const TRANSCRIPT_COMMITTED_EVENT: &str = "Transcript committed";
const TRANSCRIPTION_FAILED_EVENT: &str = "Transcription failed";
const TRANSCRIPTION_FAILED_PREFIX: &str = "Transcription failed:";
const TRANSCRIPTION_STARTED_EVENT: &str = "Transcribing recording";

fn main() {
    let runtime = SidecarRuntime::new(SidecarPaths::from_env_or_temp());

    let mut app = fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_hdr()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(
            fairy_dust::example_cube_on_ground(CUBE_GROUND_OFFSET),
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
        .insert_resource(runtime);
    register_runtime_brp_methods(app.app_mut());
    app.add_systems(Startup, spawn_status_panel)
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
    audio:                      Result<AudioInput, String>,
    paths:                      SidecarPaths,
    sample_rate:                u32,
    recording_session:          Option<RecordingSession>,
    pending_transcription_jobs: Vec<PendingTranscriptionJob>,
    queued_transcriptions:      VecDeque<QueuedTranscription>,
    mic_rms:                    f32,
    last_event:                 String,
    last_text:                  Option<String>,
    last_status:                String,
}

struct RecordingSession {
    session_id: String,
    samples:    Vec<f32>,
}

struct QueuedTranscription {
    session_id:  String,
    sample_rate: u32,
    samples:     Vec<f32>,
}

struct PendingTranscriptionJob {
    transcription: PendingTranscription,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidecarPaths {
    root:        PathBuf,
    scratch_dir: PathBuf,
}

impl SidecarPaths {
    fn from_env_or_temp() -> Self {
        let root = env::var_os(HANA_ART_RUN_DIR_ENV).map_or_else(
            || {
                env::temp_dir()
                    .join(DEFAULT_RUNTIME_DIRECTORY)
                    .join(process::id().to_string())
            },
            PathBuf::from,
        );
        Self::new(root)
    }

    fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            scratch_dir: root.join(AUDIO_DIRECTORY_NAME),
            root,
        }
    }

    fn root(&self) -> &std::path::Path { &self.root }

    fn scratch_dir(&self) -> &std::path::Path { &self.scratch_dir }
}

impl SidecarRuntime {
    fn new(paths: SidecarPaths) -> Self {
        let audio = AudioInput::open_default().map_err(|error| error.to_string());
        let sample_rate = audio
            .as_ref()
            .map_or(FALLBACK_SAMPLE_RATE, |input| input.status().sample_rate);
        let last_status = match &audio {
            Ok(_input) => String::from(MIC_READY),
            Err(error) => format!("Mic unavailable: {error}"),
        };
        Self {
            audio,
            paths,
            sample_rate,
            recording_session: None,
            pending_transcription_jobs: Vec::new(),
            queued_transcriptions: VecDeque::new(),
            mic_rms: 0.0,
            last_event: String::from(READY_EVENT),
            last_text: None,
            last_status,
        }
    }

    const fn paths(&self) -> &SidecarPaths { &self.paths }

    fn transcription_work_count(&self) -> usize {
        self.pending_transcription_jobs.len() + self.queued_transcriptions.len()
    }

    fn has_transcription_work(&self) -> bool { self.transcription_work_count() > 0 }

    fn set_paths(&mut self, paths: SidecarPaths) {
        self.paths = paths;
        self.last_status = format!("Runtime: {}", self.paths.root().display());
    }

    fn clear_error_text(&mut self) {
        if self
            .last_text
            .as_deref()
            .is_some_and(|text| text.starts_with(ERROR_PREFIX))
        {
            self.last_text = None;
        }
    }

    fn clear_transcription_status(&mut self) {
        if self.last_status.starts_with(TRANSCRIPTION_FAILED_PREFIX)
            || self.last_status.starts_with(NO_TRANSCRIPT_PREFIX)
        {
            self.last_status = String::from(MIC_READY);
        }
    }

    fn recording_ms(&self) -> u64 {
        self.recording_session
            .as_ref()
            .map_or(0, |recording_session| {
                duration_ms(recording_session.samples.len(), self.sample_rate)
            })
    }
}

#[derive(Deserialize)]
struct SetRuntimeParams {
    run_dir: PathBuf,
}

fn register_runtime_brp_methods(app: &mut App) {
    let runtime_system_id = app.world_mut().register_system(voice_runtime_handler);
    let set_runtime_system_id = app.world_mut().register_system(voice_set_runtime_handler);
    let Some(mut remote_methods) = app.world_mut().get_resource_mut::<RemoteMethods>() else {
        warn!(
            "{VOICE_RUNTIME_METHOD} and {RUNTIME_SET_PATHS_METHOD} were not registered because Bevy Remote is unavailable"
        );
        return;
    };

    remote_methods.insert(
        VOICE_RUNTIME_METHOD,
        RemoteMethodSystemId::Instant(runtime_system_id),
    );
    remote_methods.insert(
        RUNTIME_SET_PATHS_METHOD,
        RemoteMethodSystemId::Instant(set_runtime_system_id),
    );
}

fn voice_runtime_handler(
    In(params): In<Option<JsonValue>>,
    runtime: Res<SidecarRuntime>,
) -> BrpResult {
    if params.as_ref().is_some_and(|params| !params.is_object()) {
        return Err(invalid_params(BRP_INVALID_RUNTIME_PARAMS_ERROR));
    }

    Ok(runtime_json(&runtime))
}

fn voice_set_runtime_handler(
    In(params): In<Option<JsonValue>>,
    mut runtime: ResMut<SidecarRuntime>,
) -> BrpResult {
    let params = params.ok_or_else(|| invalid_params(BRP_MISSING_SET_RUNTIME_PARAMS_ERROR))?;
    let SetRuntimeParams { run_dir } = serde_json::from_value(params).map_err(invalid_params)?;
    if runtime.recording_session.is_some() {
        return Err(invalid_params(BRP_ACTIVE_RECORDING_ERROR));
    }
    if runtime.has_transcription_work() {
        return Err(invalid_params(BRP_PENDING_TRANSCRIPTION_ERROR));
    }

    let previous = runtime.paths().root().to_path_buf();
    let paths = SidecarPaths::new(run_dir);
    runtime.set_paths(paths);
    let changed = previous != runtime.paths().root();
    let mut response = runtime_json(&runtime);
    if let Some(response) = response.as_object_mut() {
        response.insert(CHANGED_FIELD.to_string(), json!(changed));
    }
    Ok(response)
}

fn runtime_json(runtime: &SidecarRuntime) -> JsonValue {
    let paths = runtime.paths();

    json!({
        (RUNTIME_KIND_FIELD): RUNTIME_KIND,
        (RUNTIME_VERSION_FIELD): RUNTIME_VERSION,
        (RUNTIME_CONFIGURED_FIELD): RUNTIME_CONFIGURED,
        (RUNTIME_ROOT_FIELD): paths.root().to_string_lossy(),
        (RUNTIME_SCRATCH_DIRECTORY_FIELD): paths.scratch_dir().to_string_lossy(),
        (RUNTIME_RECORDING_FIELD): runtime.recording_session.is_some(),
        (RUNTIME_PENDING_TRANSCRIPTIONS_FIELD): runtime.pending_transcription_jobs.len(),
        (RUNTIME_QUEUED_TRANSCRIPTIONS_FIELD): runtime.queued_transcriptions.len(),
    })
}

fn invalid_params(error: impl ToString) -> BrpError {
    BrpError {
        code:    error_codes::INVALID_PARAMS,
        message: error.to_string(),
        data:    None,
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

fn spawn_status_panel(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    match status_panel(&mut materials) {
        Ok(panel) => {
            commands.spawn((VoiceStatusPanel, panel, Transform::default()));
        },
        Err(error) => {
            error!("voice_sidecar: failed to build status panel: {error}");
        },
    }
}

fn spawn_cube_prompt_faces(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cubes: Query<Entity, With<VoiceCube>>,
) {
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
            match prompt_face_panel(&mut materials) {
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
    if runtime.recording_session.is_some() {
        send_recording(&mut runtime);
    } else {
        start_recording(&mut runtime);
    }
}

fn start_recording(runtime: &mut SidecarRuntime) {
    if let Err(error) = &runtime.audio {
        runtime.last_status = format!("Mic unavailable: {error}");
        runtime.last_event = String::from(RECORDING_UNAVAILABLE_EVENT);
        return;
    }

    drain_microphone(runtime, AudioDrainMode::IgnoreSamples);
    runtime.clear_error_text();
    runtime.clear_transcription_status();

    let session_id = format!("voice-{}", now_unix_millis());
    runtime.recording_session = Some(RecordingSession {
        session_id: session_id.clone(),
        samples:    Vec::new(),
    });
    runtime.last_event = String::from(RECORDING_ACTIVE_EVENT);
    debug!(session_id = %session_id, "manual recording started");
}

fn send_recording(runtime: &mut SidecarRuntime) {
    drain_microphone(runtime, AudioDrainMode::RecordSamples);
    let Some(recording_session) = runtime.recording_session.take() else {
        return;
    };
    let recorded_ms = duration_ms(recording_session.samples.len(), runtime.sample_rate);
    if recording_session.samples.is_empty() {
        runtime.last_event = String::from(NO_AUDIO_CAPTURED_EVENT);
        return;
    }

    runtime.last_event = format!("Sent {recorded_ms} ms recording");
    debug!(
        session_id = %recording_session.session_id,
        recorded_ms,
        "manual recording sent"
    );
    let sample_rate = runtime.sample_rate;
    queue_transcription(
        runtime,
        recording_session.session_id,
        sample_rate,
        recording_session.samples,
    );
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
        && let Some(recording_session) = &mut runtime.recording_session
    {
        recording_session.samples.extend_from_slice(&samples);
    }
}

fn poll_transcription(mut runtime: ResMut<SidecarRuntime>) {
    let mut outcomes = Vec::new();
    let mut index = 0;
    while index < runtime.pending_transcription_jobs.len() {
        if let Some(outcome) = runtime.pending_transcription_jobs[index]
            .transcription
            .try_recv()
        {
            let _ = runtime.pending_transcription_jobs.swap_remove(index);
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
            text,
            backend,
        } => {
            commit_transcript(runtime, session_id, text, backend);
        },
        TranscriptionOutcome::Failed { session_id, error } => {
            runtime.last_event = String::from(TRANSCRIPTION_FAILED_EVENT);
            runtime.last_status = format!("Transcription failed: {error}");
            runtime.last_text = Some(format!("Error: {error}"));
            warn!(
                session_id = %session_id,
                error = %error,
                "transcription failed"
            );
        },
        TranscriptionOutcome::Rejected { session_id, reason } => {
            runtime.last_event = String::from(NO_SPEECH_EVENT);
            runtime.last_status = format!("No transcript: {reason}");
            runtime.clear_error_text();
            debug!(
                session_id = %session_id,
                reason = %reason,
                "recording transcription rejected"
            );
        },
    }
}

fn commit_transcript(
    runtime: &mut SidecarRuntime,
    session_id: String,
    text: String,
    backend: String,
) {
    runtime.clear_transcription_status();
    runtime.last_event = String::from(TRANSCRIPT_COMMITTED_EVENT);
    runtime.last_text = Some(text.clone());
    debug!(session_id = %session_id, backend = %backend, text = %text, "transcript committed");
}

fn queue_transcription(
    runtime: &mut SidecarRuntime,
    session_id: String,
    sample_rate: u32,
    samples: Vec<f32>,
) {
    let queued_transcription = QueuedTranscription {
        session_id,
        sample_rate,
        samples,
    };
    if runtime.pending_transcription_jobs.is_empty() {
        runtime.last_event = String::from(TRANSCRIPTION_STARTED_EVENT);
        spawn_transcription_job(runtime, queued_transcription);
        return;
    }
    if runtime.queued_transcriptions.len() < MAX_QUEUED_TRANSCRIPTIONS {
        runtime.last_event = format!(
            "Recording queued ({})",
            runtime.queued_transcriptions.len() + 1
        );
        debug!(
            session_id = %queued_transcription.session_id,
            queued = runtime.queued_transcriptions.len() + 1,
            "recording queued behind active transcription"
        );
        runtime
            .queued_transcriptions
            .push_back(queued_transcription);
        return;
    }
    warn!(
        session_id = %queued_transcription.session_id,
        queued = runtime.queued_transcriptions.len(),
        "dropping recording because transcription queue is full"
    );
    runtime.last_event = String::from(RECORDING_DROPPED_EVENT);
}

fn start_next_transcription(runtime: &mut SidecarRuntime) {
    if !runtime.pending_transcription_jobs.is_empty() {
        return;
    }
    let Some(queued_transcription) = runtime.queued_transcriptions.pop_front() else {
        return;
    };
    runtime.last_event = format!(
        "Transcribing queued recording; {} queued",
        runtime.queued_transcriptions.len()
    );
    spawn_transcription_job(runtime, queued_transcription);
}

fn spawn_transcription_job(
    runtime: &mut SidecarRuntime,
    queued_transcription: QueuedTranscription,
) {
    runtime
        .pending_transcription_jobs
        .push(PendingTranscriptionJob {
            transcription: spawn_transcription(TranscriptionRequest::new(
                queued_transcription.session_id,
                queued_transcription.sample_rate,
                queued_transcription.samples,
                runtime.paths.scratch_dir(),
            )),
        });
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
        .unwrap_or_else(|| String::from(NO_TRANSCRIPT_PLACEHOLDER));
    let recording = format!("{} ms", runtime.recording_ms());
    let level = format!("{:.3}", runtime.mic_rms);
    let queue = format!(
        "{} active / {} queued",
        runtime.pending_transcription_jobs.len(),
        runtime.queued_transcriptions.len()
    );
    set_status_field(&mut panel_text, *panel, FIELD_STATE, phase);
    set_status_field(&mut panel_text, *panel, FIELD_RECORDING, &recording);
    set_status_field(&mut panel_text, *panel, FIELD_EVENT, &event);
    set_status_field(&mut panel_text, *panel, FIELD_TRANSCRIPT, &transcript);
    set_status_field(&mut panel_text, *panel, FIELD_MIC, &level);
    set_status_field(&mut panel_text, *panel, FIELD_QUEUE, &queue);
}

fn interface_phase(runtime: &SidecarRuntime) -> InterfacePhase {
    if runtime.recording_session.is_some() {
        InterfacePhase::Recording
    } else if runtime.has_transcription_work() {
        InterfacePhase::Transcribing
    } else {
        InterfacePhase::Ready
    }
}

fn duration_ms(samples: usize, sample_rate: u32) -> u64 {
    let samples = u64::try_from(samples).map_or(u64::MAX, |value| value);
    samples.saturating_mul(MILLISECONDS_PER_SECOND) / u64::from(sample_rate.max(MIN_SAMPLE_RATE))
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples.iter().map(|sample| sample * sample).sum::<f32>();
    (sum / samples.len().to_f32()).sqrt()
}

fn set_status_field(panel_text: &mut PanelText, panel: Entity, field: &str, text: &str) {
    panel_text.set_text(panel, &PanelElementId::named(field), text);
}

fn prompt_face_panel(
    materials: &mut Assets<StandardMaterial>,
) -> Result<DiegeticPanel, PanelBuildError> {
    let mut cube_face_panel_style = CubeFacePanelStyle::for_cube(CUBE_SIZE);
    cube_face_panel_style.size *= FACE_PROMPT_SIZE_MULTIPLIER;
    cube_face_panel_style.padding *= FACE_PROMPT_PADDING_MULTIPLIER;
    cube_face_panel_style.title_size *= FACE_PROMPT_TITLE_SIZE_MULTIPLIER;
    cube_face_panel_style.color = FACE_PROMPT_COLOR;
    let transparent = materials.add(cube_face_panel_material());
    DiegeticPanel::world()
        .size(cube_face_panel_style.size, cube_face_panel_style.size)
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .material(transparent)
        .text_material(materials.add(prompt_text_material()))
        .with_tree(prompt_face_tree(cube_face_panel_style))
        .build()
}

fn prompt_text_material() -> StandardMaterial {
    let mut material = cube_face_panel_material();
    material.base_color = FACE_PROMPT_COLOR;
    material.emissive = FACE_PROMPT_COLOR.into();
    material
}

fn prompt_face_tree(cube_face_panel_style: CubeFacePanelStyle) -> LayoutTree {
    let mut layout_builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(cube_face_panel_style.size))
            .height(Sizing::fixed(cube_face_panel_style.size))
            .padding(Padding::all(cube_face_panel_style.padding))
            .alignment(AlignX::Center, AlignY::Center)
            .clip(),
    );
    layout_builder.text(
        Text::new(
            PROMPT_TEXT,
            TextStyle::new(cube_face_panel_style.title_size)
                .with_color(cube_face_panel_style.color)
                .with_align(TextAlign::Center)
                .with_shadow_mode(GlyphShadowMode::None),
        )
        .wrap(TextWrap::Words),
    );
    layout_builder.build()
}

fn status_panel(
    materials: &mut Assets<StandardMaterial>,
) -> Result<DiegeticPanel, PanelBuildError> {
    let unlit = materials.add(screen_panel_material());
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(status_panel_tree())
        .build()
}

fn status_panel_tree() -> LayoutTree {
    let mut layout_builder =
        LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut layout_builder,
        Sizing::fixed(STATUS_PANEL_WIDTH),
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |layout_builder| {
            layout_builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .gap(STATUS_PANEL_GAP),
                |layout_builder| {
                    layout_builder.text((STATUS_PANEL_TITLE, status_title_style()));
                    status_divider(layout_builder);
                    status_row(layout_builder, STATE_LABEL, FIELD_STATE, STATE_READY);
                    status_row(
                        layout_builder,
                        RECORDING_LABEL,
                        FIELD_RECORDING,
                        RECORDING_READY,
                    );
                    status_row(layout_builder, EVENT_LABEL, FIELD_EVENT, READY_EVENT);
                    status_row(
                        layout_builder,
                        TRANSCRIPT_LABEL,
                        FIELD_TRANSCRIPT,
                        NO_TRANSCRIPT_PLACEHOLDER,
                    );
                    status_row(layout_builder, MIC_LEVEL_LABEL, FIELD_MIC, MIC_LEVEL_READY);
                    status_row(layout_builder, QUEUE_LABEL, FIELD_QUEUE, QUEUE_READY);
                },
            );
        },
    );
    layout_builder.build()
}

fn status_row(
    layout_builder: &mut LayoutBuilder,
    label: &'static str,
    field: &'static str,
    value: &str,
) {
    layout_builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(STATUS_ROW_GAP)
            .align_y(AlignY::Top),
        |layout_builder| {
            layout_builder.with(
                El::new()
                    .width(Sizing::fixed(STATUS_LABEL_WIDTH))
                    .height(Sizing::FIT),
                |layout_builder| {
                    layout_builder.text((label, status_label_style()));
                },
            );
            layout_builder.with(
                El::new().width(Sizing::GROW).height(Sizing::FIT),
                |layout_builder| {
                    layout_builder.text(
                        Text::new(value, status_value_style(status_value_role(field)))
                            .id(PanelElementId::named(field))
                            .wrap(TextWrap::Words),
                    );
                },
            );
        },
    );
}

fn status_divider(layout_builder: &mut LayoutBuilder) {
    layout_builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(STATUS_DIVIDER_HEIGHT))
            .background(ACCENT_COLOR),
        |_| {},
    );
}

fn status_title_style() -> TextStyle {
    TextStyle::new(STATUS_TITLE_FONT_SIZE)
        .with_color(TEXT_COLOR)
        .bold()
        .with_shadow_mode(GlyphShadowMode::None)
}

fn status_label_style() -> TextStyle {
    TextStyle::new(STATUS_LABEL_FONT_SIZE)
        .with_color(MUTED_COLOR)
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
    TextStyle::new(STATUS_VALUE_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

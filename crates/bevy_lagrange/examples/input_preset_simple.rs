//! Spawns an `OrbitCam` with `OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse)`.
//! The cube faces show the preset's orbit / pan / zoom controls and light up
//! while pointer input is active.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor as PanelAnchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::InvalidSize;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlign;
use bevy_diegetic::Unit;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::UpsideDownPolicy;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::TitleBar;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .insert((CameraHomeTarget, SimpleMouseCube))
        .with_camera_home()
        .yaw(CAMERA_YAW)
        .pitch(CAMERA_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Simple Mouse")
                .with_anchor(Anchor::TopLeft)
                .control(CUBE_SPIN_CONTROL),
        )
        .wire_chip_to_state::<CubeSpinState, _>(CUBE_SPIN_CONTROL, |state| {
            state.cube_spin.control_activation()
        })
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .add_systems(PostStartup, spawn_face_labels)
        .init_resource::<CubeSpinState>()
        .insert_resource(FaceLabelHold::default())
        .add_systems(Update, (toggle_cube_spin, update_face_labels, spin_cube))
        .add_observer(capture_zoom_started)
        .run();
}

// Camera.
const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_PITCH_LIMIT: f32 = std::f32::consts::TAU / 3.0;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
const CAMERA_ZOOM_LOWER_LIMIT: f32 = 1.0;
const CAMERA_ZOOM_UPPER_LIMIT: f32 = 8.0;
const CUBE_SPIN_CONTROL: &str = "R Spin";
const HOME_MARGIN: f32 = 0.5;

#[derive(Resource)]
struct CubeSpinState {
    cube_spin: CubeSpin,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CubeSpin {
    Spinning,
    Paused,
}

impl CubeSpin {
    const fn control_activation(self) -> ControlActivation {
        match self {
            Self::Spinning => ControlActivation::Active,
            Self::Paused => ControlActivation::Inactive,
        }
    }

    const fn toggled(self) -> Self {
        match self {
            Self::Spinning => Self::Paused,
            Self::Paused => Self::Spinning,
        }
    }
}

impl Default for CubeSpinState {
    fn default() -> Self {
        Self {
            cube_spin: CubeSpin::Spinning,
        }
    }
}

#[derive(Component)]
struct SimpleMouseCamera;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        OrbitCam {
            focus: CAMERA_FOCUS,
            yaw: Some(CAMERA_YAW),
            pitch: Some(CAMERA_PITCH),
            radius: Some(CAMERA_RADIUS),
            pitch_upper_limit: Some(CAMERA_PITCH_LIMIT),
            pitch_lower_limit: Some(-CAMERA_PITCH_LIMIT),
            zoom_upper_limit: Some(CAMERA_ZOOM_UPPER_LIMIT),
            zoom_lower_limit: CAMERA_ZOOM_LOWER_LIMIT,
            upside_down_policy: UpsideDownPolicy::Allow,
            ..default()
        },
        OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        SimpleMouseCamera,
        FairyDustOrbitCam,
    ));
}

fn toggle_cube_spin(key_input: Res<ButtonInput<KeyCode>>, mut spin: ResMut<CubeSpinState>) {
    if key_input.just_pressed(KeyCode::KeyR) {
        spin.cube_spin = spin.cube_spin.toggled();
    }
}

// Cube face panels.
const FACE_LABEL_COLOR: Color = Color::srgb(0.1, 0.35, 1.0);
const FACE_LABEL_RELEASE_DELAY_SECS: f32 = 0.3;
const FACE_PANEL_ACTIVE_SIZE: f32 = 46.0;
const FACE_PANEL_BODY_SIZE: f32 = 40.0;
const FACE_PANEL_OFFSET: f32 = CUBE_SIZE * 0.5 + 0.006;
const FACE_PANEL_PADDING: f32 = 0.065;
const FACE_PANEL_ROW_GAP: f32 = 0.025;
const FACE_PANEL_SIZE: f32 = CUBE_SIZE * 0.88;
const FACE_PANEL_TITLE_SIZE: f32 = 66.0;

const ORBIT_LABELS: &[&str] = &["LMB drag"];
const PAN_LABELS: &[&str] = &["RMB drag"];
const ZOOM_LABELS: &[&str] = &["Wheel", "Trackpad", "Pinch"];

#[derive(Component)]
struct SimpleMouseCube;

#[derive(Component, Clone, Copy)]
enum FaceLabel {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Resource, Default)]
struct FaceLabelHold {
    orbit: HeldFaceLabel,
    pan:   HeldFaceLabel,
    zoom:  HeldFaceLabel,
}

#[derive(Default)]
struct HeldFaceLabel {
    remaining_secs: f32,
    lines:          Option<Vec<String>>,
}

impl HeldFaceLabel {
    fn hold_sources(&mut self, sources: CameraInteractionSources) {
        let Some(lines) = source_lines(sources) else {
            return;
        };
        self.remaining_secs = FACE_LABEL_RELEASE_DELAY_SECS;
        self.lines = Some(lines);
    }
}

#[derive(Clone)]
struct FacePanelContent {
    title:    &'static str,
    lines:    Vec<String>,
    activity: FacePanelActivity,
}

#[derive(Clone, Copy)]
enum FacePanelActivity {
    Active,
    Idle,
}

impl FacePanelContent {
    fn idle(title: &'static str, lines: &'static [&'static str]) -> Self {
        Self {
            title,
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
            activity: FacePanelActivity::Idle,
        }
    }

    const fn active(title: &'static str, lines: Vec<String>) -> Self {
        Self {
            title,
            lines,
            activity: FacePanelActivity::Active,
        }
    }
}

fn spawn_face_labels(mut commands: Commands, cubes: Query<Entity, With<SimpleMouseCube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            spawn_face_panel(
                parent,
                face,
                FaceLabel::Orbit,
                FacePanelContent::idle("Orbit", ORBIT_LABELS),
            );
        }
        for face in [Face::Left, Face::Right] {
            spawn_face_panel(
                parent,
                face,
                FaceLabel::Pan,
                FacePanelContent::idle("Pan", PAN_LABELS),
            );
        }
        for face in [Face::Top, Face::Bottom] {
            spawn_face_panel(
                parent,
                face,
                FaceLabel::Zoom,
                FacePanelContent::idle("Zoom", ZOOM_LABELS),
            );
        }
    });
}

fn update_face_labels(
    mut commands: Commands,
    time: Res<Time>,
    mut hold: ResMut<FaceLabelHold>,
    cameras: Query<&OrbitCamInteractionState, With<SimpleMouseCamera>>,
    labels: Query<(Entity, &FaceLabel)>,
) {
    let Ok(interaction_state) = cameras.single() else {
        return;
    };

    let orbit = held_content(
        &mut hold.orbit,
        time.delta_secs(),
        source_lines(interaction_state.orbit_sources()),
        ORBIT_LABELS,
        "Orbit",
    );
    let pan = held_content(
        &mut hold.pan,
        time.delta_secs(),
        source_lines(interaction_state.pan_sources()),
        PAN_LABELS,
        "Pan",
    );
    let zoom = held_content(
        &mut hold.zoom,
        time.delta_secs(),
        source_lines(interaction_state.zoom_sources()),
        ZOOM_LABELS,
        "Zoom",
    );

    for (entity, kind) in &labels {
        let next = match kind {
            FaceLabel::Orbit => orbit.clone(),
            FaceLabel::Pan => pan.clone(),
            FaceLabel::Zoom => zoom.clone(),
        };
        commands.set_tree(entity, build_face_panel_tree(next));
    }
}

fn capture_zoom_started(
    event: On<OrbitCamInteractionStarted>,
    cameras: Query<(), With<SimpleMouseCamera>>,
    mut hold: ResMut<FaceLabelHold>,
) {
    if event.kind != OrbitCamInteractionKind::Zoom || cameras.get(event.camera).is_err() {
        return;
    }
    hold.zoom.hold_sources(event.sources);
}

fn held_content(
    hold: &mut HeldFaceLabel,
    delta_secs: f32,
    active_lines: Option<Vec<String>>,
    idle_lines: &'static [&'static str],
    title: &'static str,
) -> FacePanelContent {
    if let Some(lines) = active_lines {
        hold.remaining_secs = FACE_LABEL_RELEASE_DELAY_SECS;
        hold.lines = Some(lines);
        return FacePanelContent::active(title, hold.lines.clone().unwrap_or_default());
    }

    hold.remaining_secs = (hold.remaining_secs - delta_secs).max(0.0);
    if hold.remaining_secs > 0.0 {
        return FacePanelContent::active(title, hold.lines.clone().unwrap_or_default());
    }

    hold.lines = None;
    FacePanelContent::idle(title, idle_lines)
}

fn source_lines(sources: CameraInteractionSources) -> Option<Vec<String>> {
    let mut labels = Vec::new();
    if sources.contains(CameraInteractionSources::MOUSE) {
        labels.push("Mouse".to_string());
    }
    if sources.contains(CameraInteractionSources::WHEEL) {
        labels.push("Wheel".to_string());
    }
    if sources.contains(CameraInteractionSources::SMOOTH_SCROLL) {
        labels.push("Trackpad".to_string());
    }
    if sources.contains(CameraInteractionSources::PINCH) {
        labels.push("Pinch".to_string());
    }
    (!labels.is_empty()).then_some(labels)
}

fn spawn_face_panel(
    parent: &mut ChildSpawnerCommands,
    face: Face,
    kind: FaceLabel,
    content: FacePanelContent,
) {
    match face_panel(content) {
        Ok(panel) => {
            parent.spawn((
                Name::new("Simple mouse face panel"),
                kind,
                panel,
                face_panel_transform(face),
            ));
        },
        Err(error) => {
            error!("input_preset_simple: failed to build cube face panel: {error}");
        },
    }
}

fn face_panel_transform(face: Face) -> Transform {
    match face {
        Face::Front => Transform::from_xyz(0.0, 0.0, FACE_PANEL_OFFSET),
        Face::Back => Transform::from_xyz(0.0, 0.0, -FACE_PANEL_OFFSET)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        Face::Right => Transform::from_xyz(FACE_PANEL_OFFSET, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        Face::Left => Transform::from_xyz(-FACE_PANEL_OFFSET, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        Face::Top => Transform::from_xyz(0.0, FACE_PANEL_OFFSET, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Face::Bottom => Transform::from_xyz(0.0, -FACE_PANEL_OFFSET, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
    }
}

fn face_panel(content: FacePanelContent) -> Result<DiegeticPanel, InvalidSize> {
    let transparent = face_panel_material();
    DiegeticPanel::world()
        .size(FACE_PANEL_SIZE, FACE_PANEL_SIZE)
        .font_unit(Unit::Millimeters)
        .anchor(PanelAnchor::Center)
        .material(transparent.clone())
        .text_material(transparent)
        .with_tree(build_face_panel_tree(content))
        .build()
}

fn face_panel_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

fn build_face_panel_tree(content: FacePanelContent) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(FACE_PANEL_SIZE))
            .height(Sizing::fixed(FACE_PANEL_SIZE))
            .direction(Direction::TopToBottom)
            .child_alignment(AlignX::Center, AlignY::Center)
            .child_gap(FACE_PANEL_ROW_GAP)
            .padding(Padding::all(FACE_PANEL_PADDING))
            .clip(),
    );

    builder.text(
        content.title,
        LayoutTextStyle::new(FACE_PANEL_TITLE_SIZE)
            .with_color(FACE_LABEL_COLOR)
            .with_align(TextAlign::Center)
            .with_shadow_mode(GlyphShadowMode::None),
    );

    let body_size = match content.activity {
        FacePanelActivity::Active => FACE_PANEL_ACTIVE_SIZE,
        FacePanelActivity::Idle => FACE_PANEL_BODY_SIZE,
    };
    let body = LayoutTextStyle::new(body_size)
        .with_color(FACE_LABEL_COLOR)
        .with_align(TextAlign::Center)
        .with_shadow_mode(GlyphShadowMode::None);

    for line in content.lines {
        builder.text(line, body.clone());
    }

    builder.build()
}

// Scene.
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const CUBE_SPIN_SPEED: f32 = 0.2;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + CUBE_GROUND_CLEARANCE, 0.0);
const GROUND_SIZE: f32 = 5.0;

fn spin_cube(
    time: Res<Time>,
    spin: Res<CubeSpinState>,
    mut cubes: Query<&mut Transform, With<SimpleMouseCube>>,
) {
    match spin.cube_spin {
        CubeSpin::Spinning => {},
        CubeSpin::Paused => return,
    }
    for mut transform in &mut cubes {
        transform.rotate_y(CUBE_SPIN_SPEED * time.delta_secs());
    }
}

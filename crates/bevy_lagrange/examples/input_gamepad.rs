//! Spawns an `OrbitCam` with `OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad)`
//! and wires `GamepadButton::South` to an `AnimateToFit` home animation. The
//! `GamepadHomeBegin` / `GamepadHomeEnd` events drive the title-bar chip via
//! `wire_chip_to_events`, and a `GamepadConnection` resource drives the
//! connection chip via `wire_chip_to_state`. Cube faces show the preset's
//! orbit / pan / zoom controls and light up while sticks and triggers move.
//!
//! Controls:
//!   Orbit — right stick (RB + RS for slow)
//!   Pan   — left stick (LB + LS for slow)
//!   Zoom  — RT in / LT out (RB + RT, LB + LT for slow)
//!   South — fly camera home via `AnimateToFit`

use std::time::Duration;

use bevy::input::gamepad::Gamepad;
use bevy::input::gamepad::GamepadButton;
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
use bevy_kana::event;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::UpsideDownPolicy;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeEntity;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::TitleBar;

fn main() {
    fairy_dust::sprinkle_example()
        .insert_resource(
            CameraInputRoutingConfig::cursor_hit_test()
                .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
        )
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .insert((CameraHomeTarget, GamepadInputCube))
        .with_camera_home()
        .yaw(CAMERA_YAW)
        .pitch(CAMERA_PITCH)
        .margin(HOME_MARGIN)
        .without_title_bar_control()
        .with_title_bar(
            TitleBar::new()
                .with_title("Gamepad")
                .with_anchor(Anchor::TopLeft)
                .control(CUBE_SPIN_CONTROL)
                .control(GAMEPAD_HOME_CONTROL)
                .control(GAMEPAD_CONNECTED_CONTROL),
        )
        .wire_chip_to_state::<CubeSpinState, _>(CUBE_SPIN_CONTROL, |state| {
            state.cube_spin.control_activation()
        })
        .wire_chip_to_state::<GamepadConnection, _>(GAMEPAD_CONNECTED_CONTROL, |connection| {
            connection.control_activation
        })
        .wire_chip_to_events::<GamepadHomeBegin, GamepadHomeEnd>(GAMEPAD_HOME_CONTROL)
        .init_resource::<GamepadConnection>()
        .init_resource::<GamepadHomeAnimation>()
        .init_resource::<CubeSpinState>()
        .insert_resource(FaceLabelHold::default())
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .add_systems(PostStartup, spawn_face_labels)
        .add_systems(
            Update,
            (
                update_gamepad_connection,
                toggle_cube_spin,
                update_face_labels,
                spin_cube,
                home_on_gamepad_south,
            ),
        )
        .add_observer(finish_gamepad_home)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// GAMEPAD CAMERA — OrbitCamPreset::Gamepad + AnimateToFit on GamepadButton::South.
//
// How it works:
//   1. `spawn_camera` installs `OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad)` so Lagrange
//      reads the gamepad sticks and triggers for orbit, pan, and zoom.
//   2. `update_gamepad_connection` polls `Query<&Gamepad>` and updates `GamepadConnection`; the
//      connection chip is driven through `wire_chip_to_state`.
//   3. `home_on_gamepad_south` watches `GamepadButton::South`, triggers `AnimateToFit` to fly the
//      camera back to the home pose, and fires `GamepadHomeBegin` so the home chip lights up.
//   4. `finish_gamepad_home` observes `AnimationEnd` from `AnimationSource::AnimateToFit` and fires
//      `GamepadHomeEnd` so the home chip turns off when the fly finishes.
// ═════════════════════════════════════════════════════════════════════════════

const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_PITCH_LIMIT: f32 = std::f32::consts::TAU / 3.0;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
const CAMERA_ZOOM_LOWER_LIMIT: f32 = 1.0;
const CAMERA_ZOOM_UPPER_LIMIT: f32 = 8.0;
const CUBE_SPIN_CONTROL: &str = "R Spin";
const GAMEPAD_CONNECTED_CONTROL: &str = "Gamepad Connected";
const GAMEPAD_HOME_CONTROL: &str = "GamepadButton::South - Home";
const GAMEPAD_HOME_DURATION: Duration = Duration::from_millis(800);
const HOME_MARGIN: f32 = 0.5;

#[derive(Resource)]
struct GamepadConnection {
    control_activation: ControlActivation,
}

impl Default for GamepadConnection {
    fn default() -> Self {
        Self {
            control_activation: ControlActivation::Inactive,
        }
    }
}

#[derive(Resource)]
struct GamepadHomeAnimation {
    control_activation: ControlActivation,
}

impl Default for GamepadHomeAnimation {
    fn default() -> Self {
        Self {
            control_activation: ControlActivation::Inactive,
        }
    }
}

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

event!(
    /// Fires when the gamepad south face button starts a home animation.
    GamepadHomeBegin
);
event!(
    /// Fires when the gamepad south-button home animation ends.
    GamepadHomeEnd
);

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
        OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad),
        FairyDustOrbitCam,
    ));
}

fn update_gamepad_connection(
    gamepads: Query<(), With<Gamepad>>,
    mut connection: ResMut<GamepadConnection>,
) {
    let control_activation = if gamepads.is_empty() {
        ControlActivation::Inactive
    } else {
        ControlActivation::Active
    };
    if connection.control_activation != control_activation {
        connection.control_activation = control_activation;
    }
}

fn home_on_gamepad_south(
    gamepads: Query<&Gamepad>,
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    mut gamepad_home: ResMut<GamepadHomeAnimation>,
) {
    if !gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::South))
    {
        return;
    }

    let Some(home) = home else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };

    gamepad_home.control_activation = ControlActivation::Active;
    commands.trigger(GamepadHomeBegin);
    commands.trigger(
        AnimateToFit::new(camera, home.0)
            .yaw(CAMERA_YAW)
            .pitch(CAMERA_PITCH)
            .margin(HOME_MARGIN)
            .duration(GAMEPAD_HOME_DURATION),
    );
}

fn finish_gamepad_home(
    event: On<AnimationEnd>,
    mut commands: Commands,
    mut gamepad_home: ResMut<GamepadHomeAnimation>,
) {
    if gamepad_home.control_activation != ControlActivation::Active
        || event.source != AnimationSource::AnimateToFit
    {
        return;
    }
    gamepad_home.control_activation = ControlActivation::Inactive;
    commands.trigger(GamepadHomeEnd);
}

fn toggle_cube_spin(key_input: Res<ButtonInput<KeyCode>>, mut spin: ResMut<CubeSpinState>) {
    if key_input.just_pressed(KeyCode::KeyR) {
        spin.cube_spin = spin.cube_spin.toggled();
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE PANELS — gamepad controls grouped by camera action.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_LABEL_COLOR: Color = Color::srgb(0.1, 0.35, 1.0);
const FACE_LABEL_RELEASE_DELAY_SECS: f32 = 0.3;
const FACE_PANEL_ACTIVE_SIZE: f32 = 56.0;
const FACE_PANEL_BODY_SIZE: f32 = 48.0;
const FACE_PANEL_OFFSET: f32 = CUBE_SIZE * 0.5 + 0.006;
const FACE_PANEL_PADDING: f32 = 0.06;
const FACE_PANEL_ROW_GAP: f32 = 0.02;
const FACE_PANEL_SIZE: f32 = CUBE_SIZE * 0.88;
const FACE_PANEL_TITLE_SIZE: f32 = 78.0;
const STICK_ACTIVE_THRESHOLD: f32 = 0.18;
const TRIGGER_ACTIVE_THRESHOLD: f32 = 0.05;

const ORBIT_LABELS: &[&str] = &["RS", "RB + RS slow"];
const PAN_LABELS: &[&str] = &["LS", "LB + LS slow"];
const ZOOM_LABELS: &[&str] = &[
    "RT zoom in",
    "LT zoom out",
    "RB + RT slow in",
    "LB + LT slow out",
];

#[derive(Component)]
struct GamepadInputCube;

#[derive(Component, Clone, Copy)]
enum GamepadFaceLabel {
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
    content:        Option<FacePanelContent>,
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

fn spawn_face_labels(mut commands: Commands, cubes: Query<Entity, With<GamepadInputCube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            spawn_face_panel(
                parent,
                face,
                GamepadFaceLabel::Orbit,
                FacePanelContent::idle("Orbit", ORBIT_LABELS),
            );
        }
        for face in [Face::Left, Face::Right] {
            spawn_face_panel(
                parent,
                face,
                GamepadFaceLabel::Pan,
                FacePanelContent::idle("Pan", PAN_LABELS),
            );
        }
        for face in [Face::Top, Face::Bottom] {
            spawn_face_panel(
                parent,
                face,
                GamepadFaceLabel::Zoom,
                FacePanelContent::idle("Zoom", ZOOM_LABELS),
            );
        }
    });
}

fn update_face_labels(
    mut commands: Commands,
    time: Res<Time>,
    gamepads: Query<&Gamepad>,
    mut hold: ResMut<FaceLabelHold>,
    labels: Query<(Entity, &GamepadFaceLabel)>,
) {
    let active_gamepad = gamepads
        .iter()
        .find(|gamepad| gamepad_has_input(gamepad))
        .or_else(|| gamepads.iter().next());

    let orbit = held_content(
        &mut hold.orbit,
        time.delta_secs(),
        active_orbit_content(active_gamepad),
        "Orbit",
        ORBIT_LABELS,
    );
    let pan = held_content(
        &mut hold.pan,
        time.delta_secs(),
        active_pan_content(active_gamepad),
        "Pan",
        PAN_LABELS,
    );
    let zoom = held_content(
        &mut hold.zoom,
        time.delta_secs(),
        active_zoom_content(active_gamepad),
        "Zoom",
        ZOOM_LABELS,
    );

    for (entity, label) in &labels {
        let content = match label {
            GamepadFaceLabel::Orbit => orbit.clone(),
            GamepadFaceLabel::Pan => pan.clone(),
            GamepadFaceLabel::Zoom => zoom.clone(),
        };
        commands.set_tree(entity, build_face_panel_tree(content));
    }
}

fn spawn_face_panel(
    parent: &mut ChildSpawnerCommands,
    face: Face,
    kind: GamepadFaceLabel,
    content: FacePanelContent,
) {
    match face_panel(content) {
        Ok(panel) => {
            parent.spawn((
                Name::new("Gamepad input face panel"),
                kind,
                panel,
                face_panel_transform(face),
            ));
        },
        Err(error) => {
            error!("input_gamepad: failed to build cube face panel: {error}");
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

fn gamepad_has_input(gamepad: &Gamepad) -> bool {
    gamepad.right_stick().length() > STICK_ACTIVE_THRESHOLD
        || gamepad.left_stick().length() > STICK_ACTIVE_THRESHOLD
        || trigger_value(gamepad, GamepadButton::RightTrigger2) > TRIGGER_ACTIVE_THRESHOLD
        || trigger_value(gamepad, GamepadButton::LeftTrigger2) > TRIGGER_ACTIVE_THRESHOLD
        || gamepad.pressed(GamepadButton::South)
}

fn active_orbit_content(gamepad: Option<&Gamepad>) -> Option<FacePanelContent> {
    let gamepad = gamepad?;
    let stick = gamepad.right_stick();
    if stick.length() <= STICK_ACTIVE_THRESHOLD {
        return None;
    }

    let control = if gamepad.pressed(GamepadButton::RightTrigger) {
        "RB + RS"
    } else {
        "RS"
    };
    Some(FacePanelContent::active(
        "Orbit",
        vec![control.to_string(), stick_direction(stick)],
    ))
}

fn active_pan_content(gamepad: Option<&Gamepad>) -> Option<FacePanelContent> {
    let gamepad = gamepad?;
    let stick = gamepad.left_stick();
    if stick.length() <= STICK_ACTIVE_THRESHOLD {
        return None;
    }

    let control = if gamepad.pressed(GamepadButton::LeftTrigger) {
        "LB + LS"
    } else {
        "LS"
    };
    Some(FacePanelContent::active(
        "Pan",
        vec![control.to_string(), stick_direction(stick)],
    ))
}

fn active_zoom_content(gamepad: Option<&Gamepad>) -> Option<FacePanelContent> {
    let gamepad = gamepad?;

    let mut lines = Vec::new();
    if trigger_value(gamepad, GamepadButton::RightTrigger2) > TRIGGER_ACTIVE_THRESHOLD {
        let label = if gamepad.pressed(GamepadButton::RightTrigger) {
            "RB + RT slow in"
        } else {
            "RT zoom in"
        };
        lines.push(label.to_string());
    }
    if trigger_value(gamepad, GamepadButton::LeftTrigger2) > TRIGGER_ACTIVE_THRESHOLD {
        let label = if gamepad.pressed(GamepadButton::LeftTrigger) {
            "LB + LT slow out"
        } else {
            "LT zoom out"
        };
        lines.push(label.to_string());
    }

    if lines.is_empty() {
        None
    } else {
        Some(FacePanelContent::active("Zoom", lines))
    }
}

fn held_content(
    hold: &mut HeldFaceLabel,
    delta_secs: f32,
    active: Option<FacePanelContent>,
    title: &'static str,
    idle_lines: &'static [&'static str],
) -> FacePanelContent {
    if let Some(active) = active {
        hold.remaining_secs = FACE_LABEL_RELEASE_DELAY_SECS;
        hold.content = Some(active.clone());
        return active;
    }

    hold.remaining_secs = (hold.remaining_secs - delta_secs).max(0.0);
    if hold.remaining_secs > 0.0 {
        return hold
            .content
            .clone()
            .unwrap_or_else(|| FacePanelContent::idle(title, idle_lines));
    }

    hold.content = None;
    FacePanelContent::idle(title, idle_lines)
}

fn trigger_value(gamepad: &Gamepad, button: GamepadButton) -> f32 {
    gamepad.get(button).unwrap_or(0.0)
}

fn stick_direction(stick: Vec2) -> String {
    let mut parts = Vec::new();
    if stick.y > STICK_ACTIVE_THRESHOLD {
        parts.push("up");
    } else if stick.y < -STICK_ACTIVE_THRESHOLD {
        parts.push("down");
    }
    if stick.x > STICK_ACTIVE_THRESHOLD {
        parts.push("right");
    } else if stick.x < -STICK_ACTIVE_THRESHOLD {
        parts.push("left");
    }
    parts.join(" + ")
}

// ═════════════════════════════════════════════════════════════════════════════
// SCENE SCAFFOLDING — cube body and ground sized to match.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const CUBE_SPIN_SPEED: f32 = 0.2;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + CUBE_GROUND_CLEARANCE, 0.0);

const GROUND_SIZE: f32 = 5.0;

fn spin_cube(
    time: Res<Time>,
    spin: Res<CubeSpinState>,
    mut cubes: Query<&mut Transform, With<GamepadInputCube>>,
) {
    match spin.cube_spin {
        CubeSpin::Spinning => {},
        CubeSpin::Paused => return,
    }
    for mut transform in &mut cubes {
        transform.rotate_y(CUBE_SPIN_SPEED * time.delta_secs());
    }
}

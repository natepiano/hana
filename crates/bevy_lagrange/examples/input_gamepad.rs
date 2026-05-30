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
use bevy_diegetic::DiegeticPanelCommands;
use bevy_kana::event;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamControlSummary;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::describe_orbit_cam_controls;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeEntity;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::CubeFacePanelContent;
use fairy_dust::CubeFacePanelStyle;
use fairy_dust::CubeSpinConfig;
use fairy_dust::CubeSpinControl;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::HoldState;
use fairy_dust::ReleaseHold;
use fairy_dust::TitleBar;
use fairy_dust::TitleChip;
use fairy_dust::TitleChipActivation;
use fairy_dust::apply_example_orbit_cam_limits;
use fairy_dust::cube_face_panel;
use fairy_dust::cube_face_panel_tree;
use fairy_dust::cube_face_transform;

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
                .control(GAMEPAD_HOME_CONTROL)
                .control(GAMEPAD_CONNECTED_CONTROL),
        )
        .wire_chip_to_activation::<GamepadConnection>(GAMEPAD_CONNECTED_CONTROL)
        .wire_chip_to_events::<GamepadHomeBegin, GamepadHomeEnd>(GAMEPAD_HOME_CONTROL)
        .with_cube_spin_config::<GamepadInputCube>(CubeSpinConfig::new().without_key().with_chip(
            TitleChip::new("cube_spin_pause", "GamepadButton::West - Pause"),
        ))
        .init_resource::<GamepadConnection>()
        .init_resource::<GamepadHomeAnimation>()
        .insert_resource(FaceLabelHold::default())
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .add_systems(PostStartup, spawn_face_labels)
        .add_systems(
            Update,
            (
                update_gamepad_connection,
                update_face_labels,
                home_on_gamepad_south,
                toggle_spin_on_gamepad_west,
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
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
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

impl TitleChipActivation for GamepadConnection {
    fn activation(&self) -> ControlActivation { self.control_activation }
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

event!(
    /// Fires when the gamepad south face button starts a home animation.
    GamepadHomeBegin
);
event!(
    /// Fires when the gamepad south-button home animation ends.
    GamepadHomeEnd
);

fn spawn_camera(mut commands: Commands) {
    let mut camera = OrbitCam {
        focus: CAMERA_FOCUS,
        yaw: Some(CAMERA_YAW),
        pitch: Some(CAMERA_PITCH),
        radius: Some(CAMERA_RADIUS),
        ..default()
    };
    apply_example_orbit_cam_limits(&mut camera);
    commands.spawn((
        camera,
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

fn toggle_spin_on_gamepad_west(
    gamepads: Query<&Gamepad>,
    mut control: ResMut<CubeSpinControl<GamepadInputCube>>,
) {
    if gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::West))
    {
        control.toggle();
    }
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

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE PANELS — gamepad controls grouped by camera action.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_PANEL_STYLE: CubeFacePanelStyle = CubeFacePanelStyle {
    title_size: 78.0,
    body_size: 48.0,
    active_body_size: 56.0,
    ..CubeFacePanelStyle::for_cube(CUBE_SIZE)
};
const STICK_ACTIVE_THRESHOLD: f32 = 0.18;
const TRIGGER_ACTIVE_THRESHOLD: f32 = 0.05;

#[derive(Component)]
struct GamepadInputCube;

#[derive(Component, Clone, Copy)]
enum GamepadFaceLabel {
    Orbit,
    Pan,
    Zoom,
}

impl GamepadFaceLabel {
    const fn kind(self) -> OrbitCamInteractionKind {
        match self {
            Self::Orbit => OrbitCamInteractionKind::Orbit,
            Self::Pan => OrbitCamInteractionKind::Pan,
            Self::Zoom => OrbitCamInteractionKind::Zoom,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Orbit => "Orbit",
            Self::Pan => "Pan",
            Self::Zoom => "Zoom",
        }
    }
}

/// Holds the preset's described controls so idle face labels share the camera
/// control panel's vocabulary; live labels still show the active stick/trigger.
#[derive(Resource)]
struct FaceGuidance(OrbitCamControlSummary);

#[derive(Resource, Default)]
struct FaceLabelHold {
    orbit: ReleaseHold<CubeFacePanelContent>,
    pan:   ReleaseHold<CubeFacePanelContent>,
    zoom:  ReleaseHold<CubeFacePanelContent>,
}

fn spawn_face_labels(mut commands: Commands, cubes: Query<Entity, With<GamepadInputCube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad));
    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            spawn_face_panel(parent, face, GamepadFaceLabel::Orbit, &summary);
        }
        for face in [Face::Left, Face::Right] {
            spawn_face_panel(parent, face, GamepadFaceLabel::Pan, &summary);
        }
        for face in [Face::Top, Face::Bottom] {
            spawn_face_panel(parent, face, GamepadFaceLabel::Zoom, &summary);
        }
    });
    commands.insert_resource(FaceGuidance(summary));
}

fn update_face_labels(
    mut commands: Commands,
    time: Res<Time>,
    gamepads: Query<&Gamepad>,
    cameras: Query<&OrbitCamInteractionState, With<FairyDustOrbitCam>>,
    mut hold: ResMut<FaceLabelHold>,
    guidance: Res<FaceGuidance>,
    labels: Query<(Entity, &GamepadFaceLabel)>,
) {
    let active_gamepad = gamepads
        .iter()
        .find(|gamepad| gamepad_has_input(gamepad))
        .or_else(|| gamepads.iter().next());
    // The engine's resolved speed is the single source of truth for "Slow" —
    // the example never re-reads the rb/lb gate buttons itself.
    let state = cameras.iter().next().copied().unwrap_or_default();

    let orbit = held_content(
        &mut hold.orbit,
        time.delta(),
        active_orbit_content(active_gamepad, state.speed(OrbitCamInteractionKind::Orbit)),
        &guidance.0,
        GamepadFaceLabel::Orbit,
    );
    let pan = held_content(
        &mut hold.pan,
        time.delta(),
        active_pan_content(active_gamepad, state.speed(OrbitCamInteractionKind::Pan)),
        &guidance.0,
        GamepadFaceLabel::Pan,
    );
    let zoom = held_content(
        &mut hold.zoom,
        time.delta(),
        active_zoom_content(active_gamepad, state.speed(OrbitCamInteractionKind::Zoom)),
        &guidance.0,
        GamepadFaceLabel::Zoom,
    );

    for (entity, label) in &labels {
        let content = match label {
            GamepadFaceLabel::Orbit => orbit.clone(),
            GamepadFaceLabel::Pan => pan.clone(),
            GamepadFaceLabel::Zoom => zoom.clone(),
        };
        commands.set_tree(entity, cube_face_panel_tree(FACE_PANEL_STYLE, content));
    }
}

fn spawn_face_panel(
    parent: &mut ChildSpawnerCommands,
    face: Face,
    kind: GamepadFaceLabel,
    summary: &OrbitCamControlSummary,
) {
    let content = CubeFacePanelContent::idle(kind.title(), idle_labels(summary, kind.kind()));
    match cube_face_panel(FACE_PANEL_STYLE, content) {
        Ok(panel) => {
            parent.spawn((
                Name::new("Gamepad input face panel"),
                kind,
                panel,
                cube_face_transform(face, CUBE_SIZE),
            ));
        },
        Err(error) => {
            error!("input_gamepad: failed to build cube face panel: {error}");
        },
    }
}

fn gamepad_has_input(gamepad: &Gamepad) -> bool {
    gamepad.right_stick().length() > STICK_ACTIVE_THRESHOLD
        || gamepad.left_stick().length() > STICK_ACTIVE_THRESHOLD
        || trigger_value(gamepad, GamepadButton::RightTrigger2) > TRIGGER_ACTIVE_THRESHOLD
        || trigger_value(gamepad, GamepadButton::LeftTrigger2) > TRIGGER_ACTIVE_THRESHOLD
        || gamepad.pressed(GamepadButton::South)
}

fn active_orbit_content(
    gamepad: Option<&Gamepad>,
    speed: ControlSpeed,
) -> Option<CubeFacePanelContent> {
    let gamepad = gamepad?;
    let stick = gamepad.right_stick();
    if stick.length() <= STICK_ACTIVE_THRESHOLD {
        return None;
    }

    let slow = speed == ControlSpeed::Slow;
    let control = if slow { "rb+rs" } else { "rs" };
    Some(CubeFacePanelContent::active(
        slow_title("Orbit", slow),
        vec![control.to_string(), stick_direction(stick)],
    ))
}

fn active_pan_content(
    gamepad: Option<&Gamepad>,
    speed: ControlSpeed,
) -> Option<CubeFacePanelContent> {
    let gamepad = gamepad?;
    let stick = gamepad.left_stick();
    if stick.length() <= STICK_ACTIVE_THRESHOLD {
        return None;
    }

    let slow = speed == ControlSpeed::Slow;
    let control = if slow { "lb+ls" } else { "ls" };
    Some(CubeFacePanelContent::active(
        slow_title("Pan", slow),
        vec![control.to_string(), stick_direction(stick)],
    ))
}

fn active_zoom_content(
    gamepad: Option<&Gamepad>,
    speed: ControlSpeed,
) -> Option<CubeFacePanelContent> {
    let gamepad = gamepad?;
    let slow = speed == ControlSpeed::Slow;

    let mut lines = Vec::new();
    if trigger_value(gamepad, GamepadButton::RightTrigger2) > TRIGGER_ACTIVE_THRESHOLD {
        lines.push(if slow { "rb+rt" } else { "rt" }.to_string());
    }
    if trigger_value(gamepad, GamepadButton::LeftTrigger2) > TRIGGER_ACTIVE_THRESHOLD {
        lines.push(if slow { "lb+lt" } else { "lt" }.to_string());
    }

    if lines.is_empty() {
        None
    } else {
        Some(CubeFacePanelContent::active(
            slow_title("Zoom", slow),
            lines,
        ))
    }
}

/// Appends a `Slow` suffix to a face title when the slow gate is engaged.
fn slow_title(base: &str, slow: bool) -> String {
    if slow {
        format!("{base} Slow")
    } else {
        base.to_string()
    }
}

fn held_content(
    hold: &mut ReleaseHold<CubeFacePanelContent>,
    delta: std::time::Duration,
    active: Option<CubeFacePanelContent>,
    summary: &OrbitCamControlSummary,
    label: GamepadFaceLabel,
) -> CubeFacePanelContent {
    match hold.update(delta, active) {
        HoldState::Active(content) | HoldState::Held(content) => content.clone(),
        HoldState::Idle => {
            CubeFacePanelContent::idle(label.title(), idle_labels(summary, label.kind()))
        },
    }
}

/// All control labels configured for `kind`, shown while idle.
fn idle_labels(summary: &OrbitCamControlSummary, kind: OrbitCamInteractionKind) -> Vec<String> {
    summary
        .rows
        .iter()
        .filter(|row| row.kind == kind)
        .map(|row| row.label.clone())
        .collect()
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

const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = fairy_dust::example_cube_on_ground(CUBE_GROUND_CLEARANCE);

const GROUND_SIZE: f32 = fairy_dust::EXAMPLE_GROUND_SIZE;

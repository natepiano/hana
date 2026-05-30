//! Demonstrates choosing the `OrbitCam` time source so pausing virtual game
//! time does not pause camera controls. `configure_camera` sets
//! `OrbitCam::time_source = TimeSource::Real`; `pause_game_system` toggles
//! `Time<Virtual>`; `cube_rotator_system` reads the default `Res<Time>` which
//! resolves to virtual time inside `Update`, so the cube freezes while the
//! camera keeps lerping.
//!
//! Controls:
//!   P or Space — toggle game pause

use bevy::prelude::*;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::TimeSource;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::CubeFacePanelContent;
use fairy_dust::CubeFacePanelStyle;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;
use fairy_dust::TitleChipActivation;
use fairy_dust::cube_face_panel;
use fairy_dust::cube_face_panel_tree;
use fairy_dust::cube_face_transform;

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
        .insert((Cube, CameraHomeTarget))
        .with_orbit_cam_preset(configure_camera, OrbitCamPreset::BlenderLike)
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Pausing")
                .with_anchor(Anchor::TopLeft)
                .control(PAUSE_CONTROL),
        )
        .wire_chip_to_activation::<PauseState>(PAUSE_CONTROL)
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .init_resource::<PauseState>()
        .add_systems(PostStartup, spawn_face_panels)
        // Chained so the pause toggle, panel refresh, and cube rotation all
        // observe the same `Time<Virtual>` state within a single frame.
        .add_systems(
            Update,
            (pause_game_system, update_game_panels, cube_rotator_system).chain(),
        )
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// CAMERA TIME SOURCE & VIRTUAL-TIME PAUSE — the demonstrated API.
// `OrbitCam::time_source = TimeSource::Real` plus `Time<Virtual>` pause is what
// makes the camera keep moving while the cube freezes.
//
// How it works:
//   1. `configure_camera` runs when the OrbitCam spawns and sets `time_source = TimeSource::Real`
//      so the camera's smoothing reads wall-clock time.
//   2. `pause_game_system` toggles `Time<Virtual>` on P / Space.
//   3. `cube_rotator_system` reads the default `Res<Time>`, which resolves to `Time<Virtual>`
//      inside `Update`, so its delta is zero while paused and the cube stops spinning. The camera,
//      reading real time, keeps lerping.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_ROTATION_SPEED: f32 = 0.8;
const PAUSE_CONTROL: &str = "P Pause";

#[derive(Component)]
struct Cube;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PauseState {
    #[default]
    Running,
    Paused,
}

impl PauseState {
    const fn control_activation(self) -> ControlActivation {
        match self {
            Self::Running => ControlActivation::Inactive,
            Self::Paused => ControlActivation::Active,
        }
    }
}

impl TitleChipActivation for PauseState {
    fn activation(&self) -> ControlActivation { self.control_activation() }
}

const fn configure_camera(camera: &mut OrbitCam) { camera.time_source = TimeSource::Real; }

fn pause_game_system(
    key_input: Res<ButtonInput<KeyCode>>,
    mut time: ResMut<Time<Virtual>>,
    mut pause_state: ResMut<PauseState>,
) {
    if key_input.just_pressed(KeyCode::KeyP) || key_input.just_pressed(KeyCode::Space) {
        if time.is_paused() {
            time.unpause();
        } else {
            time.pause();
        }
        *pause_state = if time.is_paused() {
            PauseState::Paused
        } else {
            PauseState::Running
        };
    }
}

// `Res<Time>` resolves to `Time<Virtual>` in `Update`, so this freezes while
// the game is paused.
fn cube_rotator_system(time: Res<Time>, mut query: Query<&mut Transform, With<Cube>>) {
    for mut transform in &mut query {
        transform.rotate_y(CUBE_ROTATION_SPEED * time.delta_secs());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// SCENE SCAFFOLDING — cube body, ground, and camera home placement.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = fairy_dust::example_cube_on_ground(CUBE_GROUND_CLEARANCE);

const GROUND_SIZE: f32 = fairy_dust::EXAMPLE_GROUND_SIZE;

const HOME_PITCH: f32 = 0.42;
const HOME_YAW: f32 = -0.28;
const HOME_MARGIN: f32 = 0.5;

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE PANELS — WorldText labels stuck to the cube faces that swap text
// when the pause state changes.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_PANEL_TEXT_SIZE: f32 = 68.0;
const FACE_PANEL_STYLE: CubeFacePanelStyle = CubeFacePanelStyle {
    size:             CUBE_SIZE,
    padding:          0.04,
    row_gap:          0.05,
    title_size:       FACE_PANEL_TEXT_SIZE,
    body_size:        FACE_PANEL_TEXT_SIZE,
    active_body_size: FACE_PANEL_TEXT_SIZE,
    color:            fairy_dust::CUBE_FACE_PANEL_BLUE,
};

const CAMERA_LABEL: &str = "OrbitCam";
const CAMERA_TIME_SOURCE_LABEL: &str = "TimeSource::Real";
const GAME_PAUSED_LABEL: &str = "Game Paused";
const GAME_UNPAUSED_LABEL: &str = "Game Unpaused";
const GAME_TIME_SOURCE_LABEL: &str = "TimeSource::Virtual";

#[derive(Component)]
struct GameStatusPanel;

fn spawn_face_panels(mut commands: Commands, cube_query: Query<Entity, With<Cube>>) {
    let Ok(cube) = cube_query.single() else {
        return;
    };

    let Ok(camera_panel) = cube_face_panel(
        FACE_PANEL_STYLE,
        panel_content(CAMERA_LABEL, CAMERA_TIME_SOURCE_LABEL),
    ) else {
        return;
    };
    let Ok(game_panel) = cube_face_panel(
        FACE_PANEL_STYLE,
        panel_content(game_status_title(false), GAME_TIME_SOURCE_LABEL),
    ) else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            parent.spawn((
                Name::new("Camera time source panel"),
                camera_panel.clone(),
                cube_face_transform(face, CUBE_SIZE),
            ));
        }
        for face in [Face::Left, Face::Right] {
            parent.spawn((
                Name::new("Game time source panel"),
                GameStatusPanel,
                game_panel.clone(),
                cube_face_transform(face, CUBE_SIZE),
            ));
        }
    });
}

fn update_game_panels(
    mut commands: Commands,
    time: Res<Time<Virtual>>,
    mut paused_state: Local<Option<bool>>,
    panels: Query<Entity, With<GameStatusPanel>>,
) {
    let paused = time.is_paused();
    if paused_state
        .as_ref()
        .is_some_and(|previous| *previous == paused)
    {
        return;
    }

    *paused_state = Some(paused);
    let tree = cube_face_panel_tree(
        FACE_PANEL_STYLE,
        panel_content(game_status_title(paused), GAME_TIME_SOURCE_LABEL),
    );

    for panel in &panels {
        commands.set_tree(panel, tree.clone());
    }
}

fn panel_content(title: &'static str, time_source: &'static str) -> CubeFacePanelContent {
    CubeFacePanelContent::idle(title, [time_source])
}

const fn game_status_title(paused: bool) -> &'static str {
    if paused {
        GAME_PAUSED_LABEL
    } else {
        GAME_UNPAUSED_LABEL
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// UI — description panel explaining the two time sources on screen.
// ═════════════════════════════════════════════════════════════════════════════

const DESCRIPTION_HEADING: &str = "Time Sources";
const DESCRIPTION_LINES: [&str; 4] = [
    "Virtual time drives gameplay and respects pause.",
    "Real time is wall-clock time and keeps advancing.",
    "This camera opts into Real time for control smoothing.",
    "The cube freezes on pause; the OrbitCam still moves.",
];

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_HEADING)
        .with_fit_width()
        .with_body_size(LABEL_SIZE.0)
        .lines(DESCRIPTION_LINES)
}

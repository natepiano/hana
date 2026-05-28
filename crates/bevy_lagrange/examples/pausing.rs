//! Demonstrates choosing the `OrbitCam` time source so pausing virtual game
//! time does not pause camera controls. `configure_camera` sets
//! `OrbitCam::time_source = TimeSource::Real`; `pause_game_system` toggles
//! `Time<Virtual>`; `cube_rotator_system` reads the default `Res<Time>` which
//! resolves to virtual time inside `Update`, so the cube freezes while the
//! camera keeps lerping.
//!
//! Controls:
//!   P or Space — toggle game pause

use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
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
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::TimeSource;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DescriptionPanel;
use fairy_dust::LABEL_SIZE;
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
        .insert((Cube, CameraHomeTarget))
        .with_orbit_cam(
            configure_camera,
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Pausing")
                .with_anchor(Anchor::TopLeft),
        )
        .with_description_panel(description_panel())
        .with_camera_control_panel()
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

#[derive(Component)]
struct Cube;

const fn configure_camera(camera: &mut OrbitCam) { camera.time_source = TimeSource::Real; }

fn pause_game_system(key_input: Res<ButtonInput<KeyCode>>, mut time: ResMut<Time<Virtual>>) {
    if key_input.just_pressed(KeyCode::KeyP) || key_input.just_pressed(KeyCode::Space) {
        if time.is_paused() {
            time.unpause();
        } else {
            time.pause();
        }
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

const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + CUBE_GROUND_CLEARANCE, 0.0);

const GROUND_SIZE: f32 = 5.0;

const HOME_PITCH: f32 = 0.42;
const HOME_YAW: f32 = -0.28;
const HOME_MARGIN: f32 = 0.5;

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE PANELS — WorldText labels stuck to the cube faces that swap text
// when the pause state changes.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_PANEL_OFFSET: f32 = CUBE_SIZE * 0.5 + 0.003;
const FACE_PANEL_PADDING: f32 = 0.04;
const FACE_PANEL_ROW_GAP: f32 = 0.05;
const FACE_PANEL_TEXT_SIZE: f32 = 68.0;
const FACE_PANEL_TEXT_COLOR: Color = Color::srgb(0.16, 0.42, 1.0);

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

    let Ok(camera_panel) = face_panel(CAMERA_LABEL, CAMERA_TIME_SOURCE_LABEL) else {
        return;
    };
    let Ok(game_panel) = game_status_panel(false) else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        for transform in [front_face_transform(), back_face_transform()] {
            parent.spawn((
                Name::new("Camera time source panel"),
                camera_panel.clone(),
                transform,
            ));
        }
        for transform in [left_face_transform(), right_face_transform()] {
            parent.spawn((
                Name::new("Game time source panel"),
                GameStatusPanel,
                game_panel.clone(),
                transform,
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
    let tree = face_panel_tree(game_status_title(paused), GAME_TIME_SOURCE_LABEL);

    for panel in &panels {
        commands.set_tree(panel, tree.clone());
    }
}

fn face_panel(title: &str, time_source: &str) -> Result<DiegeticPanel, InvalidSize> {
    let transparent = face_panel_material();
    DiegeticPanel::world()
        .size(CUBE_SIZE, CUBE_SIZE)
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .material(transparent.clone())
        .text_material(transparent)
        .with_tree(face_panel_tree(title, time_source))
        .build()
}

fn game_status_panel(paused: bool) -> Result<DiegeticPanel, InvalidSize> {
    face_panel(game_status_title(paused), GAME_TIME_SOURCE_LABEL)
}

fn face_panel_tree(title: &str, time_source: &str) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(CUBE_SIZE))
            .height(Sizing::fixed(CUBE_SIZE))
            .direction(Direction::TopToBottom)
            .child_alignment(AlignX::Center, AlignY::Center)
            .child_gap(FACE_PANEL_ROW_GAP)
            .padding(Padding::all(FACE_PANEL_PADDING))
            .clip(),
    );
    let text_style = face_panel_text();
    builder.text(title, text_style.clone());
    builder.text(time_source, text_style);
    builder.build()
}

fn face_panel_text() -> LayoutTextStyle {
    LayoutTextStyle::new(FACE_PANEL_TEXT_SIZE)
        .with_color(FACE_PANEL_TEXT_COLOR)
        .with_align(TextAlign::Center)
        .with_shadow_mode(GlyphShadowMode::None)
        .no_wrap()
}

fn face_panel_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

const fn game_status_title(paused: bool) -> &'static str {
    if paused {
        GAME_PAUSED_LABEL
    } else {
        GAME_UNPAUSED_LABEL
    }
}

const fn front_face_transform() -> Transform { Transform::from_xyz(0.0, 0.0, FACE_PANEL_OFFSET) }

fn back_face_transform() -> Transform {
    Transform::from_xyz(0.0, 0.0, -FACE_PANEL_OFFSET).with_rotation(Quat::from_rotation_y(PI))
}

fn left_face_transform() -> Transform {
    Transform::from_xyz(-FACE_PANEL_OFFSET, 0.0, 0.0)
        .with_rotation(Quat::from_rotation_y(-FRAC_PI_2))
}

fn right_face_transform() -> Transform {
    Transform::from_xyz(FACE_PANEL_OFFSET, 0.0, 0.0).with_rotation(Quat::from_rotation_y(FRAC_PI_2))
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

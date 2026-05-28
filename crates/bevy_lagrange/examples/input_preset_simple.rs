//! Spawns an `OrbitCam` with `OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse)`
//! — the mouse-oriented preset. `spawn_camera` shows the manual spawn: an
//! `OrbitCam` initialized with focus / yaw / pitch / radius, the preset input
//! mode, and the `FairyDustOrbitCam` marker that opts the camera into the
//! example shell (studio lighting, home, control panel). The in-app control
//! panel lists the exact key and pointer bindings for the preset.

use bevy::prelude::*;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::TitleBar;

// camera
const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
const HOME_MARGIN: f32 = 0.5;

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + CUBE_GROUND_CLEARANCE, 0.0);

// scene
const GROUND_SIZE: f32 = 5.0;

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
        .insert(CameraHomeTarget)
        .with_camera_home()
        .yaw(CAMERA_YAW)
        .pitch(CAMERA_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Simple Mouse")
                .with_anchor(Anchor::TopLeft),
        )
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        OrbitCam {
            focus: CAMERA_FOCUS,
            yaw: Some(CAMERA_YAW),
            pitch: Some(CAMERA_PITCH),
            radius: Some(CAMERA_RADIUS),
            ..default()
        },
        OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        FairyDustOrbitCam,
    ));
}

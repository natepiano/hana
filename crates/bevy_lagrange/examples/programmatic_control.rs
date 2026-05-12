//! Demonstrates app-authored camera control through direct `OrbitCam` target fields.

use bevy::prelude::*;
use bevy_lagrange::ForceUpdate;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraGuidance;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;

const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5, 0.0);

const GROUND_COLOR: Color = Color::srgb(0.28, 0.42, 0.34);
const GROUND_SIZE: f32 = 8.0;

const HOME_FOCUS: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5, 0.0);
const HOME_PITCH: f32 = 0.45;
const HOME_RADIUS: f32 = 6.0;
const HOME_YAW: f32 = 0.65;

const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);

#[derive(Component)]
struct ProgrammaticCamera;

fn main() {
    fairy_dust::sprinkle_example()
        .with_orbit_cam_bundle(
            configure_camera,
            (
                ProgrammaticCamera,
                OrbitCamPreset::BlenderLike,
                CameraGuidance::for_preset(OrbitCamPreset::BlenderLike)
                    .with_anchor(Anchor::BottomRight),
            ),
        )
        .with_ground_plane()
        .size(GROUND_SIZE)
        .color(GROUND_COLOR)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .with_title_bar(
            TitleBar::new("Programmatic Control")
                .with_anchor(Anchor::TopLeft)
                .control("H Home"),
        )
        .with_description_panel(description_panel())
        .with_camera_guidance_panel()
        .add_systems(Startup, spawn_light)
        .add_systems(Update, home_camera)
        .run();
}

const fn configure_camera(camera: &mut OrbitCam) {
    camera.focus = HOME_FOCUS;
    camera.yaw = Some(HOME_YAW);
    camera.pitch = Some(HOME_PITCH);
    camera.radius = Some(HOME_RADIUS);
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new("Programmatic Control")
        .with_anchor(Anchor::BottomLeft)
        .line("Move the camera with the normal Blender-like controls.")
        .line("Press H to home the camera through direct target fields.")
        .line("The system writes target_focus, target_yaw, target_pitch, and target_radius.")
        .line("ForceUpdate::Pending tells OrbitCam to apply the programmatic change.")
}

fn spawn_light(mut commands: Commands) {
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION),
    ));
}

fn home_camera(
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<&mut OrbitCam, With<ProgrammaticCamera>>,
) {
    if !keys.just_pressed(KeyCode::KeyH) {
        return;
    }

    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };

    camera.target_focus = HOME_FOCUS;
    camera.target_yaw = HOME_YAW;
    camera.target_pitch = HOME_PITCH;
    camera.target_radius = HOME_RADIUS;
    camera.force_update = ForceUpdate::Pending;
}

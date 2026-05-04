//! Demonstrates how to control the camera using the keyboard
//! Controls:
//!     Orbit/rotate smoothly: Arrows
//!     Orbit/rotate in 45deg increments: Ctrl+Arrows
//!     Pan smoothly: Shift+Arrows
//!     Pan in 1m increments: Ctrl+Shift+Arrows
//!     Zoom in/out: Z/X

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::ForceUpdate;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadInput;
use bevy_window_manager::WindowManagerPlugin;

// camera
const CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 1.5, 5.0);
const ORBIT_SPEED_DEGREES_PER_SECOND: f32 = 50.0;
const ORBIT_STEP_DEGREES: f32 = 45.0;
const PAN_STEP_DISTANCE: f32 = 1.0;
const ZOOM_SPEED_PER_SECOND: f32 = 5.0;

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 0.0);

// scene
const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
const GROUND_SIZE: f32 = 5.0;
const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);

enum ArrowControlMode {
    StepPan,
    StepOrbit,
    SmoothPan,
    SmoothOrbit,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, keyboard_controls)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(GROUND_COLOR)),
    ));
    // Cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE))),
        MeshMaterial3d(materials.add(CUBE_COLOR)),
        Transform::from_translation(CUBE_TRANSLATION),
    ));
    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION),
    ));
    // Camera
    commands.spawn((
        Transform::from_translation(CAMERA_TRANSLATION),
        OrbitCam {
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput::blender_default()),
                ..default()
            }),
            ..default()
        },
    ));
}

fn arrow_control_mode(key_input: &ButtonInput<KeyCode>) -> ArrowControlMode {
    if key_input.pressed(KeyCode::ControlLeft) && key_input.pressed(KeyCode::ShiftLeft) {
        ArrowControlMode::StepPan
    } else if key_input.pressed(KeyCode::ControlLeft) {
        ArrowControlMode::StepOrbit
    } else if key_input.pressed(KeyCode::ShiftLeft) {
        ArrowControlMode::SmoothPan
    } else {
        ArrowControlMode::SmoothOrbit
    }
}

fn keyboard_controls(
    // If you set `OrbitCam::time_source` to `TimeSource::Real`, you may want to use
    // `Res<Time<Real>>` here too, so you can control the camera while virtual time is paused.
    time: Res<Time>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut orbit_cam_query: Query<(&mut OrbitCam, &mut Transform)>,
) {
    for (mut orbit_cam, mut transform) in &mut orbit_cam_query {
        match arrow_control_mode(&key_input) {
            // Jump focus point 1m using Ctrl+Shift + Arrows.
            ArrowControlMode::StepPan => {
                if key_input.just_pressed(KeyCode::ArrowRight) {
                    orbit_cam.target_focus += Vec3::X * PAN_STEP_DISTANCE;
                }
                if key_input.just_pressed(KeyCode::ArrowLeft) {
                    orbit_cam.target_focus -= Vec3::X * PAN_STEP_DISTANCE;
                }
                if key_input.just_pressed(KeyCode::ArrowUp) {
                    orbit_cam.target_focus += Vec3::Y * PAN_STEP_DISTANCE;
                }
                if key_input.just_pressed(KeyCode::ArrowDown) {
                    orbit_cam.target_focus -= Vec3::Y * PAN_STEP_DISTANCE;
                }
            },
            // Jump by 45 degrees using Left Ctrl + Arrows.
            ArrowControlMode::StepOrbit => {
                if key_input.just_pressed(KeyCode::ArrowRight) {
                    orbit_cam.target_yaw += ORBIT_STEP_DEGREES.to_radians();
                }
                if key_input.just_pressed(KeyCode::ArrowLeft) {
                    orbit_cam.target_yaw -= ORBIT_STEP_DEGREES.to_radians();
                }
                if key_input.just_pressed(KeyCode::ArrowUp) {
                    orbit_cam.target_pitch += ORBIT_STEP_DEGREES.to_radians();
                }
                if key_input.just_pressed(KeyCode::ArrowDown) {
                    orbit_cam.target_pitch -= ORBIT_STEP_DEGREES.to_radians();
                }
            },
            // Pan using Left Shift + Arrows.
            ArrowControlMode::SmoothPan => {
                let mut delta_translation = Vec3::ZERO;
                if key_input.pressed(KeyCode::ArrowRight) {
                    delta_translation += transform.rotation * Vec3::X * time.delta_secs();
                }
                if key_input.pressed(KeyCode::ArrowLeft) {
                    delta_translation += transform.rotation * Vec3::NEG_X * time.delta_secs();
                }
                if key_input.pressed(KeyCode::ArrowUp) {
                    delta_translation += transform.rotation * Vec3::Y * time.delta_secs();
                }
                if key_input.pressed(KeyCode::ArrowDown) {
                    delta_translation += transform.rotation * Vec3::NEG_Y * time.delta_secs();
                }
                transform.translation += delta_translation;
                orbit_cam.target_focus += delta_translation;
            },
            // Smooth rotation using arrow keys without modifiers.
            ArrowControlMode::SmoothOrbit => {
                if key_input.pressed(KeyCode::ArrowRight) {
                    orbit_cam.target_yaw +=
                        ORBIT_SPEED_DEGREES_PER_SECOND.to_radians() * time.delta_secs();
                }
                if key_input.pressed(KeyCode::ArrowLeft) {
                    orbit_cam.target_yaw -=
                        ORBIT_SPEED_DEGREES_PER_SECOND.to_radians() * time.delta_secs();
                }
                if key_input.pressed(KeyCode::ArrowUp) {
                    orbit_cam.target_pitch +=
                        ORBIT_SPEED_DEGREES_PER_SECOND.to_radians() * time.delta_secs();
                }
                if key_input.pressed(KeyCode::ArrowDown) {
                    orbit_cam.target_pitch -=
                        ORBIT_SPEED_DEGREES_PER_SECOND.to_radians() * time.delta_secs();
                }

                // Zoom with Z and X.
                if key_input.pressed(KeyCode::KeyZ) {
                    orbit_cam.target_radius -= ZOOM_SPEED_PER_SECOND * time.delta_secs();
                }
                if key_input.pressed(KeyCode::KeyX) {
                    orbit_cam.target_radius += ZOOM_SPEED_PER_SECOND * time.delta_secs();
                }
            },
        }

        // Force camera to update its transform.
        orbit_cam.force_update = ForceUpdate::Pending;
    }
}

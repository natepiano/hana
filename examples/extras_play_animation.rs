//! Demonstrates `PlayAnimation` — plays a queued sequence of camera movements.
//!
//! Controls:
//!   Space — Play a 5-step camera animation sequence
//!   R     — Reset camera
//!
//! Observe CameraMoveBegin/CameraMoveEnd for each step via info!() logging.

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_panorbit_camera::CameraMove;
use bevy_panorbit_camera::CameraMoveBegin;
use bevy_panorbit_camera::CameraMoveEnd;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::PlayAnimation;
use bevy_panorbit_camera::TrackpadBehavior;

const START_POS: Vec3 = Vec3::new(0.0, 2.0, 6.0);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PanOrbitCameraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, keyboard_input)
        .add_observer(on_move_begin)
        .add_observer(on_move_end)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(10.0, 10.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
    ));
    // Cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    // Camera
    commands.spawn((
        Transform::from_translation(START_POS),
        PanOrbitCamera {
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan: Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Instructions
    commands.spawn(Text::new(
        "Space - Play 5-step animation sequence\nR - Reset camera",
    ));
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    camera_query: Query<Entity, With<PanOrbitCamera>>,
    mut pan_orbit_query: Query<&mut PanOrbitCamera>,
) {
    let Ok(camera) = camera_query.single() else {
        return;
    };

    if keys.just_pressed(KeyCode::Space) {
        let focus = Vec3::new(0.0, 0.5, 0.0);
        let moves = [
            // Step 1: orbit to the side and slightly closer
            CameraMove::ToOrbit {
                focus,
                yaw: 1.5,
                pitch: 0.2,
                radius: 4.0,
                duration: Duration::from_millis(800),
                easing: EaseFunction::CubicInOut,
            },
            // Step 2: dramatic zoom out — pull way back and high overhead
            CameraMove::ToOrbit {
                focus,
                yaw: 2.5,
                pitch: 1.3,
                radius: 20.0,
                duration: Duration::from_millis(1200),
                easing: EaseFunction::CubicIn,
            },
            // Step 3: sweep around to the opposite side while staying wide
            CameraMove::ToOrbit {
                focus,
                yaw: 4.5,
                pitch: 0.6,
                radius: 14.0,
                duration: Duration::from_millis(1200),
                easing: EaseFunction::SineInOut,
            },
            // Step 4: dramatic zoom back in — swoop down close
            CameraMove::ToOrbit {
                focus,
                yaw: 5.5,
                pitch: 0.1,
                radius: 2.0,
                duration: Duration::from_millis(1000),
                easing: EaseFunction::CubicIn,
            },
            // Step 5: bounce back to starting view
            CameraMove::ToOrbit {
                focus,
                yaw: 0.0,
                pitch: 0.3,
                radius: 6.0,
                duration: Duration::from_millis(1200),
                easing: EaseFunction::BounceOut,
            },
        ];

        commands.trigger(PlayAnimation::new(camera, moves));
    }

    if keys.just_pressed(KeyCode::KeyR) {
        if let Ok(mut pan_orbit) = pan_orbit_query.get_mut(camera) {
            let radius = START_POS.length();
            pan_orbit.target_focus = Vec3::ZERO;
            pan_orbit.target_yaw = f32::atan2(START_POS.x, START_POS.z);
            pan_orbit.target_pitch = f32::asin(START_POS.y / radius);
            pan_orbit.target_radius = radius;
            pan_orbit.force_update = true;
            info!("Camera reset");
        }
    }
}

fn on_move_begin(trigger: On<CameraMoveBegin>) {
    info!(
        "CameraMoveBegin: camera={:?} duration={:?}",
        trigger.camera,
        trigger.camera_move.duration()
    );
}

fn on_move_end(trigger: On<CameraMoveEnd>) {
    info!(
        "CameraMoveEnd: camera={:?} duration={:?}",
        trigger.camera,
        trigger.camera_move.duration()
    );
}

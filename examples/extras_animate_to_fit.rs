//! Demonstrates `AnimateToFit` — animates the camera to a specific orientation
//! while framing a target entity.
//!
//! Controls:
//!   Space — AnimateToFit with yaw=45° pitch=30°
//!   R     — Reset camera
//!
//! Observe AnimationBegin and AnimationEnd via info!() logging.

use std::f32::consts::TAU;
use std::time::Duration;

use bevy::prelude::*;
use bevy_panorbit_camera::AnimateToFit;
use bevy_panorbit_camera::AnimationBegin;
use bevy_panorbit_camera::AnimationEnd;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;

#[derive(Component)]
struct Target;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PanOrbitCameraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, keyboard_input)
        .add_observer(on_animation_begin)
        .add_observer(on_animation_end)
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
    // Target cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.5, 1.5, 1.5))),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.4, 0.8))),
        Transform::from_xyz(0.0, 0.75, 0.0),
        Target,
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
        Transform::from_xyz(0.0, 3.0, 8.0),
        PanOrbitCamera::default(),
    ));

    info!("Press Space to AnimateToFit (yaw=45 pitch=30), R to reset");
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    camera_query: Query<Entity, With<PanOrbitCamera>>,
    target_query: Query<Entity, With<Target>>,
    mut pan_orbit_query: Query<&mut PanOrbitCamera>,
) {
    let Ok(camera) = camera_query.single() else {
        return;
    };
    let Ok(target) = target_query.single() else {
        return;
    };

    if keys.just_pressed(KeyCode::Space) {
        commands.trigger(
            AnimateToFit::new(camera, target)
                .yaw(TAU / 8.0)
                .pitch(TAU / 12.0)
                .margin(0.15)
                .duration(Duration::from_millis(1200)),
        );
    }

    if keys.just_pressed(KeyCode::KeyR) {
        if let Ok(mut pan_orbit) = pan_orbit_query.get_mut(camera) {
            pan_orbit.target_focus = Vec3::ZERO;
            pan_orbit.target_yaw = 0.0;
            pan_orbit.target_pitch = 0.0;
            pan_orbit.target_radius = 8.0;
            pan_orbit.force_update = true;
            info!("Camera reset");
        }
    }
}

fn on_animation_begin(trigger: On<AnimationBegin>) {
    info!(
        "AnimationBegin: camera={:?} source={:?}",
        trigger.camera, trigger.source
    );
}

fn on_animation_end(trigger: On<AnimationEnd>) {
    info!(
        "AnimationEnd: camera={:?} source={:?}",
        trigger.camera, trigger.source
    );
}

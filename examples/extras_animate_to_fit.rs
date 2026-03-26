//! Demonstrates `AnimateToFit` — animates the camera to a specific orientation
//! while framing a target entity.
//!
//! Controls:
//!   Space — `AnimateToFit` with yaw=45 degrees pitch=30 degrees
//!   R     — Reset camera
//!
//! Observe `AnimationBegin` and `AnimationEnd` via `info!()` logging.

use std::f32::consts::TAU;
use std::time::Duration;

use bevy::prelude::*;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::PanOrbitCamera;
use bevy_lagrange::TrackpadBehavior;

const START_POS: Vec3 = Vec3::new(0.0, 3.0, 8.0);

#[derive(Component)]
struct Target;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
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
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
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
        Transform::from_translation(START_POS),
        PanOrbitCamera {
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Instructions
    commands.spawn(Text::new(
        "Space - AnimateToFit the cube (yaw=45 pitch=30)\nR - Reset camera",
    ));
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

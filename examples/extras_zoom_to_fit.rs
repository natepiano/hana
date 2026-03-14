//! Demonstrates `ZoomToFit` — frames a target entity in the camera view.
//!
//! Controls:
//!   Space — ZoomToFit the cube (animated)
//!   D     — Toggle debug visualization
//!   R     — Reset camera to starting position
//!
//! Observe ZoomBegin, ZoomEnd, ZoomCancelled via info!() logging.

use std::time::Duration;

use bevy::prelude::*;
use bevy_panorbit_camera::AnimationEnd;
use bevy_panorbit_camera::AnimationSource;
use bevy_panorbit_camera::FitVisualization;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::SetFitTarget;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera::ZoomBegin;
use bevy_panorbit_camera::ZoomEnd;
use bevy_panorbit_camera::ZoomToFit;

const START_POS: Vec3 = Vec3::new(0.0, 3.0, 8.0);

#[derive(Component)]
struct Target;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PanOrbitCameraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_input, toggle_debug_visualization))
        .add_observer(on_zoom_begin)
        .add_observer(on_zoom_end)
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
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_xyz(3.0, 0.5, -2.0),
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
                modifier_pan: Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Instructions
    commands.spawn(Text::new(
        "Space - ZoomToFit the cube\nD - Toggle debug visualization\nR - Reset camera",
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
            ZoomToFit::new(camera, target)
                .margin(0.15)
                .duration(Duration::from_millis(800)),
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

fn on_zoom_begin(trigger: On<ZoomBegin>) {
    info!(
        "ZoomBegin: camera={:?} target={:?} margin={:.2}",
        trigger.camera, trigger.target, trigger.margin
    );
}

fn on_zoom_end(trigger: On<ZoomEnd>) {
    info!(
        "ZoomEnd: camera={:?} target={:?}",
        trigger.camera, trigger.target
    );
}

fn on_animation_end(trigger: On<AnimationEnd>) {
    if trigger.source == AnimationSource::ZoomToFit {
        info!("Animation backing the ZoomToFit completed");
    }
}

fn toggle_debug_visualization(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    camera_query: Query<(Entity, Option<&FitVisualization>), With<PanOrbitCamera>>,
    target_query: Query<Entity, With<Target>>,
) {
    if !keys.just_pressed(KeyCode::KeyD) {
        return;
    }
    let Ok(target) = target_query.single() else {
        return;
    };
    for (camera, viz) in &camera_query {
        if viz.is_some() {
            commands.entity(camera).remove::<FitVisualization>();
            info!("Debug visualization OFF");
        } else {
            commands.trigger(SetFitTarget::new(camera, target));
            commands.entity(camera).insert(FitVisualization);
            info!("Debug visualization ON");
        }
    }
}

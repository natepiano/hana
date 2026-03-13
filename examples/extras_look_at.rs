//! Demonstrates `LookAt` and `LookAtAndZoomToFit`.
//!
//! Controls:
//!   L — LookAt the target (rotates camera in place)
//!   K — LookAtAndZoomToFit (rotates + frames the target)
//!   R — Reset camera
//!
//! Shows the difference: LookAt only rotates, LookAtAndZoomToFit also adjusts radius.

use std::time::Duration;

use bevy::prelude::*;
use bevy_panorbit_camera::AnimationBegin;
use bevy_panorbit_camera::AnimationEnd;
use bevy_panorbit_camera::LookAt;
use bevy_panorbit_camera::LookAtAndZoomToFit;
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
    // Target sphere — off to the side so LookAt is visible
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.5).mesh().uv(32, 18))),
        MeshMaterial3d(materials.add(Color::srgb(0.9, 0.3, 0.1))),
        Transform::from_xyz(4.0, 1.0, -3.0),
        Target,
    ));
    // Reference cube at origin
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.5, 0.5, 0.5))),
        MeshMaterial3d(materials.add(Color::srgb(0.5, 0.5, 0.5))),
        Transform::from_xyz(0.0, 0.25, 0.0),
    ));
    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    // Camera — pointed at origin, target is off to the side
    commands.spawn((
        Transform::from_xyz(0.0, 3.0, 8.0),
        PanOrbitCamera::default(),
    ));

    info!("Press L to LookAt, K to LookAtAndZoomToFit, R to reset");
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

    if keys.just_pressed(KeyCode::KeyL) {
        commands.trigger(LookAt::new(camera, target).duration(Duration::from_millis(600)));
        info!("LookAt triggered — camera rotates to face target");
    }

    if keys.just_pressed(KeyCode::KeyK) {
        commands.trigger(
            LookAtAndZoomToFit::new(camera, target)
                .margin(0.2)
                .duration(Duration::from_millis(800)),
        );
        info!("LookAtAndZoomToFit triggered — camera rotates and frames target");
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
    info!("AnimationBegin: source={:?}", trigger.source);
}

fn on_animation_end(trigger: On<AnimationEnd>) {
    info!("AnimationEnd: source={:?}", trigger.source);
}

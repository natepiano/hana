//! Demonstrates `ZoomToFit`, `LookAt`, and `LookAtAndZoomToFit`.
//!
//! Controls:
//!   Space — `ZoomToFit` the cube (frames without changing look direction)
//!   L     — `LookAt` the cube (rotates camera in place)
//!   K     — `LookAtAndZoomToFit` the cube (rotates + frames)
//!   D     — Toggle debug visualization
//!   R     — Reset camera to starting position
//!
//! Compare K vs Space: both frame the target, but `LookAtAndZoomToFit` also
//! changes the orbit focus to the target, while `ZoomToFit` keeps the
//! current focus and only adjusts radius.

use std::time::Duration;

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::FitVisualization;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::LookAt;
use bevy_lagrange::LookAtAndZoomToFit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::SetFitTarget;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::ZoomBegin;
use bevy_lagrange::ZoomEnd;
use bevy_lagrange::ZoomToFit;

const START_POS: Vec3 = Vec3::new(0.0, 1.5, 3.0);

#[derive(Component)]
struct Target;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_input, toggle_debug_visualization))
        .add_observer(on_zoom_begin)
        .add_observer(on_zoom_end)
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
    // Target cube — off to the right so it's barely in view
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_xyz(3.5, 0.5, 0.0),
        Target,
    ));
    // Gray cube near origin — what the camera starts focused on
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
    // Camera — close to the gray cube, target cube just visible on the right
    commands.spawn((
        Transform::from_translation(START_POS),
        OrbitCam {
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
        "Space - ZoomToFit (frames without rotating)\n\
         L - LookAt (rotates camera only)\n\
         K - LookAtAndZoomToFit (rotates + frames)\n\
         D - Toggle debug visualization\n\
         R - Reset camera",
    ));
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    camera_query: Query<Entity, With<OrbitCam>>,
    target_query: Query<Entity, With<Target>>,
    mut pan_orbit_query: Query<&mut OrbitCam>,
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
        info!("ZoomToFit triggered");
    }

    if keys.just_pressed(KeyCode::KeyL) {
        commands.trigger(LookAt::new(camera, target).duration(Duration::from_millis(600)));
        info!("LookAt triggered");
    }

    if keys.just_pressed(KeyCode::KeyK) {
        commands.trigger(
            LookAtAndZoomToFit::new(camera, target)
                .margin(0.15)
                .duration(Duration::from_millis(800)),
        );
        info!("LookAtAndZoomToFit triggered");
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

fn on_animation_begin(trigger: On<AnimationBegin>) {
    info!("AnimationBegin: source={:?}", trigger.source);
}

fn on_animation_end(trigger: On<AnimationEnd>) {
    if trigger.source == AnimationSource::ZoomToFit {
        info!("Animation backing the ZoomToFit completed");
    } else {
        info!("AnimationEnd: source={:?}", trigger.source);
    }
}

fn toggle_debug_visualization(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    camera_query: Query<(Entity, Option<&FitVisualization>), With<OrbitCam>>,
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

//! Demonstrates `ZoomToFit`, `LookAt`, and `LookAtAndZoomToFit`.
//!
//! Controls:
//!   Space — `ZoomToFit` the cube (frames without changing look direction)
//!   L     — `LookAt` the cube (rotates camera in place)
//!   K     — `LookAtAndZoomToFit` the cube (rotates + frames)
//!   D     — Toggle debug overlay
//!   R     — Reset camera to starting position
//!
//! Compare K vs Space: both frame the target, but `LookAtAndZoomToFit` also
//! changes the orbit focus to the target, while `ZoomToFit` keeps the
//! current focus and only adjusts radius.

mod constants;

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::FitOverlay;
use bevy_lagrange::ForceUpdate;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::LookAt;
use bevy_lagrange::LookAtAndZoomToFit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::SetFitTarget;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomBegin;
use bevy_lagrange::ZoomEnd;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

use crate::constants::FIT_DURATION;
use crate::constants::FIT_MARGIN;
use crate::constants::GROUND_COLOR;
use crate::constants::GROUND_SIZE;
use crate::constants::LIGHT_TRANSLATION;
use crate::constants::LOOK_AT_DURATION;
use crate::constants::REFERENCE_CUBE_COLOR;
use crate::constants::REFERENCE_CUBE_SIZE;
use crate::constants::REFERENCE_CUBE_TRANSLATION;
use crate::constants::START_POS;
use crate::constants::TARGET_COLOR;
use crate::constants::TARGET_SIZE;
use crate::constants::TARGET_TRANSLATION;

#[derive(Component)]
struct Target;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_input, toggle_debug_overlay))
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
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(GROUND_COLOR)),
    ));
    // Target cube — off to the right so it's barely in view
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(TARGET_SIZE.x, TARGET_SIZE.y, TARGET_SIZE.z))),
        MeshMaterial3d(materials.add(TARGET_COLOR)),
        Transform::from_translation(TARGET_TRANSLATION),
        Target,
    ));
    // Gray cube near origin — what the camera starts focused on
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(
            REFERENCE_CUBE_SIZE.x,
            REFERENCE_CUBE_SIZE.y,
            REFERENCE_CUBE_SIZE.z,
        ))),
        MeshMaterial3d(materials.add(REFERENCE_CUBE_COLOR)),
        Transform::from_translation(REFERENCE_CUBE_TRANSLATION),
    ));
    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION),
    ));
    // Camera — close to the gray cube, target cube just visible on the right
    commands.spawn((
        Transform::from_translation(START_POS),
        OrbitCam {
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput::blender_default()),
                ..default()
            }),
            ..default()
        },
    ));

    // Instructions
    commands.spawn(Text::new(
        "Space - ZoomToFit (frames without rotating)\n\
         L - LookAt (rotates camera only)\n\
         K - LookAtAndZoomToFit (rotates + frames)\n\
         D - Toggle debug overlay\n\
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
                .margin(FIT_MARGIN)
                .duration(FIT_DURATION),
        );
        info!("ZoomToFit triggered");
    }

    if keys.just_pressed(KeyCode::KeyL) {
        commands.trigger(LookAt::new(camera, target).duration(LOOK_AT_DURATION));
        info!("LookAt triggered");
    }

    if keys.just_pressed(KeyCode::KeyK) {
        commands.trigger(
            LookAtAndZoomToFit::new(camera, target)
                .margin(FIT_MARGIN)
                .duration(FIT_DURATION),
        );
        info!("LookAtAndZoomToFit triggered");
    }

    if keys.just_pressed(KeyCode::KeyR)
        && let Ok(mut pan_orbit) = pan_orbit_query.get_mut(camera)
    {
        let radius = START_POS.length();
        pan_orbit.target_focus = Vec3::ZERO;
        pan_orbit.target_yaw = f32::atan2(START_POS.x, START_POS.z);
        pan_orbit.target_pitch = f32::asin(START_POS.y / radius);
        pan_orbit.target_radius = radius;
        pan_orbit.force_update = ForceUpdate::Pending;
        info!("Camera reset");
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

fn toggle_debug_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    camera_query: Query<(Entity, Option<&FitOverlay>), With<OrbitCam>>,
    target_query: Query<Entity, With<Target>>,
) {
    if !keys.just_pressed(KeyCode::KeyD) {
        return;
    }
    let Ok(target) = target_query.single() else {
        return;
    };
    for (camera, debug_overlay) in &camera_query {
        if debug_overlay.is_some() {
            commands.entity(camera).remove::<FitOverlay>();
            info!("Debug overlay OFF");
        } else {
            commands.trigger(SetFitTarget::new(camera, target));
            commands.entity(camera).insert(FitOverlay);
            info!("Debug overlay ON");
        }
    }
}

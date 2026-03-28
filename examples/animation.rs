//! Demonstrates two approaches to camera animation.
//!
//! **Manual** (per-frame): directly writes `OrbitCam` fields each frame
//! for a continuous orbit loop. Input is disabled during manual animation.
//!
//! **Event-driven** (extras): triggers `PlayAnimation` or `AnimateToFit`
//! events. The plugin handles interpolation, easing, and queuing.
//!
//! Controls:
//!   M     — Toggle manual orbit animation on/off
//!   Space — `PlayAnimation` 5-step sequence (event-driven)
//!   A     — `AnimateToFit` the cube (event-driven)
//!   R     — Reset camera

use std::f32::consts::TAU;
use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::CameraMove;
use bevy_lagrange::CameraMoveBegin;
use bevy_lagrange::CameraMoveEnd;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::PlayAnimation;
use bevy_lagrange::TrackpadBehavior;

const START_POS: Vec3 = Vec3::new(0.0, 3.0, 8.0);
const INSTRUCTIONS_FONT_SIZE: f32 = 18.0;

#[derive(Component)]
struct Target;

#[derive(Resource, Default)]
struct ManualAnimationActive(bool);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .init_resource::<ManualAnimationActive>()
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_input, manual_animate).chain())
        .add_observer(on_animation_begin)
        .add_observer(on_animation_end)
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
    commands.spawn((
        Text::new(
            "M - Toggle manual orbit animation\n\
             Space - PlayAnimation (5-step sequence)\n\
             A - AnimateToFit (yaw=45 pitch=30)\n\
             R - Reset camera",
        ),
        TextFont {
            font_size: INSTRUCTIONS_FONT_SIZE,
            ..default()
        },
    ));
}

fn stop_manual(manual: &mut ManualAnimationActive, cam: &mut OrbitCam) {
    if manual.0 {
        manual.0 = false;
        cam.enabled = true;
        cam.orbit_smoothness = 0.8;
        cam.zoom_smoothness = 0.8;
        cam.pan_smoothness = 0.8;
        info!("Manual animation OFF");
    }
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut manual: ResMut<ManualAnimationActive>,
    camera_query: Query<Entity, With<OrbitCam>>,
    target_query: Query<Entity, With<Target>>,
    mut orbit_cam_query: Query<&mut OrbitCam>,
) {
    let Ok(camera) = camera_query.single() else {
        return;
    };
    let Ok(mut cam) = orbit_cam_query.get_mut(camera) else {
        return;
    };

    // Toggle manual animation
    if keys.just_pressed(KeyCode::KeyM) {
        if manual.0 {
            stop_manual(&mut manual, &mut cam);
        } else {
            manual.0 = true;
            cam.enabled = false;
            cam.orbit_smoothness = 0.0;
            cam.zoom_smoothness = 0.0;
            cam.pan_smoothness = 0.0;
            // Sync so there's no lerp gap
            if let (Some(yaw), Some(pitch)) = (cam.yaw, cam.pitch) {
                cam.target_yaw = yaw;
                cam.target_pitch = pitch;
            }
            info!("Manual animation ON");
        }
    }

    // AnimateToFit — event-driven
    if keys.just_pressed(KeyCode::KeyA) {
        stop_manual(&mut manual, &mut cam);
        let Ok(target) = target_query.single() else {
            return;
        };
        commands.trigger(
            AnimateToFit::new(camera, target)
                .yaw(TAU / 8.0)
                .pitch(TAU / 12.0)
                .margin(0.15)
                .duration(Duration::from_millis(1200)),
        );
        info!("AnimateToFit triggered");
    }

    // PlayAnimation — event-driven multi-step sequence
    if keys.just_pressed(KeyCode::Space) {
        stop_manual(&mut manual, &mut cam);
        let focus = Vec3::new(0.0, 0.75, 0.0);
        let moves = [
            CameraMove::ToOrbit {
                focus,
                yaw: 1.5,
                pitch: 0.2,
                radius: 4.0,
                duration: Duration::from_millis(800),
                easing: EaseFunction::CubicInOut,
            },
            CameraMove::ToOrbit {
                focus,
                yaw: 2.5,
                pitch: 1.3,
                radius: 20.0,
                duration: Duration::from_millis(1200),
                easing: EaseFunction::CubicIn,
            },
            CameraMove::ToOrbit {
                focus,
                yaw: 4.5,
                pitch: 0.6,
                radius: 14.0,
                duration: Duration::from_millis(1200),
                easing: EaseFunction::SineInOut,
            },
            CameraMove::ToOrbit {
                focus,
                yaw: 5.5,
                pitch: 0.1,
                radius: 2.0,
                duration: Duration::from_millis(1000),
                easing: EaseFunction::CubicIn,
            },
            CameraMove::ToOrbit {
                focus,
                yaw: 0.0,
                pitch: 0.3,
                radius: 8.0,
                duration: Duration::from_millis(1200),
                easing: EaseFunction::BounceOut,
            },
        ];

        commands.trigger(PlayAnimation::new(camera, moves));
        info!("PlayAnimation triggered (5 steps)");
    }

    // Reset
    if keys.just_pressed(KeyCode::KeyR) {
        stop_manual(&mut manual, &mut cam);
        let radius = START_POS.length();
        cam.target_focus = Vec3::ZERO;
        cam.target_yaw = f32::atan2(START_POS.x, START_POS.z);
        cam.target_pitch = f32::asin(START_POS.y / radius);
        cam.target_radius = radius;
        cam.force_update = true;
        info!("Camera reset");
    }
}

/// Per-frame manual animation — only runs when the resource flag is active.
fn manual_animate(
    time: Res<Time>,
    manual: Res<ManualAnimationActive>,
    mut query: Query<&mut OrbitCam>,
) {
    if !manual.0 {
        return;
    }
    for mut cam in &mut query {
        cam.target_yaw += 15f32.to_radians() * time.delta_secs();
        cam.target_pitch = time.elapsed_secs_wrapped().sin() * TAU * 0.1;
        cam.radius =
            Some((((time.elapsed_secs_wrapped() * 2.0).cos() + 1.0) * 0.5).mul_add(2.0, 4.0));
        cam.force_update = true;
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

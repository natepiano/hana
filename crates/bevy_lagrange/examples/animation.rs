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
use bevy_lagrange::CameraInputDisabled;
use bevy_lagrange::CameraMove;
use bevy_lagrange::CameraMoveBegin;
use bevy_lagrange::CameraMoveEnd;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::PlayAnimation;
use bevy_window_manager::WindowManagerPlugin;

// animation
const ANIMATE_TO_FIT_DURATION: Duration = Duration::from_millis(1200);
const ANIMATE_TO_FIT_MARGIN: f32 = 0.15;
const ANIMATE_TO_FIT_PITCH: f32 = TAU / 12.0;
const ANIMATE_TO_FIT_YAW: f32 = TAU / 8.0;
const MANUAL_MODE_SMOOTHNESS_ACTIVE: f32 = 0.0;
const MANUAL_MODE_SMOOTHNESS_INACTIVE: f32 = 0.8;
const MANUAL_ORBIT_PITCH_AMPLITUDE: f32 = TAU * 0.1;
const MANUAL_ORBIT_RADIUS_BASE: f32 = 4.0;
const MANUAL_ORBIT_RADIUS_DELTA: f32 = 2.0;
const MANUAL_ORBIT_RADIUS_FREQUENCY: f32 = 2.0;
const MANUAL_ORBIT_YAW_RADIANS_PER_SECOND: f32 = TAU / 24.0;

// camera
const START_POS: Vec3 = Vec3::new(0.0, 3.0, 8.0);

// instructions
const INSTRUCTIONS_FONT_SIZE: f32 = 18.0;
const INSTRUCTIONS_TEXT: &str = "M - Toggle manual orbit animation\n\
             Space - PlayAnimation (5-step sequence)\n\
             A - AnimateToFit (yaw=45 pitch=30)\n\
             R - Reset camera";

// play animation
#[derive(Clone, Copy)]
struct OrbitAnimationStep {
    duration: Duration,
    easing:   EaseFunction,
    pitch:    f32,
    radius:   f32,
    yaw:      f32,
}

const PLAY_ANIMATION_FOCUS: Vec3 = Vec3::new(0.0, 0.75, 0.0);
const PLAY_ANIMATION_STEPS: [OrbitAnimationStep; 5] = [
    OrbitAnimationStep {
        duration: Duration::from_millis(800),
        easing:   EaseFunction::CubicInOut,
        pitch:    0.2,
        radius:   4.0,
        yaw:      1.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::CubicIn,
        pitch:    1.3,
        radius:   20.0,
        yaw:      2.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::SineInOut,
        pitch:    0.6,
        radius:   14.0,
        yaw:      4.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_secs(1),
        easing:   EaseFunction::CubicIn,
        pitch:    0.1,
        radius:   2.0,
        yaw:      5.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::BounceOut,
        pitch:    0.3,
        radius:   8.0,
        yaw:      0.0,
    },
];

// scene
const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
const GROUND_SIZE: f32 = 10.0;
const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);
const TARGET_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const TARGET_SIZE: Vec3 = Vec3::splat(1.5);
const TARGET_TRANSLATION: Vec3 = Vec3::new(0.0, 0.75, 0.0);

#[derive(Component)]
struct Target;

#[derive(Default, PartialEq, Eq)]
enum ManualAnimationMode {
    #[default]
    Inactive,
    Active,
}

#[derive(Resource, Default)]
struct ManualAnimationState {
    mode: ManualAnimationMode,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .init_resource::<ManualAnimationState>()
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
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(GROUND_COLOR)),
    ));
    // Target cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(TARGET_SIZE.x, TARGET_SIZE.y, TARGET_SIZE.z))),
        MeshMaterial3d(materials.add(TARGET_COLOR)),
        Transform::from_translation(TARGET_TRANSLATION),
        Target,
    ));
    // Light
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION),
    ));
    // Camera
    commands.spawn((Transform::from_translation(START_POS), OrbitCam::default()));

    // Instructions
    commands.spawn((
        Text::new(INSTRUCTIONS_TEXT),
        TextFont {
            font_size: FontSize::Px(INSTRUCTIONS_FONT_SIZE),
            ..default()
        },
    ));
}

fn stop_manual(
    commands: &mut Commands,
    manual: &mut ManualAnimationState,
    camera: Entity,
    cam: &mut OrbitCam,
) {
    if manual.mode == ManualAnimationMode::Active {
        manual.mode = ManualAnimationMode::Inactive;
        commands.entity(camera).remove::<CameraInputDisabled>();
        cam.orbit_smoothness = MANUAL_MODE_SMOOTHNESS_INACTIVE;
        cam.zoom_smoothness = MANUAL_MODE_SMOOTHNESS_INACTIVE;
        cam.pan_smoothness = MANUAL_MODE_SMOOTHNESS_INACTIVE;
        info!("Manual animation OFF");
    }
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut manual: ResMut<ManualAnimationState>,
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
        if manual.mode == ManualAnimationMode::Active {
            stop_manual(&mut commands, &mut manual, camera, &mut cam);
        } else {
            manual.mode = ManualAnimationMode::Active;
            commands.entity(camera).insert(CameraInputDisabled);
            cam.orbit_smoothness = MANUAL_MODE_SMOOTHNESS_ACTIVE;
            cam.zoom_smoothness = MANUAL_MODE_SMOOTHNESS_ACTIVE;
            cam.pan_smoothness = MANUAL_MODE_SMOOTHNESS_ACTIVE;
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
        stop_manual(&mut commands, &mut manual, camera, &mut cam);
        let Ok(target) = target_query.single() else {
            return;
        };
        commands.trigger(
            AnimateToFit::new(camera, target)
                .yaw(ANIMATE_TO_FIT_YAW)
                .pitch(ANIMATE_TO_FIT_PITCH)
                .margin(ANIMATE_TO_FIT_MARGIN)
                .duration(ANIMATE_TO_FIT_DURATION),
        );
        info!("AnimateToFit triggered");
    }

    // PlayAnimation — event-driven multi-step sequence
    if keys.just_pressed(KeyCode::Space) {
        stop_manual(&mut commands, &mut manual, camera, &mut cam);
        let moves = PLAY_ANIMATION_STEPS.map(|step| CameraMove::ToOrbit {
            focus:    PLAY_ANIMATION_FOCUS,
            yaw:      step.yaw,
            pitch:    step.pitch,
            radius:   step.radius,
            duration: step.duration,
            easing:   step.easing,
        });

        commands.trigger(PlayAnimation::new(camera, moves));
        info!("PlayAnimation triggered (5 steps)");
    }

    // Reset
    if keys.just_pressed(KeyCode::KeyR) {
        stop_manual(&mut commands, &mut manual, camera, &mut cam);
        let radius = START_POS.length();
        cam.target_focus = Vec3::ZERO;
        cam.target_yaw = f32::atan2(START_POS.x, START_POS.z);
        cam.target_pitch = f32::asin(START_POS.y / radius);
        cam.target_radius = radius;
        info!("Camera reset");
    }
}

/// Per-frame manual animation — only runs when the resource flag is active.
fn manual_animate(
    time: Res<Time>,
    manual: Res<ManualAnimationState>,
    mut query: Query<&mut OrbitCam>,
) {
    if manual.mode != ManualAnimationMode::Active {
        return;
    }
    for mut cam in &mut query {
        cam.target_yaw += MANUAL_ORBIT_YAW_RADIANS_PER_SECOND * time.delta_secs();
        cam.target_pitch = time.elapsed_secs_wrapped().sin() * MANUAL_ORBIT_PITCH_AMPLITUDE;
        cam.radius = Some(
            (((time.elapsed_secs_wrapped() * MANUAL_ORBIT_RADIUS_FREQUENCY).cos() + 1.0) * 0.5)
                .mul_add(MANUAL_ORBIT_RADIUS_DELTA, MANUAL_ORBIT_RADIUS_BASE),
        );
        cam.force_update();
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

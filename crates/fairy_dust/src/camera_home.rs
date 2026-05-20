//! Capability: a generalized "home" pose for the spawned `OrbitCam`.
//!
//! Registers an invisible cube entity at the caller-supplied [`Transform`] and
//! wires up `H` to [`bevy_lagrange::AnimateToFit`] that entity. The Transform's
//! `scale` defines the region the camera frames; the builder's `yaw`/`pitch`
//! set the orbit orientation. The home pose drives both the startup framing
//! (instant) and the `H` key animation. If a title bar is installed, the
//! `H Home` control chip is prepended automatically.

use std::time::Duration;

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::window::WindowResized;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationReason;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::OrbitCamInteractionStarted;

use crate::constants::HOME_CONTROL;
use crate::constants::HOME_KEY;
use crate::constants::HOME_MIN_EXTENT;
use crate::orbit_cam::FairyDustOrbitCam;
use crate::screen_panels::ControlActivation;
use crate::screen_panels::TitleBarControlState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InitialAnimateState {
    #[default]
    Pending,
    Fired,
}

#[derive(Component)]
struct CameraHomeMarker;

/// Stashed home configuration. Read by the title-bar installer to decide
/// whether to prepend the `H Home` chip.
#[derive(Resource, Clone)]
pub(crate) struct CameraHomeConfig {
    pub transform: Transform,
    pub yaw:       f32,
    pub pitch:     f32,
    pub duration:  Duration,
    pub margin:    f32,
}

/// Resource holding the entity used as the invisible home cube. Exposed so
/// downstream code can mutate the cube's [`Transform`] directly when the
/// event-based [`SetCameraHomeFromEntity`] flow doesn't fit.
#[derive(Resource)]
pub struct CameraHomeEntity(pub Entity);

/// Tracks whether the camera is still at the home pose. The window-resize
/// refit only fires when this is `Yes`, so user-driven pan/zoom/orbit isn't
/// undone by a resize.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AtHome {
    #[default]
    Yes,
    No,
}

pub(crate) fn install(app: &mut App, config: CameraHomeConfig) {
    app.insert_resource(config);
    app.init_resource::<AtHome>();
    app.add_systems(Startup, spawn_home_marker);
    app.add_systems(
        Update,
        (
            trigger_initial_animate,
            handle_home_key,
            refit_on_window_resized,
        ),
    );
    app.add_observer(on_home_animation_begin);
    app.add_observer(on_home_animation_end);
    app.add_observer(on_non_home_animation_begin);
    app.add_observer(on_user_interaction_started);
    app.add_observer(on_set_camera_home_from_entity);
}

/// Fire to retarget the H-key home pose onto an arbitrary mesh entity.
///
/// The observer reads the source entity's [`GlobalTransform`] + [`Aabb`] and
/// writes the resulting world-space center/extents into the invisible home
/// cube. Subsequent `H` presses (and window-resize refits) frame the updated
/// region. Does not animate the camera — trigger an [`AnimateToFit`]
/// separately if you want to move there immediately.
#[derive(Event)]
pub struct SetCameraHomeFromEntity {
    /// Mesh entity whose world-space AABB defines the new home pose.
    pub source: Entity,
}

fn on_set_camera_home_from_entity(
    trigger: On<SetCameraHomeFromEntity>,
    home: Option<Res<CameraHomeEntity>>,
    bounds: Query<(&GlobalTransform, &Aabb)>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(home) = home else {
        return;
    };
    let Ok((global, aabb)) = bounds.get(trigger.source) else {
        return;
    };
    let center = global.transform_point(Vec3::from(aabb.center));
    let (scale, _, _) = global.to_scale_rotation_translation();
    let world_extents = (Vec3::from(aabb.half_extents) * scale * 2.0).abs();
    // Avoid a zero-thickness slab when the source is a 2D mesh — AnimateToFit
    // needs non-zero extents on every axis to compute a fit radius.
    let safe_extents = world_extents.max(Vec3::splat(HOME_MIN_EXTENT));
    let Ok(mut t) = transforms.get_mut(home.0) else {
        return;
    };
    *t = Transform::from_translation(center).with_scale(safe_extents);
}

fn spawn_home_marker(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    config: Res<CameraHomeConfig>,
) {
    let mesh = meshes.add(Cuboid::from_size(Vec3::ONE));
    let entity = commands
        .spawn((
            CameraHomeMarker,
            Mesh3d(mesh),
            config.transform,
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(CameraHomeEntity(entity));
}

fn trigger_initial_animate(
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    mut state: Local<InitialAnimateState>,
) {
    if *state == InitialAnimateState::Fired {
        return;
    }
    let Some(home) = home else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    commands.trigger(
        AnimateToFit::new(camera, home.0)
            .yaw(config.yaw)
            .pitch(config.pitch)
            .margin(config.margin)
            .duration(Duration::ZERO),
    );
    *state = InitialAnimateState::Fired;
}

fn handle_home_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
) {
    if !keys.just_pressed(HOME_KEY) {
        return;
    }
    let Some(home) = home else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    commands.trigger(
        AnimateToFit::new(camera, home.0)
            .yaw(config.yaw)
            .pitch(config.pitch)
            .margin(config.margin)
            .duration(config.duration),
    );
}

fn refit_on_window_resized(
    mut events: MessageReader<WindowResized>,
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    at_home: Res<AtHome>,
) {
    if events.is_empty() {
        return;
    }
    events.clear();
    if *at_home != AtHome::Yes {
        return;
    }
    let Some(home) = home else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    commands.trigger(
        AnimateToFit::new(camera, home.0)
            .yaw(config.yaw)
            .pitch(config.pitch)
            .margin(config.margin)
            .duration(Duration::ZERO),
    );
}

fn on_home_animation_begin(
    trigger: On<AnimationBegin>,
    home: Option<Res<CameraHomeEntity>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    if home.is_none() || trigger.source != AnimationSource::AnimateToFit {
        return;
    }
    for mut bar in &mut bars {
        bar.set_active(HOME_CONTROL, ControlActivation::Active);
    }
}

fn on_home_animation_end(
    trigger: On<AnimationEnd>,
    home: Option<Res<CameraHomeEntity>>,
    mut bars: Query<&mut TitleBarControlState>,
    mut at_home: ResMut<AtHome>,
) {
    if home.is_none() || trigger.source != AnimationSource::AnimateToFit {
        return;
    }
    for mut bar in &mut bars {
        bar.set_active(HOME_CONTROL, ControlActivation::Inactive);
    }
    if matches!(trigger.reason, AnimationReason::Completed) {
        *at_home = AtHome::Yes;
    }
}

fn on_non_home_animation_begin(trigger: On<AnimationBegin>, mut at_home: ResMut<AtHome>) {
    if trigger.source != AnimationSource::AnimateToFit {
        *at_home = AtHome::No;
    }
}

fn on_user_interaction_started(
    _trigger: On<OrbitCamInteractionStarted>,
    mut at_home: ResMut<AtHome>,
) {
    *at_home = AtHome::No;
}

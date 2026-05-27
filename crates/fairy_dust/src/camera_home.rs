//! Capability: a generalized "home" pose for the spawned `OrbitCam`.
//!
//! Registers an invisible cube entity at the caller-supplied [`Transform`] and
//! wires up `H` to [`bevy_lagrange::AnimateToFit`] that entity. The Transform's
//! `scale` defines the region the camera frames; the builder's `yaw`/`pitch`
//! set the orbit orientation. The home pose drives both the startup framing
//! (instant) and the `H` key animation. If a title bar is installed, the
//! `H Home` control chip is prepended automatically.
//!
//! Add [`CameraHomeTarget`] to any entity to frame that entity (and its
//! descendants) instead of the cube. [`bevy_lagrange::AnimateToFit`] extracts
//! every descendant mesh's vertices, so tagging a `WorldText` parent frames all
//! its glyph children without the caller measuring anything. Startup waits for
//! the target's meshes to exist, then snaps to it; the cube is the fallback when
//! nothing carries the marker.

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
use crate::orbit_cam::FairyDustOrbitCam;
use crate::restart_camera::RestartCameraRestore;
use crate::restart_camera::RestoreWindowAnimation;
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

/// Frames this entity and its descendants as the camera home.
///
/// Add it to any entity instead of relying on the placeholder region passed to
/// [`crate::SprinkleBuilder::with_camera_home`]. `H` animates to it, window
/// resizes refit it, and startup snaps to it once its meshes exist. With no
/// marked entity the capability falls back to the placeholder cube. When more
/// than one entity carries the marker the first found wins.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CameraHomeTarget;

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

/// Resource holding the entity used as the invisible home cube.
///
/// Exposed so downstream code can mutate the cube's [`Transform`] directly when
/// neither the [`CameraHomeTarget`] marker nor the [`SetCameraHome`] event fits.
#[derive(Resource)]
pub struct CameraHomeEntity(pub Entity);

/// The entity most recently named by a [`SetCameraHome`] event, if any. Takes
/// priority over a [`CameraHomeTarget`] marker and the fallback cube when
/// resolving the home target.
#[derive(Resource, Default)]
struct CameraHomeOverride(Option<Entity>);

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
    app.init_resource::<CameraHomeOverride>();
    app.add_systems(Startup, spawn_home_marker);
    app.add_systems(
        Update,
        (snap_home_on_ready, handle_home_key, refit_on_window_resized),
    );
    app.add_observer(on_home_animation_begin);
    app.add_observer(on_home_animation_end);
    app.add_observer(on_non_home_animation_begin);
    app.add_observer(on_user_interaction_started);
    app.add_observer(on_set_camera_home);
}

/// Names the entity the camera should treat as home, and snaps to it.
///
/// `fairy_dust` records `target` so `H` and window-resize refits frame it via
/// [`AnimateToFit`] (which fits the target *and its descendants*), and snaps the
/// camera there the first time it receives this event. Fire it when a readiness
/// signal tells you the entity is measurable; re-firing re-points the home,
/// which suits a target that is respawned (e.g. a debug overlay rebuilt on each
/// change). Use this when the fit target is built late or transient — for a
/// stable entity, the [`CameraHomeTarget`] marker is simpler.
#[derive(Event)]
pub struct SetCameraHome {
    /// Entity (and its descendants) to frame as the camera home.
    pub target: Entity,
}

fn on_set_camera_home(
    trigger: On<SetCameraHome>,
    mut override_target: ResMut<CameraHomeOverride>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    restore: Option<Res<RestartCameraRestore>>,
    mut snapped: Local<bool>,
    mut commands: Commands,
) {
    override_target.0 = Some(trigger.target);
    if *snapped {
        return;
    }
    // A saved restart pose is restored by `snap_home_on_ready`; don't fight it.
    if restore
        .as_deref()
        .is_some_and(RestartCameraRestore::has_restart_camera_pose)
    {
        *snapped = true;
        return;
    }
    let Ok(camera) = cameras.single() else {
        return;
    };
    *snapped = true;
    commands.trigger(
        AnimateToFit::new(camera, trigger.target)
            .yaw(config.yaw)
            .pitch(config.pitch)
            .margin(config.margin)
            .duration(Duration::ZERO),
    );
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

/// The current home target, in priority order: the [`SetCameraHome`] override,
/// then the first [`CameraHomeTarget`] entity, then the fallback cube.
fn resolve_home_target(
    cube: Entity,
    override_target: &CameraHomeOverride,
    targets: &Query<Entity, With<CameraHomeTarget>>,
) -> Entity {
    override_target
        .0
        .or_else(|| targets.iter().next())
        .unwrap_or(cube)
}

/// Whether `target` or one of its descendants has an [`Aabb`] yet. `Aabb` lands
/// once a `Mesh3d`'s asset loads, which is also when [`AnimateToFit`] can
/// extract vertices — so this gates the startup snap on the meshes existing.
fn target_meshes_ready(target: Entity, children: &Query<&Children>, aabbs: &Query<&Aabb>) -> bool {
    std::iter::once(target)
        .chain(children.iter_descendants(target))
        .any(|entity| aabbs.contains(entity))
}

/// Snaps the camera to the home target once its meshes exist, exactly once. A
/// saved restart pose wins — it restores the prior window pose instead of the
/// snap. The fallback cube is ready at frame 0, so cube-only homes snap
/// immediately as before; a [`CameraHomeTarget`] waits for its glyphs/meshes.
fn snap_home_on_ready(
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    override_target: Res<CameraHomeOverride>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    targets: Query<Entity, With<CameraHomeTarget>>,
    children: Query<&Children>,
    aabbs: Query<&Aabb>,
    restore: Option<Res<RestartCameraRestore>>,
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
    let target = resolve_home_target(home.0, &override_target, &targets);
    if !target_meshes_ready(target, &children, &aabbs) {
        return;
    }
    if restore
        .as_deref()
        .is_some_and(RestartCameraRestore::has_restart_camera_pose)
    {
        commands.trigger(RestoreWindowAnimation);
        *state = InitialAnimateState::Fired;
        return;
    }
    commands.trigger(
        AnimateToFit::new(camera, target)
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
    override_target: Res<CameraHomeOverride>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    targets: Query<Entity, With<CameraHomeTarget>>,
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
        AnimateToFit::new(
            camera,
            resolve_home_target(home.0, &override_target, &targets),
        )
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
    override_target: Res<CameraHomeOverride>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    targets: Query<Entity, With<CameraHomeTarget>>,
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
        AnimateToFit::new(
            camera,
            resolve_home_target(home.0, &override_target, &targets),
        )
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

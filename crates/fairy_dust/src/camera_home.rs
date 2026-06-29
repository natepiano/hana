//! Capability: a generalized "home" pose for the spawned `OrbitCam`.
//!
//! Wires up `H` to [`bevy_lagrange::AnimateToFit`] the union of every
//! [`CameraHomeTarget`] entity. The builder's `yaw`/`pitch` set the orbit
//! orientation. The home pose drives both the startup framing (instant) and the
//! `H` key animation. If a title bar is installed, the `H Home` control chip is
//! prepended automatically unless the home builder opts out.
//!
//! Add [`CameraHomeTarget`] to any entity to frame that entity (and its
//! descendants). [`bevy_lagrange::AnimateToFit`] extracts every descendant
//! mesh's vertices, so tagging a `WorldText` parent frames all its glyph
//! children without the caller measuring anything. Startup waits for the
//! target's meshes to exist, then snaps to it. If nothing carries the marker,
//! the home camera warns once and waits.

use std::time::Duration;

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::window::WindowResized;
use bevy_diegetic::Anchor;
use bevy_diegetic::PrecomposeHelper;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationReason;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::FitAnchor;
use bevy_lagrange::OrbitCamInteractionStarted;

use crate::constants::AABB_CORNER_SIGNS;
use crate::constants::HOME_AABB_GIZMO_COLOR;
use crate::constants::HOME_CONTROL;
use crate::constants::HOME_KEY;
use crate::constants::MIN_HOME_CUBE_SCALE;
use crate::ensure_plugin;
use crate::orbit_cam::FairyDustOrbitCam;
use crate::restart_camera::RestartCameraRestore;
use crate::restart_camera::RestoreWindowAnimation;
use crate::screen_panels::ControlActivation;
use crate::screen_panels::TitleBarControlState;
use crate::shortcuts;

#[derive(Component)]
struct CameraHomeContext;

action!(HomeCamera);
event!(HomeCameraEvent);
action!(ToggleHomeAabbGizmo);
event!(ToggleHomeAabbGizmoEvent);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InitialAnimateState {
    #[default]
    Pending,
    Fired,
}

#[derive(Component)]
pub(crate) struct CameraHomeMarker;

/// Frames this entity and its descendants as the camera home.
///
/// Add it to any entity whose [`Aabb`] should contribute to the home region.
/// `H` animates to the target union, window resizes refit it, and startup snaps
/// to it once its meshes exist. With no marked entity the capability warns once
/// and waits. When more than one entity carries the marker, Fairy Dust frames
/// their union.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CameraHomeTarget;

/// Stashed home configuration. Read by the title-bar installer to decide
/// whether to prepend the `H Home` chip.
#[derive(Resource, Clone)]
pub(crate) struct CameraHomeConfig {
    pub yaw:               f32,
    pub pitch:             f32,
    pub duration:          Duration,
    pub margin:            f32,
    pub anchor:            Anchor,
    pub offset_px:         Vec2,
    pub title_bar_control: HomeTitleBarControl,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HomeTitleBarControl {
    #[default]
    Shown,
    Hidden,
}

/// Resource holding the entity used as the invisible home cube.
///
/// The cube is an internal fit proxy: each frame its [`Transform`] is rewritten
/// to the union of every [`CameraHomeTarget`] entity.
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HomeAabbGizmoDisplay {
    Shown,
    #[default]
    Hidden,
}

impl HomeAabbGizmoDisplay {
    const fn toggled(self) -> Self {
        match self {
            Self::Shown => Self::Hidden,
            Self::Hidden => Self::Shown,
        }
    }
}

/// Tracks whether [`draw_home_aabb_gizmo`] is currently drawing a wireframe
/// of the home cube. Toggled by **Ctrl+Shift+A** — undocumented debug
/// affordance available in every `fairy_dust`-built example, no setup needed.
/// Defaults to off.
#[derive(Resource, Default)]
struct HomeAabbGizmoVisible(HomeAabbGizmoDisplay);

/// Flips [`HomeAabbGizmoVisible`] on Ctrl+Shift+A. The gizmo combo is a chord,
/// not a single key, so it doesn't collide with bare-`A` bindings the caller
/// may have.
fn toggle_home_aabb_gizmo(mut visible: ResMut<HomeAabbGizmoVisible>) {
    visible.0 = visible.0.toggled();
}

/// Draws a wireframe of the home cube — sized to the union of every
/// [`CameraHomeTarget`] entity — while [`HomeAabbGizmoVisible`] is on. Lets
/// you see what region the camera is actually framing.
fn draw_home_aabb_gizmo(
    visible: Res<HomeAabbGizmoVisible>,
    home: Option<Res<CameraHomeEntity>>,
    cube: Query<&Transform, With<CameraHomeMarker>>,
    mut gizmos: Gizmos,
) {
    if visible.0 == HomeAabbGizmoDisplay::Hidden {
        return;
    }
    let Some(home) = home else {
        return;
    };
    let Ok(transform) = cube.get(home.0) else {
        return;
    };
    gizmos.cube(*transform, HOME_AABB_GIZMO_COLOR);
}

pub(crate) fn install(app: &mut App, config: CameraHomeConfig) {
    ensure_plugin(app, EnhancedInputPlugin);
    app.insert_resource(config);
    app.init_resource::<AtHome>();
    app.init_resource::<HomeAabbGizmoVisible>();
    app.add_input_context::<CameraHomeContext>();
    shortcuts::reserve_key(app, HOME_KEY, HOME_CONTROL);
    app.add_systems(Startup, (spawn_home_marker, spawn_home_actions));
    // `update_home_cube` runs first so the cube reflects the current union of
    // `CameraHomeTarget` entities before any handler triggers `AnimateToFit`
    // off it.
    app.add_systems(
        Update,
        (
            update_home_cube,
            snap_home_on_ready,
            refit_on_window_resized,
        )
            .chain(),
    );
    // Ctrl+Shift+A debug toggle — always available, opt-in by keypress.
    app.add_systems(Update, draw_home_aabb_gizmo);
    bind_action_system!(app, HomeCamera, HomeCameraEvent, handle_home_key);
    bind_action_system!(
        app,
        ToggleHomeAabbGizmo,
        ToggleHomeAabbGizmoEvent,
        toggle_home_aabb_gizmo
    );
    app.add_observer(on_home_animation_begin);
    app.add_observer(on_home_animation_end);
    app.add_observer(on_non_home_animation_begin);
    app.add_observer(on_user_interaction_started);
}

fn spawn_home_actions(mut commands: Commands) {
    commands.spawn((
        CameraHomeContext,
        actions!(CameraHomeContext[
            (Action::<HomeCamera>::new(), bindings![HOME_KEY]),
            (
                Action::<ToggleHomeAabbGizmo>::new(),
                bindings![KeyCode::KeyA.with_mod_keys(ModKeys::CONTROL | ModKeys::SHIFT)],
            ),
        ]),
    ));
}

/// World-space union of every [`CameraHomeTarget`] entity and its
/// descendants' [`Aabb`]s. Returns `(center, size)` of the union box, or
/// `None` when no marked entity (or descendant) has an [`Aabb`] yet.
fn world_aabb_union(
    targets: &Query<Entity, With<CameraHomeTarget>>,
    children: &Query<&Children>,
    aabbs: &Query<&Aabb>,
    transforms: &Query<&GlobalTransform, Without<CameraHomeMarker>>,
    precompose_helpers: &Query<(), With<PrecomposeHelper>>,
) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut found = false;
    for root in targets {
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            if precompose_helpers.contains(entity) {
                continue;
            }
            let (Ok(aabb), Ok(global_transform)) = (aabbs.get(entity), transforms.get(entity))
            else {
                if let Ok(child_entities) = children.get(entity) {
                    stack.extend(child_entities.iter().rev());
                }
                continue;
            };
            let local_center = Vec3::from(aabb.center);
            let half = Vec3::from(aabb.half_extents);
            for sign in AABB_CORNER_SIGNS {
                let world = global_transform.transform_point(local_center + sign * half);
                min = min.min(world);
                max = max.max(world);
                found = true;
            }
            if let Ok(child_entities) = children.get(entity) {
                stack.extend(child_entities.iter().rev());
            }
        }
    }
    if !found {
        return None;
    }
    Some(((min + max) * 0.5, max - min))
}

/// Each frame, rewrites the hidden home cube's [`Transform`] (and its
/// [`GlobalTransform`], so same-tick observers see it) to the union of every
/// [`CameraHomeTarget`] entity. The cube is what every fit handler frames, so
/// updating it here is the only place the framed region changes.
fn update_home_cube(
    home: Option<Res<CameraHomeEntity>>,
    targets: Query<Entity, With<CameraHomeTarget>>,
    children: Query<&Children>,
    aabbs: Query<&Aabb>,
    target_transforms: Query<&GlobalTransform, Without<CameraHomeMarker>>,
    precompose_helpers: Query<(), With<PrecomposeHelper>>,
    mut cube: Query<(&mut Transform, &mut GlobalTransform), With<CameraHomeMarker>>,
    mut warned_no_target: Local<bool>,
) {
    let Some(home) = home else {
        return;
    };
    let Ok((mut cube_transform, mut cube_global)) = cube.get_mut(home.0) else {
        return;
    };
    if targets.is_empty() {
        if !*warned_no_target {
            warn!("fairy_dust camera home has no CameraHomeTarget; home camera will wait");
            *warned_no_target = true;
        }
        return;
    }
    let Some((center, size)) = world_aabb_union(
        &targets,
        &children,
        &aabbs,
        &target_transforms,
        &precompose_helpers,
    ) else {
        return;
    };
    let new_transform =
        Transform::from_translation(center).with_scale(size.max(Vec3::splat(MIN_HOME_CUBE_SCALE)));
    if *cube_transform == new_transform {
        return;
    }
    *cube_transform = new_transform;
    // Write `GlobalTransform` too — `AnimateToFit` observers run after this
    // system in the same tick and read `GlobalTransform`. `PostUpdate`'s
    // transform propagation will recompute it from `Transform` afterwards
    // (same value, no change).
    *cube_global = GlobalTransform::from(new_transform);
}

fn spawn_home_marker(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mesh = meshes.add(Cuboid::from_size(Vec3::ONE));
    let entity = commands
        .spawn((
            CameraHomeMarker,
            Mesh3d(mesh),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(CameraHomeEntity(entity));
}

const fn fit_anchor(anchor: Anchor) -> FitAnchor {
    match anchor {
        Anchor::TopLeft => FitAnchor::TopLeft,
        Anchor::TopCenter => FitAnchor::TopCenter,
        Anchor::TopRight => FitAnchor::TopRight,
        Anchor::CenterLeft => FitAnchor::CenterLeft,
        Anchor::Center => FitAnchor::Center,
        Anchor::CenterRight => FitAnchor::CenterRight,
        Anchor::BottomLeft => FitAnchor::BottomLeft,
        Anchor::BottomCenter => FitAnchor::BottomCenter,
        Anchor::BottomRight => FitAnchor::BottomRight,
    }
}

const fn home_fit(
    camera: Entity,
    home: Entity,
    config: &CameraHomeConfig,
    duration: Duration,
) -> AnimateToFit {
    AnimateToFit::new(camera, home)
        .yaw(config.yaw)
        .pitch(config.pitch)
        .margin(config.margin)
        .anchor(fit_anchor(config.anchor))
        .offset_px(config.offset_px)
        .duration(duration)
}

/// Whether the snap can fire: with marked entities present, wait until at
/// least one (or one of its descendants) has an [`Aabb`] so the union the
/// cube was sized to is meaningful.
fn target_meshes_ready(
    targets: &Query<Entity, With<CameraHomeTarget>>,
    children: &Query<&Children>,
    aabbs: &Query<&Aabb>,
    precompose_helpers: &Query<(), With<PrecomposeHelper>>,
) -> bool {
    targets.iter().any(|target| {
        let mut stack = vec![target];
        while let Some(entity) = stack.pop() {
            if precompose_helpers.contains(entity) {
                continue;
            }
            if aabbs.contains(entity) {
                return true;
            }
            if let Ok(child_entities) = children.get(entity) {
                stack.extend(child_entities.iter().rev());
            }
        }
        false
    })
}

/// Snaps the camera to the home target once its meshes exist, exactly once. A
/// saved restart pose wins — it restores the prior window pose instead of the
/// snap. A [`CameraHomeTarget`] waits for its glyphs/meshes.
fn snap_home_on_ready(
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    targets: Query<Entity, With<CameraHomeTarget>>,
    children: Query<&Children>,
    aabbs: Query<&Aabb>,
    precompose_helpers: Query<(), With<PrecomposeHelper>>,
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
    if !target_meshes_ready(&targets, &children, &aabbs, &precompose_helpers) {
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
    commands.trigger(home_fit(camera, home.0, &config, Duration::ZERO));
    *state = InitialAnimateState::Fired;
}

fn handle_home_key(
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    targets: Query<Entity, With<CameraHomeTarget>>,
    children: Query<&Children>,
    aabbs: Query<&Aabb>,
    precompose_helpers: Query<(), With<PrecomposeHelper>>,
) {
    let Some(home) = home else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    if !target_meshes_ready(&targets, &children, &aabbs, &precompose_helpers) {
        return;
    }
    commands.trigger(home_fit(camera, home.0, &config, config.duration));
}

fn refit_on_window_resized(
    mut events: MessageReader<WindowResized>,
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    at_home: Res<AtHome>,
    targets: Query<Entity, With<CameraHomeTarget>>,
    children: Query<&Children>,
    aabbs: Query<&Aabb>,
    precompose_helpers: Query<(), With<PrecomposeHelper>>,
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
    if !target_meshes_ready(&targets, &children, &aabbs, &precompose_helpers) {
        return;
    }
    commands.trigger(home_fit(camera, home.0, &config, Duration::ZERO));
}

/// Whether an animation lifecycle event is the home fit.
///
/// The home fit is an `AnimateToFit` that frames the home cube. A user-issued
/// `AnimateToFit` aimed at some other entity shares the `AnimateToFit` source
/// but carries a different `target`, so it is not the home fit and must not
/// drive the `H Home` chip.
fn frames_home(
    home: Option<&CameraHomeEntity>,
    source: AnimationSource,
    target: Option<Entity>,
) -> bool {
    home.is_some_and(|home| source == AnimationSource::AnimateToFit && target == Some(home.0))
}

fn on_home_animation_begin(
    trigger: On<AnimationBegin>,
    home: Option<Res<CameraHomeEntity>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    if !frames_home(home.as_deref(), trigger.source, trigger.target) {
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
    if !frames_home(home.as_deref(), trigger.source, trigger.target) {
        return;
    }
    for mut bar in &mut bars {
        bar.set_active(HOME_CONTROL, ControlActivation::Inactive);
    }
    if matches!(trigger.reason, AnimationReason::Completed) {
        *at_home = AtHome::Yes;
    }
}

fn on_non_home_animation_begin(
    trigger: On<AnimationBegin>,
    home: Option<Res<CameraHomeEntity>>,
    mut at_home: ResMut<AtHome>,
) {
    if !frames_home(home.as_deref(), trigger.source, trigger.target) {
        *at_home = AtHome::No;
    }
}

fn on_user_interaction_started(
    _trigger: On<OrbitCamInteractionStarted>,
    mut at_home: ResMut<AtHome>,
) {
    *at_home = AtHome::No;
}

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use super::constants::INSTANT_SMOOTHNESS;
use super::events::AnimationBegin;
use super::events::AnimationEnd;
use super::events::AnimationReason;
use super::events::AnimationRejected;
use super::events::AnimationSource;
use super::events::PlayAnimation;
use super::queue::AnimationSourceMarker;
use super::queue::CameraMove;
use super::queue::CameraMoveList;
use super::queue::ZoomAnimationMarker;
use crate::constants::DEFAULT_ORBIT_ANGLE;
use crate::constants::DEFAULT_TARGET_RADIUS;
use crate::fit::ZoomBegin;
use crate::fit::ZoomContext;
use crate::fit::ZoomEnd;
use crate::fit::ZoomReason;
use crate::free_cam::FreeCam;
use crate::orbit_cam::OrbitCam;

/// Controls what happens when a new animation request conflicts with an active one.
///
/// Insert this component on a camera entity to configure conflict resolution. If not
/// present, defaults to [`LastWins`](AnimationConflictPolicy::LastWins).
///
/// This component is orthogonal to
/// [`CameraInputInterruptBehavior`](crate::CameraInputInterruptBehavior) —
/// `AnimationConflictPolicy` handles programmatic animation requests (e.g.
/// [`ZoomToFit`](crate::ZoomToFit), [`PlayAnimation`](crate::PlayAnimation)) that conflict with an
/// active animation, while `CameraInputInterruptBehavior` handles physical user input interrupting
/// an animation.
///
/// - [`LastWins`](AnimationConflictPolicy::LastWins) — cancel the current animation and start the
///   new one. Fires appropriate `*Cancelled` events for the interrupted operation.
/// - [`FirstWins`](AnimationConflictPolicy::FirstWins) — reject the incoming request. Fires
///   [`AnimationRejected`](crate::AnimationRejected).
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component, Default)]
pub enum AnimationConflictPolicy {
    /// Cancel the current animation and start the new one.
    #[default]
    LastWins,
    /// Reject the incoming request and keep the current animation.
    FirstWins,
}

/// Component that stores camera runtime state values during animations.
///
/// When camera animations are active (via [`CameraMoveList`]), the smoothness values are
/// temporarily set to `0.0` for instant movement. Original values are stored here and
/// restored when the animation completes.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct OrbitCamStash {
    pub(crate) zoom:  f32,
    pub(crate) pan:   f32,
    pub(crate) orbit: f32,
}

/// Component that stores `FreeCam` runtime state values during animations.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct FreeCamStash {
    pub(crate) translate: f32,
    pub(crate) look:      f32,
    pub(crate) roll:      f32,
}

/// Ensures camera runtime state is stashed once and animation overrides are applied.
fn stash_orbit_camera_state(
    commands: &mut Commands,
    entity: Entity,
    camera: &mut OrbitCam,
    existing_stash: Option<&OrbitCamStash>,
) {
    if existing_stash.is_none() {
        let stash = OrbitCamStash {
            zoom:  camera.zoom.damping(),
            pan:   camera.pan.damping(),
            orbit: camera.orbit.damping(),
        };
        commands.entity(entity).insert(stash);
    }

    camera.zoom.set_damping(INSTANT_SMOOTHNESS);
    camera.pan.set_damping(INSTANT_SMOOTHNESS);
    camera.orbit.set_damping(INSTANT_SMOOTHNESS);
}

/// Ensures `FreeCam` runtime state is stashed once and animation overrides are applied.
fn stash_free_camera_state(
    commands: &mut Commands,
    entity: Entity,
    camera: &mut FreeCam,
    existing_stash: Option<&FreeCamStash>,
) {
    if existing_stash.is_none() {
        let stash = FreeCamStash {
            translate: camera.translate.damping(),
            look:      camera.look.damping(),
            roll:      camera.roll.damping(),
        };
        commands.entity(entity).insert(stash);
    }

    camera.translate.set_damping(INSTANT_SMOOTHNESS);
    camera.look.set_damping(INSTANT_SMOOTHNESS);
    camera.roll.set_damping(INSTANT_SMOOTHNESS);
}

/// Fires `ZoomBegin` and inserts `ZoomAnimationMarker` when the accepted
/// animation carries zoom context.
fn begin_zoom_if_needed(
    commands: &mut Commands,
    entity: Entity,
    zoom_context: Option<&ZoomContext>,
) {
    if let Some(zoom_context) = zoom_context {
        commands.trigger(ZoomBegin {
            camera:   entity,
            target:   zoom_context.target,
            margin:   zoom_context.margin,
            duration: zoom_context.duration,
            easing:   zoom_context.easing,
        });
        commands
            .entity(entity)
            .insert(ZoomAnimationMarker(zoom_context.clone()));
    }
}

/// Observer for `PlayAnimation` event - initiates camera animation sequence.
/// This is the single decision point for all trigger-time logic: conflict
/// resolution, zoom lifecycle (`ZoomBegin` / `ZoomAnimationMarker`), and
/// animation begin.
pub(super) fn on_play_animation(
    start: On<PlayAnimation>,
    mut commands: Commands,
    mut orbit_query: Query<(&mut OrbitCam, Option<&OrbitCamStash>)>,
    mut free_query: Query<(&mut FreeCam, Option<&FreeCamStash>)>,
    conflict_query: Query<&AnimationConflictPolicy>,
    move_list_query: Query<&CameraMoveList>,
    marker_query: Query<&ZoomAnimationMarker>,
    source_marker_query: Query<&AnimationSourceMarker>,
) {
    let entity = start.camera;
    let zoom_context = start.zoom_context.clone();
    let source = if zoom_context.is_some() {
        AnimationSource::ZoomToFit
    } else {
        start.source
    };
    let target = start.target;

    let has_orbit_cam = orbit_query.contains(entity);
    let has_free_cam = free_query.contains(entity);
    if !has_orbit_cam && !has_free_cam {
        return;
    }

    let animation_conflict_policy = conflict_query.get(entity).copied().unwrap_or_default();
    let has_in_flight = move_list_query.get(entity).is_ok();

    if has_in_flight {
        match animation_conflict_policy {
            AnimationConflictPolicy::FirstWins => {
                commands.trigger(AnimationRejected {
                    camera: entity,
                    source,
                    target,
                });
                return;
            },
            AnimationConflictPolicy::LastWins => {
                let in_flight = source_marker_query.get(entity).ok();
                let in_flight_source =
                    in_flight.map_or(AnimationSource::PlayAnimation, |marker| marker.source);
                let in_flight_target = in_flight.and_then(|marker| marker.target);
                if let Ok(queue) = move_list_query.get(entity) {
                    let camera_move = queue.camera_moves.front().cloned().unwrap_or(
                        CameraMove::ToOrbitalLookAt {
                            target:   Vec3::ZERO,
                            yaw:      DEFAULT_ORBIT_ANGLE,
                            pitch:    DEFAULT_ORBIT_ANGLE,
                            radius:   DEFAULT_TARGET_RADIUS,
                            roll:     None,
                            duration: Duration::ZERO,
                            easing:   EaseFunction::Linear,
                        },
                    );
                    commands.trigger(AnimationEnd {
                        camera: entity,
                        source: in_flight_source,
                        target: in_flight_target,
                        reason: AnimationReason::Cancelled {
                            interrupted_move: camera_move,
                        },
                    });
                }
                if let Ok(marker) = marker_query.get(entity) {
                    commands.entity(entity).remove::<ZoomAnimationMarker>();
                    commands.trigger(ZoomEnd {
                        camera:   entity,
                        target:   marker.0.target,
                        margin:   marker.0.margin,
                        duration: marker.0.duration,
                        easing:   marker.0.easing,
                        reason:   ZoomReason::Cancelled,
                    });
                }
            },
        }
    }

    // Zoom lifecycle fires here — after conflict resolution has passed.
    // No command-ordering hazard since everything happens in the same observer.
    begin_zoom_if_needed(&mut commands, entity, zoom_context.as_ref());

    commands.trigger(AnimationBegin {
        camera: entity,
        source,
        target,
    });

    if has_orbit_cam {
        if let Ok((mut camera, existing_stash)) = orbit_query.get_mut(entity) {
            stash_orbit_camera_state(&mut commands, entity, &mut camera, existing_stash);
        }
    } else if has_free_cam && let Ok((mut camera, existing_stash)) = free_query.get_mut(entity) {
        stash_free_camera_state(&mut commands, entity, &mut camera, existing_stash);
    }

    commands
        .entity(entity)
        .insert(CameraMoveList::new(start.camera_moves.clone()));
    commands
        .entity(entity)
        .insert(AnimationSourceMarker { source, target });
}

/// Observer for direct `CameraMoveList` insertion (bypassing `PlayAnimation`).
/// Reuses the same camera-state stashing behavior as the event-driven path.
pub(super) fn on_camera_move_list_added(
    add: On<Add, CameraMoveList>,
    mut commands: Commands,
    mut orbit_query: Query<(&mut OrbitCam, Option<&OrbitCamStash>)>,
    mut free_query: Query<(&mut FreeCam, Option<&FreeCamStash>)>,
) {
    let entity = add.entity;

    if let Ok((mut camera, existing_stash)) = orbit_query.get_mut(entity) {
        stash_orbit_camera_state(&mut commands, entity, &mut camera, existing_stash);
    } else if let Ok((mut camera, existing_stash)) = free_query.get_mut(entity) {
        stash_free_camera_state(&mut commands, entity, &mut camera, existing_stash);
    }
}

/// Observer that restores camera runtime state when `CameraMoveList` is removed.
pub(super) fn restore_camera_state(
    remove: On<Remove, CameraMoveList>,
    mut commands: Commands,
    mut orbit_query: Query<(&OrbitCamStash, &mut OrbitCam)>,
    mut free_query: Query<(&FreeCamStash, &mut FreeCam)>,
) {
    let entity = remove.entity;

    if let Ok((stash, mut camera)) = orbit_query.get_mut(entity) {
        camera.zoom.set_damping(stash.zoom);
        camera.pan.set_damping(stash.pan);
        camera.orbit.set_damping(stash.orbit);
        commands.entity(entity).remove::<OrbitCamStash>();
    } else if let Ok((stash, mut camera)) = free_query.get_mut(entity) {
        camera.translate.set_damping(stash.translate);
        camera.look.set_damping(stash.look);
        camera.roll.set_damping(stash.roll);
        commands.entity(entity).remove::<FreeCamStash>();
    }
}

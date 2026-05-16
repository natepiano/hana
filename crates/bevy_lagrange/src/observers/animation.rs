use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use crate::animation::AnimationSourceMarker;
use crate::animation::CameraMove;
use crate::animation::CameraMoveList;
use crate::animation::ZoomAnimationMarker;
use crate::components::AnimationConflictPolicy;
use crate::components::CameraInputInterruptBehavior;
use crate::constants::DEFAULT_ORBIT_ANGLE;
use crate::constants::DEFAULT_TARGET_RADIUS;
use crate::constants::INSTANT_SMOOTHNESS;
use crate::events::AnimationBegin;
use crate::events::AnimationEnd;
use crate::events::AnimationReason;
use crate::events::AnimationRejected;
use crate::events::AnimationSource;
use crate::events::PlayAnimation;
use crate::events::ZoomBegin;
use crate::events::ZoomContext;
use crate::events::ZoomEnd;
use crate::events::ZoomReason;
use crate::orbit_cam::OrbitCam;

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

/// Ensures camera runtime state is stashed once and animation overrides are applied.
fn stash_camera_state(
    commands: &mut Commands,
    entity: Entity,
    camera: &mut OrbitCam,
    existing_stash: Option<&OrbitCamStash>,
    _interrupt_behavior: CameraInputInterruptBehavior,
) {
    if existing_stash.is_none() {
        let stash = OrbitCamStash {
            zoom:  camera.zoom_smoothness,
            pan:   camera.pan_smoothness,
            orbit: camera.orbit_smoothness,
        };
        commands.entity(entity).insert(stash);
    }

    camera.zoom_smoothness = INSTANT_SMOOTHNESS;
    camera.pan_smoothness = INSTANT_SMOOTHNESS;
    camera.orbit_smoothness = INSTANT_SMOOTHNESS;
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
    mut camera_query: Query<(
        &mut OrbitCam,
        Option<&OrbitCamStash>,
        Option<&CameraInputInterruptBehavior>,
        Option<&AnimationConflictPolicy>,
    )>,
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

    let Ok((mut camera, existing_stash, interrupt_behavior, conflict_policy)) =
        camera_query.get_mut(entity)
    else {
        return;
    };

    let interrupt_behavior = interrupt_behavior.copied().unwrap_or_default();
    let policy = conflict_policy.copied().unwrap_or_default();
    let has_in_flight = move_list_query.get(entity).is_ok();

    if has_in_flight {
        match policy {
            AnimationConflictPolicy::FirstWins => {
                commands.trigger(AnimationRejected {
                    camera: entity,
                    source,
                });
                return;
            },
            AnimationConflictPolicy::LastWins => {
                let in_flight_source = source_marker_query
                    .get(entity)
                    .map_or(AnimationSource::PlayAnimation, |marker| marker.0);
                if let Ok(queue) = move_list_query.get(entity) {
                    let camera_move =
                        queue
                            .camera_moves
                            .front()
                            .cloned()
                            .unwrap_or(CameraMove::ToOrbit {
                                focus:    Vec3::ZERO,
                                yaw:      DEFAULT_ORBIT_ANGLE,
                                pitch:    DEFAULT_ORBIT_ANGLE,
                                radius:   DEFAULT_TARGET_RADIUS,
                                duration: Duration::ZERO,
                                easing:   EaseFunction::Linear,
                            });
                    commands.trigger(AnimationEnd {
                        camera: entity,
                        source: in_flight_source,
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
    });

    stash_camera_state(
        &mut commands,
        entity,
        &mut camera,
        existing_stash,
        interrupt_behavior,
    );

    commands
        .entity(entity)
        .insert(CameraMoveList::new(start.camera_moves.clone()));
    commands
        .entity(entity)
        .insert(AnimationSourceMarker(source));
}

/// Observer for direct `CameraMoveList` insertion (bypassing `PlayAnimation`).
/// Reuses the same camera-state stashing behavior as the event-driven path.
pub(super) fn on_camera_move_list_added(
    add: On<Add, CameraMoveList>,
    mut commands: Commands,
    mut camera_query: Query<(
        &mut OrbitCam,
        Option<&OrbitCamStash>,
        Option<&CameraInputInterruptBehavior>,
    )>,
) {
    let entity = add.entity;
    let Ok((mut camera, existing_stash, interrupt_behavior)) = camera_query.get_mut(entity) else {
        return;
    };
    let interrupt_behavior = interrupt_behavior.copied().unwrap_or_default();

    stash_camera_state(
        &mut commands,
        entity,
        &mut camera,
        existing_stash,
        interrupt_behavior,
    );
}

/// Observer that restores camera runtime state when `CameraMoveList` is removed.
pub(super) fn restore_camera_state(
    remove: On<Remove, CameraMoveList>,
    mut commands: Commands,
    mut query: Query<(&OrbitCamStash, &mut OrbitCam)>,
) {
    let entity = remove.entity;

    let Ok((stash, mut camera)) = query.get_mut(entity) else {
        return;
    };

    camera.zoom_smoothness = stash.zoom;
    camera.pan_smoothness = stash.pan;
    camera.orbit_smoothness = stash.orbit;

    commands.entity(entity).remove::<OrbitCamStash>();
}

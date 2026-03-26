//! Observers that wire events to camera behavior.

use std::collections::VecDeque;
use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use super::animation::orbital_params_from_offset;
use super::animation::CameraMove;
use super::animation::CameraMoveList;
use super::components::AnimationConflictPolicy;
use super::components::AnimationSourceMarker;
use super::components::CameraInputInterruptBehavior;
use super::components::CurrentFitTarget;
use super::components::PanOrbitCameraStash;
use super::components::ZoomAnimationMarker;
use super::events::AnimateToFit;
use super::events::AnimationBegin;
use super::events::AnimationCancelled;
use super::events::AnimationEnd;
use super::events::AnimationRejected;
use super::events::AnimationSource;
use super::events::LookAt;
use super::events::LookAtAndZoomToFit;
use super::events::PlayAnimation;
use super::events::SetFitTarget;
use super::events::ZoomBegin;
use super::events::ZoomCancelled;
use super::events::ZoomContext;
use super::events::ZoomEnd;
use super::events::ZoomToFit;
use super::fit::calculate_fit;
use super::fit::FitSolution;
use super::support::extract_mesh_vertices;
use crate::PanOrbitCamera;

/// Parameters for an instant orbital snap.
struct SnapOrbit {
    focus:  Vec3,
    yaw:    Option<f32>,
    pitch:  Option<f32>,
    radius: f32,
}

/// Snaps the camera to an orbital position instantly (no animation) and fires
/// caller-provided lifecycle events via `emit_events`.
fn snap_to_orbit(
    commands: &mut Commands,
    panorbit: &mut PanOrbitCamera,
    snap: SnapOrbit,
    emit_events: impl FnOnce(&mut Commands),
) {
    panorbit.focus = snap.focus;
    panorbit.radius = Some(snap.radius);
    panorbit.target_focus = snap.focus;
    panorbit.target_radius = snap.radius;
    if let Some(yaw) = snap.yaw {
        panorbit.yaw = Some(yaw);
        panorbit.target_yaw = yaw;
    }
    if let Some(pitch) = snap.pitch {
        panorbit.pitch = Some(pitch);
        panorbit.target_pitch = pitch;
    }
    panorbit.force_update = true;

    emit_events(commands);
}

/// Ensures camera runtime state is stashed once and animation overrides are applied.
fn stash_camera_state(
    commands: &mut Commands,
    entity: Entity,
    camera: &mut PanOrbitCamera,
    has_existing_stash: bool,
    interrupt_behavior: CameraInputInterruptBehavior,
) {
    if !has_existing_stash {
        let stash = PanOrbitCameraStash {
            zoom:    camera.zoom_smoothness,
            pan:     camera.pan_smoothness,
            orbit:   camera.orbit_smoothness,
            enabled: camera.enabled,
        };
        commands.entity(entity).insert(stash);
    }

    camera.zoom_smoothness = 0.0;
    camera.pan_smoothness = 0.0;
    camera.orbit_smoothness = 0.0;

    if interrupt_behavior == CameraInputInterruptBehavior::Ignore {
        camera.enabled = false;
    }
}

/// Shared fit preparation used by both `ZoomToFit` and `AnimateToFit` observers.
/// Extracts target mesh vertices and computes the fit solution for the requested
/// camera orientation.
#[allow(clippy::too_many_arguments)]
fn prepare_fit_for_target(
    context: &str,
    target: Entity,
    yaw: f32,
    pitch: f32,
    margin: f32,
    projection: &Projection,
    camera: &Camera,
    mesh_query: &Query<&Mesh3d>,
    children_query: &Query<&Children>,
    global_transform_query: &Query<&GlobalTransform>,
    meshes: &Assets<Mesh>,
) -> Option<FitSolution> {
    let Some((vertices, geometric_center)) = extract_mesh_vertices(
        target,
        children_query,
        mesh_query,
        global_transform_query,
        meshes,
    ) else {
        warn!("{context}: Failed to extract mesh vertices for entity {target:?}");
        return None;
    };

    let fit = match calculate_fit(
        &vertices,
        geometric_center,
        yaw,
        pitch,
        margin,
        projection,
        camera,
    ) {
        Ok(fit) => fit,
        Err(error) => {
            warn!("{context}: Failed to calculate fit for entity {target:?}: {error}");
            return None;
        },
    };

    Some(fit)
}

/// Observer for `ZoomToFit` event - frames a target entity in the camera view.
/// When duration is `Duration::ZERO`, snaps instantly.
/// When duration is greater than zero, animates smoothly via [`PlayAnimation`]
/// with a [`ZoomContext`] so that `on_play_animation` handles all conflict
/// resolution and zoom lifecycle events in one place.
/// Requires target entity to have a `Mesh3d` (direct or on descendants).
pub(super) fn on_zoom_to_fit(
    zoom: On<ZoomToFit>,
    mut commands: Commands,
    mut camera_query: Query<(&mut PanOrbitCamera, &Projection, &Camera)>,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    let camera = zoom.camera;
    let target = zoom.target;
    let margin = zoom.margin;
    let duration = zoom.duration;
    let easing = zoom.easing;

    let Ok((mut panorbit, projection, cam)) = camera_query.get_mut(camera) else {
        return;
    };

    debug!(
        "ZoomToFit: yaw={:.3} pitch={:.3} current_focus={:.1?} current_radius={:.1} duration_ms={:.0}",
        panorbit.target_yaw,
        panorbit.target_pitch,
        panorbit.target_focus,
        panorbit.target_radius,
        duration.as_secs_f32() * 1000.0,
    );

    let Some(fit) = prepare_fit_for_target(
        "ZoomToFit",
        target,
        panorbit.target_yaw,
        panorbit.target_pitch,
        margin,
        projection,
        cam,
        &mesh_query,
        &children_query,
        &global_transform_query,
        &meshes,
    ) else {
        return;
    };

    if duration > Duration::ZERO {
        // Animated path: use `ToOrbit` to pass orbital params directly, avoiding
        // gimbal lock from atan2 decomposition at extreme pitch angles.
        let camera_moves = VecDeque::from([CameraMove::ToOrbit {
            focus: fit.focus,
            yaw: panorbit.target_yaw,
            pitch: panorbit.target_pitch,
            radius: fit.radius,
            duration,
            easing,
        }]);

        let ctx = ZoomContext {
            target,
            margin,
            duration,
            easing,
        };

        // `on_play_animation` handles conflict resolution, `ZoomBegin`, and
        // `ZoomAnimationMarker` insertion — all in one place after acceptance.
        commands.trigger(PlayAnimation::new(camera, camera_moves).zoom_context(ctx));
    } else {
        // Instant path: snap directly to target — no `PlayAnimation` involved.
        snap_to_orbit(
            &mut commands,
            &mut panorbit,
            SnapOrbit {
                focus:  fit.focus,
                yaw:    None,
                pitch:  None,
                radius: fit.radius,
            },
            |commands| {
                commands.trigger(ZoomBegin {
                    camera,
                    target,
                    margin,
                    duration,
                    easing,
                });
                commands.trigger(ZoomEnd {
                    camera,
                    target,
                    margin,
                    duration: Duration::ZERO,
                    easing,
                });
            },
        );
    }

    // Route fit target updates through a single lifecycle owner.
    commands.trigger(SetFitTarget::new(camera, target));
}

/// Fires `ZoomBegin` and inserts `ZoomAnimationMarker` when the accepted
/// animation carries zoom context.
fn begin_zoom_if_needed(
    commands: &mut Commands,
    entity: Entity,
    zoom_context: &Option<ZoomContext>,
) {
    if let Some(ctx) = zoom_context {
        commands.trigger(ZoomBegin {
            camera:   entity,
            target:   ctx.target,
            margin:   ctx.margin,
            duration: ctx.duration,
            easing:   ctx.easing,
        });
        commands
            .entity(entity)
            .insert(ZoomAnimationMarker(ctx.clone()));
    }
}

/// Observer for `PlayAnimation` event - initiates camera animation sequence.
/// This is the single decision point for all trigger-time logic: conflict
/// resolution, zoom lifecycle (`ZoomBegin` / `ZoomAnimationMarker`), and
/// animation begin.
#[allow(clippy::type_complexity)]
pub(super) fn on_play_animation(
    start: On<PlayAnimation>,
    mut commands: Commands,
    mut camera_query: Query<(
        &mut PanOrbitCamera,
        Option<&PanOrbitCameraStash>,
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
                // Cancel in-flight animation — read source from existing marker
                let in_flight_source = source_marker_query
                    .get(entity)
                    .map_or(AnimationSource::PlayAnimation, |m| m.0);
                if let Ok(queue) = move_list_query.get(entity) {
                    let camera_move =
                        queue
                            .camera_moves
                            .front()
                            .cloned()
                            .unwrap_or(CameraMove::ToOrbit {
                                focus:    Vec3::ZERO,
                                yaw:      0.0,
                                pitch:    0.0,
                                radius:   1.0,
                                duration: Duration::ZERO,
                                easing:   EaseFunction::Linear,
                            });
                    commands.trigger(AnimationCancelled {
                        camera: entity,
                        source: in_flight_source,
                        camera_move,
                    });
                }
                // Cancel in-flight zoom if present
                if let Ok(marker) = marker_query.get(entity) {
                    commands.entity(entity).remove::<ZoomAnimationMarker>();
                    commands.trigger(ZoomCancelled {
                        camera:   entity,
                        target:   marker.0.target,
                        margin:   marker.0.margin,
                        duration: marker.0.duration,
                        easing:   marker.0.easing,
                    });
                }
            },
        }
    }

    // Zoom lifecycle fires here — after conflict resolution has passed.
    // No command-ordering hazard since everything happens in the same observer.
    begin_zoom_if_needed(&mut commands, entity, &zoom_context);

    commands.trigger(AnimationBegin {
        camera: entity,
        source,
    });

    stash_camera_state(
        &mut commands,
        entity,
        &mut camera,
        existing_stash.is_some(),
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
        &mut PanOrbitCamera,
        Option<&PanOrbitCameraStash>,
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
        existing_stash.is_some(),
        interrupt_behavior,
    );
}

/// Observer for `SetFitTarget` event - sets the target entity for fit visualization.
pub(super) fn on_set_fit_target(set_target: On<SetFitTarget>, mut commands: Commands) {
    commands
        .entity(set_target.camera)
        .insert(CurrentFitTarget(set_target.target));
}

/// Observer for `AnimateToFit` event - animates the camera to a specific orientation
/// while fitting a target entity in view.
pub(super) fn on_animate_to_fit(
    event: On<AnimateToFit>,
    mut commands: Commands,
    mut camera_query: Query<(&mut PanOrbitCamera, &Projection, &Camera)>,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    let camera = event.camera;
    let target = event.target;
    let yaw = event.yaw;
    let pitch = event.pitch;
    let margin = event.margin;
    let duration = event.duration;
    let easing = event.easing;

    let Ok((mut panorbit, projection, cam)) = camera_query.get_mut(camera) else {
        return;
    };

    let Some(fit) = prepare_fit_for_target(
        "AnimateToFit",
        target,
        yaw,
        pitch,
        margin,
        projection,
        cam,
        &mesh_query,
        &children_query,
        &global_transform_query,
        &meshes,
    ) else {
        return;
    };

    if duration > Duration::ZERO {
        let camera_moves = VecDeque::from([CameraMove::ToOrbit {
            focus: fit.focus,
            yaw,
            pitch,
            radius: fit.radius,
            duration,
            easing,
        }]);
        commands.trigger(
            PlayAnimation::new(camera, camera_moves).source(AnimationSource::AnimateToFit),
        );
    } else {
        snap_to_orbit(
            &mut commands,
            &mut panorbit,
            SnapOrbit {
                focus:  fit.focus,
                yaw:    Some(yaw),
                pitch:  Some(pitch),
                radius: fit.radius,
            },
            |commands| {
                let source = AnimationSource::AnimateToFit;
                commands.trigger(AnimationBegin { camera, source });
                commands.trigger(AnimationEnd { camera, source });
            },
        );
    }
    // Route fit target updates through a single lifecycle owner.
    commands.trigger(SetFitTarget::new(camera, target));
}

/// Observer for `LookAt` event — rotates the camera in place to look at a target entity.
/// The camera stays at its current world position; only the orbit pivot re-anchors.
pub(super) fn on_look_at(
    event: On<LookAt>,
    mut commands: Commands,
    mut camera_query: Query<(&mut PanOrbitCamera, &GlobalTransform)>,
    global_transform_query: Query<&GlobalTransform>,
) {
    let camera = event.camera;
    let target = event.target;
    let duration = event.duration;
    let easing = event.easing;

    let Ok((mut panorbit, cam_transform)) = camera_query.get_mut(camera) else {
        return;
    };

    let Ok(target_transform) = global_transform_query.get(target) else {
        warn!("LookAt: target {target:?} has no GlobalTransform");
        return;
    };

    let cam_pos = cam_transform.translation();
    let target_pos = target_transform.translation();

    if duration > Duration::ZERO {
        commands.trigger(
            PlayAnimation::new(
                camera,
                [CameraMove::ToPosition {
                    translation: cam_pos,
                    focus: target_pos,
                    duration,
                    easing,
                }],
            )
            .source(AnimationSource::LookAt),
        );
    } else {
        // Instant path: back-solve orbital params and snap
        let (yaw, pitch, radius) = orbital_params_from_offset(cam_pos - target_pos);
        snap_to_orbit(
            &mut commands,
            &mut panorbit,
            SnapOrbit {
                focus: target_pos,
                yaw: Some(yaw),
                pitch: Some(pitch),
                radius,
            },
            |commands| {
                let source = AnimationSource::LookAt;
                commands.trigger(AnimationBegin { camera, source });
                commands.trigger(AnimationEnd { camera, source });
            },
        );
    }
}

/// Observer for `LookAtAndZoomToFit` event — rotates the camera in place to look at
/// a target entity and adjusts the radius to frame it, all in one fluid motion.
/// The yaw and pitch are back-solved from the camera's current world position.
pub(super) fn on_look_at_and_zoom_to_fit(
    event: On<LookAtAndZoomToFit>,
    mut commands: Commands,
    mut camera_query: Query<(&mut PanOrbitCamera, &Projection, &Camera, &GlobalTransform)>,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    let camera = event.camera;
    let target = event.target;
    let margin = event.margin;
    let duration = event.duration;
    let easing = event.easing;

    let Ok((mut panorbit, projection, cam, cam_transform)) = camera_query.get_mut(camera) else {
        return;
    };

    let cam_pos = cam_transform.translation();

    // Back-solve yaw/pitch from camera's current position relative to the target.
    // We need the target's bounds center for this, so we run the fit calculation
    // with a preliminary yaw/pitch, then refine.
    let Ok(target_gt) = global_transform_query.get(target) else {
        warn!("LookAtAndZoomToFit: target {target:?} has no GlobalTransform");
        return;
    };
    let target_pos = target_gt.translation();
    let (preliminary_yaw, preliminary_pitch, _) = orbital_params_from_offset(cam_pos - target_pos);

    let Some(fit) = prepare_fit_for_target(
        "LookAtAndZoomToFit",
        target,
        preliminary_yaw,
        preliminary_pitch,
        margin,
        projection,
        cam,
        &mesh_query,
        &children_query,
        &global_transform_query,
        &meshes,
    ) else {
        return;
    };

    // Recompute yaw/pitch relative to the fit's focus (bounds center), which may
    // differ slightly from the raw `GlobalTransform` translation.
    let (yaw, pitch, _) = orbital_params_from_offset(cam_pos - fit.focus);

    if duration > Duration::ZERO {
        commands.trigger(
            PlayAnimation::new(
                camera,
                [CameraMove::ToOrbit {
                    focus: fit.focus,
                    yaw,
                    pitch,
                    radius: fit.radius,
                    duration,
                    easing,
                }],
            )
            .source(AnimationSource::LookAtAndZoomToFit),
        );
    } else {
        snap_to_orbit(
            &mut commands,
            &mut panorbit,
            SnapOrbit {
                focus:  fit.focus,
                yaw:    Some(yaw),
                pitch:  Some(pitch),
                radius: fit.radius,
            },
            |commands| {
                let source = AnimationSource::LookAtAndZoomToFit;
                commands.trigger(AnimationBegin { camera, source });
                commands.trigger(AnimationEnd { camera, source });
            },
        );
    }

    commands.trigger(SetFitTarget::new(camera, target));
}

/// Observer that restores camera runtime state when `CameraMoveList` is removed.
pub(super) fn restore_camera_state(
    remove: On<Remove, CameraMoveList>,
    mut commands: Commands,
    mut query: Query<(&PanOrbitCameraStash, &mut PanOrbitCamera)>,
) {
    let entity = remove.entity;

    let Ok((stash, mut camera)) = query.get_mut(entity) else {
        return;
    };

    camera.zoom_smoothness = stash.zoom;
    camera.pan_smoothness = stash.pan;
    camera.orbit_smoothness = stash.orbit;
    camera.enabled = stash.enabled;

    commands.entity(entity).remove::<PanOrbitCameraStash>();
}

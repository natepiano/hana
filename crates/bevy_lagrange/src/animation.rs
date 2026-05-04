//! Camera movement queue and animation system.
//! Allows for simple animation of camera movements with easing functions.
#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]

use std::collections::VecDeque;
use std::time::Duration;

use bevy::math::curve::Curve;
use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_kana::Displacement;
use bevy_kana::Position;

use super::ForceUpdate;
use super::OrbitCam;
use super::components::CameraInputInterruptBehavior;
use super::constants::EXTERNAL_INPUT_TOLERANCE;
use super::constants::MILLIS_PER_SECOND;
use super::events::AnimationCancelled;
use super::events::AnimationEnd;
use super::events::AnimationSource;
use super::events::CameraMoveBegin;
use super::events::CameraMoveEnd;
use super::events::ZoomCancelled;
use super::events::ZoomContext;
use super::events::ZoomEnd;

/// Tracks a zoom-to-fit operation routed through the animation system.
///
/// When `AnimationEnd` fires on an entity with this marker, `ZoomEnd` is triggered and the
/// marker is removed. Wraps the [`ZoomContext`] that originated the zoom.
#[derive(Component, Clone)]
pub(crate) struct ZoomAnimationMarker(pub(crate) ZoomContext);

/// Tracks which trigger source started the current animation.
///
/// Records whether the animation was triggered by [`PlayAnimation`](crate::PlayAnimation),
/// [`ZoomToFit`](crate::ZoomToFit), or [`AnimateToFit`](crate::AnimateToFit). Inserted alongside
/// [`CameraMoveList`] and removed when the animation ends or is cancelled.
#[derive(Component)]
pub(crate) struct AnimationSourceMarker(pub(crate) AnimationSource);

#[derive(Clone, Reflect, Debug)]
struct OrbitSnapshot {
    focus:  Position,
    yaw:    f32,
    pitch:  f32,
    radius: f32,
}

impl OrbitSnapshot {
    const fn new(focus: Position, yaw: f32, pitch: f32, radius: f32) -> Self {
        Self {
            focus,
            yaw,
            pitch,
            radius,
        }
    }
}

/// Individual camera movement with target position and duration.
///
/// Two variants allow different ways to specify the target:
/// - `ToPosition` — world-space translation + focus (for cinematic sequences)
/// - `ToOrbit` — orbital parameters around a focus (for zoom-to-fit, avoids gimbal lock)
#[derive(Clone, Reflect)]
pub enum CameraMove {
    /// Animate to a world-space position looking at a focus point.
    /// The animation system decomposes this into orbital parameters internally.
    ToPosition {
        /// World-space camera position.
        translation: Vec3,
        /// World-space focus point the camera looks at.
        focus:       Vec3,
        /// Duration of this movement step.
        duration:    Duration,
        /// Easing curve for the interpolation.
        easing:      EaseFunction,
    },
    /// Animate to orbital parameters around a focus point.
    /// Avoids gimbal lock at extreme pitch angles (±PI/2) where world-space
    /// decomposition via `atan2` loses yaw information.
    ToOrbit {
        /// World-space focus point the camera orbits around.
        focus:    Vec3,
        /// Target yaw in radians.
        yaw:      f32,
        /// Target pitch in radians.
        pitch:    f32,
        /// Target orbital radius.
        radius:   f32,
        /// Duration of this movement step.
        duration: Duration,
        /// Easing curve for the interpolation.
        easing:   EaseFunction,
    },
}

impl CameraMove {
    /// Returns the duration of this movement step.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        match self {
            Self::ToPosition { duration, .. } | Self::ToOrbit { duration, .. } => *duration,
        }
    }

    /// Returns the duration in milliseconds.
    #[must_use]
    pub const fn duration_ms(&self) -> f32 { self.duration().as_secs_f32() * MILLIS_PER_SECOND }

    /// Returns the easing function for this movement step.
    #[must_use]
    pub const fn easing(&self) -> EaseFunction {
        match self {
            Self::ToPosition { easing, .. } | Self::ToOrbit { easing, .. } => *easing,
        }
    }

    /// Returns the focus point for this movement step.
    #[must_use]
    pub const fn focus(&self) -> Vec3 {
        match self {
            Self::ToPosition { focus, .. } | Self::ToOrbit { focus, .. } => *focus,
        }
    }

    /// Returns the world-space camera position for this move.
    /// For `ToOrbit`, computes the position from orbital parameters.
    #[must_use]
    pub fn translation(&self) -> Vec3 {
        match self {
            Self::ToPosition { translation, .. } => *translation,
            Self::ToOrbit {
                focus,
                yaw,
                pitch,
                radius,
                ..
            } => {
                let yaw_rot = Quat::from_axis_angle(Vec3::Y, *yaw);
                let pitch_rot = Quat::from_axis_angle(Vec3::X, -*pitch);
                let rotation = yaw_rot * pitch_rot;
                *focus + rotation * Vec3::new(0.0, 0.0, *radius)
            },
        }
    }

    /// Returns the target orbital parameters (yaw, pitch, radius).
    /// For `ToPosition`, decomposes from the world-space offset (may lose yaw at ±PI/2 pitch).
    fn orbital_params(&self) -> (f32, f32, f32) {
        match self {
            Self::ToPosition {
                translation, focus, ..
            } => orbital_params_from_offset(Displacement(*translation - *focus)),
            Self::ToOrbit {
                yaw, pitch, radius, ..
            } => (*yaw, *pitch, *radius),
        }
    }
}

/// Decomposes an offset vector (camera position minus focus) into orbital parameters.
/// Returns `(yaw, pitch, radius)`. May lose yaw information at ±PI/2 pitch due to `atan2`.
pub(crate) fn orbital_params_from_offset(offset: Displacement) -> (f32, f32, f32) {
    let radius = offset.length();
    let yaw = offset.x.atan2(offset.z);
    let horizontal_dist = offset.x.hypot(offset.z);
    let pitch = offset.y.atan2(horizontal_dist);
    (yaw, pitch, radius)
}

/// State tracking for the current camera movement
#[derive(Clone, Reflect, Default, Debug)]
enum MoveState {
    InProgress {
        elapsed_ms:   f32,
        start:        OrbitSnapshot,
        /// Values written by the animation last frame — if the camera's current
        /// values differ, external input occurred and the animation may interrupt
        /// depending on `CameraInputInterruptBehavior`.
        last_written: OrbitSnapshot,
    },
    #[default]
    Ready,
}

impl MoveState {
    /// Returns `true` if the camera's orbital parameters have been modified by
    /// something other than the animation system since the last frame.
    fn externally_modified(&self, camera: &OrbitCam) -> bool {
        match self {
            Self::InProgress { last_written, .. } => {
                let focus_changed =
                    last_written.focus.distance(camera.target_focus) > EXTERNAL_INPUT_TOLERANCE;
                let yaw_changed =
                    (last_written.yaw - camera.target_yaw).abs() > EXTERNAL_INPUT_TOLERANCE;
                let pitch_changed =
                    (last_written.pitch - camera.target_pitch).abs() > EXTERNAL_INPUT_TOLERANCE;
                let radius_changed =
                    (last_written.radius - camera.target_radius).abs() > EXTERNAL_INPUT_TOLERANCE;
                focus_changed || yaw_changed || pitch_changed || radius_changed
            },
            Self::Ready => false,
        }
    }
}

/// Component that queues multiple camera movements to execute sequentially.
///
/// Simply add this component to a camera entity with a list of movements.
/// The system will automatically process them one by one, removing the component
/// when the queue is empty.
///
/// Camera smoothing is automatically disabled while `camera_moves` are in progress and
/// restored when the queue completes via the `restore_camera_state` observer.
#[derive(Component, Reflect, Default)]
#[require(CameraInputInterruptBehavior)]
#[reflect(Component, Default)]
pub struct CameraMoveList {
    /// The queue of camera movements to process.
    pub camera_moves: VecDeque<CameraMove>,
    state:            MoveState,
}

impl CameraMoveList {
    /// Creates a new `CameraMoveList` from a queue of movements.
    #[must_use]
    pub const fn new(camera_moves: VecDeque<CameraMove>) -> Self {
        Self {
            camera_moves,
            state: MoveState::Ready,
        }
    }

    /// Calculates total remaining time in milliseconds for all queued `camera_moves`.
    pub fn remaining_time_ms(&self) -> f32 {
        // Get remaining time for current move
        let current_remaining = match &self.state {
            MoveState::InProgress { elapsed_ms, .. } => {
                self.camera_moves.front().map_or(0.0, |current_move| {
                    (current_move.duration_ms() - elapsed_ms).max(0.0)
                })
            },
            MoveState::Ready => self
                .camera_moves
                .front()
                .map_or(0.0, CameraMove::duration_ms),
        };

        // Add duration of all remaining camera_moves (skip first since already counted)
        let remaining_queue: f32 = self
            .camera_moves
            .iter()
            .skip(1)
            .map(CameraMove::duration_ms)
            .sum();

        current_remaining + remaining_queue
    }
}

/// Fires end events when the queue is exhausted and removes animation components.
fn handle_empty_queue(
    commands: &mut Commands,
    entity: Entity,
    source: AnimationSource,
    zoom_marker: Option<&ZoomAnimationMarker>,
) {
    // Remove components BEFORE triggering events — observers may re-insert
    // `CameraMoveList` (e.g. splash animation chains hold → zoom → spins),
    // and a deferred removal after the trigger would wipe the new one.
    commands
        .entity(entity)
        .remove::<(CameraMoveList, AnimationSourceMarker)>();
    commands.trigger(AnimationEnd {
        camera: entity,
        source,
    });
    if let Some(marker) = zoom_marker {
        commands.entity(entity).remove::<ZoomAnimationMarker>();
        commands.trigger(ZoomEnd {
            camera:   entity,
            target:   marker.0.target,
            margin:   marker.0.margin,
            duration: marker.0.duration,
            easing:   marker.0.easing,
        });
    }
}

/// Animation state needed to resolve an input interrupt.
struct InterruptContext<'a> {
    queue:              &'a CameraMoveList,
    interrupt_behavior: CameraInputInterruptBehavior,
    source:             AnimationSource,
    current_move:       &'a CameraMove,
    zoom_marker:        Option<&'a ZoomAnimationMarker>,
}

/// Handles external camera input according to `CameraInputInterruptBehavior`.
/// Returns the concrete handling outcome for this frame.
fn handle_camera_input_interrupt(
    commands: &mut Commands,
    entity: Entity,
    pan_orbit: &mut OrbitCam,
    ctx: &InterruptContext,
) -> CameraInputInterruptBehavior {
    let interrupt_behavior = ctx.interrupt_behavior;
    let source = ctx.source;
    let current_move = ctx.current_move;
    let zoom_marker = ctx.zoom_marker;
    match interrupt_behavior {
        CameraInputInterruptBehavior::Ignore => CameraInputInterruptBehavior::Ignore,
        CameraInputInterruptBehavior::Cancel => {
            // Stop where we are — fire cancelled events
            commands
                .entity(entity)
                .remove::<(CameraMoveList, AnimationSourceMarker)>();
            commands.trigger(AnimationCancelled {
                camera: entity,
                source,
                camera_move: current_move.clone(),
            });
            if let Some(marker) = zoom_marker {
                commands.entity(entity).remove::<ZoomAnimationMarker>();
                commands.trigger(ZoomCancelled {
                    camera:   entity,
                    target:   marker.0.target,
                    margin:   marker.0.margin,
                    duration: marker.0.duration,
                    easing:   marker.0.easing,
                });
            }
            CameraInputInterruptBehavior::Cancel
        },
        CameraInputInterruptBehavior::Complete => {
            // Jump to the final position of the entire queue
            if let Some(final_move) = ctx.queue.camera_moves.back() {
                let (yaw, pitch, radius) = final_move.orbital_params();
                pan_orbit.target_focus = final_move.focus();
                pan_orbit.target_yaw = yaw;
                pan_orbit.target_pitch = pitch;
                pan_orbit.target_radius = radius;
                pan_orbit.force_update = ForceUpdate::Pending;
            }
            // Fire normal end events
            commands
                .entity(entity)
                .remove::<(CameraMoveList, AnimationSourceMarker)>();
            commands.trigger(AnimationEnd {
                camera: entity,
                source,
            });
            if let Some(marker) = zoom_marker {
                commands.entity(entity).remove::<ZoomAnimationMarker>();
                commands.trigger(ZoomEnd {
                    camera:   entity,
                    target:   marker.0.target,
                    margin:   marker.0.margin,
                    duration: marker.0.duration,
                    easing:   marker.0.easing,
                });
            }
            CameraInputInterruptBehavior::Complete
        },
    }
}

/// Handles the `Ready` state: zero-duration fast path and transition to `InProgress`.
/// Returns `true` if the caller should `continue` the outer loop (zero-duration move consumed).
fn handle_ready_state(
    commands: &mut Commands,
    entity: Entity,
    pan_orbit: &mut OrbitCam,
    queue: &mut CameraMoveList,
    current_move: &CameraMove,
) -> bool {
    if current_move.duration().is_zero() {
        commands.trigger(CameraMoveBegin {
            camera:      entity,
            camera_move: current_move.clone(),
        });

        let (target_yaw, target_pitch, target_radius) = current_move.orbital_params();
        pan_orbit.target_focus = current_move.focus();
        pan_orbit.target_radius = target_radius;
        pan_orbit.target_yaw = target_yaw;
        pan_orbit.target_pitch = target_pitch;
        pan_orbit.force_update = ForceUpdate::Pending;

        commands.trigger(CameraMoveEnd {
            camera:      entity,
            camera_move: current_move.clone(),
        });
        queue.camera_moves.pop_front();
        return true;
    }

    // Transition to `InProgress` with captured starting orbital parameters
    let current_orbit = OrbitSnapshot::new(
        Position(pan_orbit.target_focus),
        pan_orbit.target_yaw,
        pan_orbit.target_pitch,
        pan_orbit.target_radius,
    );
    queue.state = MoveState::InProgress {
        elapsed_ms:   0.0,
        start:        current_orbit.clone(),
        last_written: current_orbit,
    };

    commands.trigger(CameraMoveBegin {
        camera:      entity,
        camera_move: current_move.clone(),
    });

    false
}

/// Interpolates the current move, applies easing with angle unwrapping, and advances
/// the queue when the frame completes.
fn handle_in_progress(
    commands: &mut Commands,
    entity: Entity,
    pan_orbit: &mut OrbitCam,
    queue: &mut CameraMoveList,
    current_move: &CameraMove,
    delta_secs: f32,
) {
    let MoveState::InProgress {
        elapsed_ms,
        start,
        last_written,
    } = &mut queue.state
    else {
        return;
    };

    // Update elapsed time
    *elapsed_ms += delta_secs * MILLIS_PER_SECOND;

    // Calculate interpolation factor (0.0 to 1.0)
    let duration_ms = current_move.duration_ms();
    let t = if duration_ms <= 0.0 {
        1.0
    } else {
        (*elapsed_ms / duration_ms).min(1.0)
    };

    let is_final_frame = t >= 1.0;

    // Extract target orbital parameters
    // `ToOrbit` provides them directly; `ToPosition` decomposes via atan2
    let (canonical_yaw, canonical_pitch, canonical_radius) = current_move.orbital_params();

    // Apply easing function from the move
    let t_interp = current_move.easing().sample_unchecked(t);

    // Unwrap angles to [-PI, PI] for smooth interpolation (always, including final
    // frame). Using canonical angles on the final frame causes yaw
    // snapping when the atan2 decomposition wraps to the opposite side
    // of the PI boundary.
    let mut yaw_diff = canonical_yaw - start.yaw;
    yaw_diff = std::f32::consts::TAU.mul_add(
        -((yaw_diff + std::f32::consts::PI) / std::f32::consts::TAU).floor(),
        yaw_diff,
    );

    let mut pitch_target = canonical_pitch;
    let pitch_diff_raw = pitch_target - start.pitch;
    if pitch_diff_raw > std::f32::consts::PI {
        pitch_target -= std::f32::consts::TAU;
    } else if pitch_diff_raw < -std::f32::consts::PI {
        pitch_target += std::f32::consts::TAU;
    }
    let pitch_diff = pitch_target - start.pitch;

    // `ToPosition` and `ToOrbit` are both normalized to orbital params above
    pan_orbit.target_focus = Vec3::lerp(*start.focus, current_move.focus(), t_interp);
    pan_orbit.target_radius = (canonical_radius - start.radius).mul_add(t_interp, start.radius);
    pan_orbit.target_yaw = yaw_diff.mul_add(t_interp, start.yaw);
    pan_orbit.target_pitch = pitch_diff.mul_add(t_interp, start.pitch);
    pan_orbit.force_update = ForceUpdate::Pending;

    // Save what we wrote so we can detect external changes next frame
    last_written.focus = Position(pan_orbit.target_focus);
    last_written.yaw = pan_orbit.target_yaw;
    last_written.pitch = pan_orbit.target_pitch;
    last_written.radius = pan_orbit.target_radius;

    // Check if move complete and advance to next
    if is_final_frame {
        commands.trigger(CameraMoveEnd {
            camera:      entity,
            camera_move: current_move.clone(),
        });
        queue.camera_moves.pop_front();
        queue.state = MoveState::Ready;
    }
}

/// System that processes camera movement queues with duration-based interpolation.
///
/// When a `OrbitCam` has a `CameraMoveList`, interpolates toward the target over
/// the specified duration with easing. When a move completes, automatically moves to the
/// next. Removes the `CameraMoveList` component when all moves are complete.
pub(crate) fn process_camera_move_list(
    mut commands: Commands,
    time: Res<Time>,
    mut camera_query: Query<(
        Entity,
        &mut OrbitCam,
        &mut CameraMoveList,
        &CameraInputInterruptBehavior,
        Option<&ZoomAnimationMarker>,
        Option<&AnimationSourceMarker>,
    )>,
) {
    for (entity, mut pan_orbit, mut queue, interrupt_behavior, zoom_marker, source_marker) in
        &mut camera_query
    {
        let source = source_marker.map_or(AnimationSource::PlayAnimation, |m| m.0);

        let Some(current_move) = queue.camera_moves.front().cloned() else {
            handle_empty_queue(&mut commands, entity, source, zoom_marker);
            continue;
        };

        if queue.state.externally_modified(&pan_orbit) {
            let outcome = handle_camera_input_interrupt(
                &mut commands,
                entity,
                &mut pan_orbit,
                &InterruptContext {
                    queue: &queue,
                    interrupt_behavior: *interrupt_behavior,
                    source,
                    current_move: &current_move,
                    zoom_marker,
                },
            );
            match outcome {
                CameraInputInterruptBehavior::Ignore => {},
                CameraInputInterruptBehavior::Cancel | CameraInputInterruptBehavior::Complete => {
                    continue;
                },
            }
        }

        match &queue.state {
            MoveState::Ready => {
                handle_ready_state(
                    &mut commands,
                    entity,
                    &mut pan_orbit,
                    &mut queue,
                    &current_move,
                );
            },
            MoveState::InProgress { .. } => {
                handle_in_progress(
                    &mut commands,
                    entity,
                    &mut pan_orbit,
                    &mut queue,
                    &current_move,
                    time.delta_secs(),
                );
            },
        }
    }
}

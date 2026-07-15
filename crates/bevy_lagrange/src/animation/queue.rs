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
use bevy_kana::Position as KanaPosition;

use super::constants::EXTERNAL_INPUT_TOLERANCE;
use super::events::AnimationEnd;
use super::events::AnimationReason;
use super::events::AnimationSource;
use super::events::CameraMoveBegin;
use super::events::CameraMoveEnd;
use crate::CameraBasis;
use crate::constants::MILLIS_PER_SECOND;
use crate::fit::ZoomContext;
use crate::fit::ZoomEnd;
use crate::fit::ZoomReason;
use crate::free_cam::FreeCam;
use crate::free_cam::FreeCamInput;
use crate::operation::LookAngles;
use crate::operation::OrbitAngles;
use crate::operation::Position;
use crate::operation::Roll;
use crate::orbit_cam::OrbitCam;
use crate::orbit_cam::OrbitCamInput;

/// Tracks a zoom-to-fit operation routed through the animation system.
///
/// When `AnimationEnd` fires on an entity with this marker, `ZoomEnd` is triggered and the
/// marker is removed. Wraps the [`ZoomContext`] that originated the zoom.
#[derive(Component, Clone)]
pub(crate) struct ZoomAnimationMarker(pub(crate) ZoomContext);

/// Tracks which trigger source started the current animation.
///
/// Records whether the animation was triggered by [`PlayAnimation`](crate::PlayAnimation),
/// [`ZoomToFit`](crate::ZoomToFit), [`AnimateToFit`](crate::AnimateToFit),
/// [`LookAt`](crate::LookAt), or [`LookAtAndZoomToFit`](crate::LookAtAndZoomToFit),
/// plus the entity it frames, if any. Inserted alongside [`CameraMoveList`] and
/// removed when the animation ends or is cancelled. The recorded `target` lets
/// the end events report the framed entity even though the queue no longer
/// carries it.
#[derive(Component)]
pub(crate) struct AnimationSourceMarker {
    pub(crate) source: AnimationSource,
    pub(crate) target: Option<Entity>,
}

#[derive(Clone, Reflect, Debug)]
struct OrbitSnapshot {
    focus:  KanaPosition,
    yaw:    f32,
    pitch:  f32,
    radius: f32,
}

impl OrbitSnapshot {
    const fn new(focus: KanaPosition, yaw: f32, pitch: f32, radius: f32) -> Self {
        Self {
            focus,
            yaw,
            pitch,
            radius,
        }
    }
}

#[derive(Clone, Reflect, Debug)]
struct FreeSnapshot {
    position: Position,
    look:     LookAngles,
    roll:     Roll,
}

impl FreeSnapshot {
    const fn new(position: Position, look: LookAngles, roll: Roll) -> Self {
        Self {
            position,
            look,
            roll,
        }
    }
}

/// Individual camera movement with target pose and duration.
///
/// Two variants allow different ways to specify the target pose:
/// - `ToLookAt` — world-space camera position + look target
/// - `ToOrbitalLookAt` — look target + yaw/pitch/radius
#[derive(Clone, Debug, Reflect)]
pub enum CameraMove {
    /// Animate to a world-space position looking at a target point.
    /// The animation system decomposes this into orbital parameters internally.
    ToLookAt {
        /// World-space camera position.
        position: Vec3,
        /// World-space point the camera looks at.
        target:   Vec3,
        /// Optional `FreeCam` roll target. `OrbitCam` ignores this field.
        roll:     Option<Roll>,
        /// Duration of this movement step.
        duration: Duration,
        /// Easing curve for the interpolation.
        easing:   EaseFunction,
    },
    /// Animate to a position described by a target point, yaw, pitch, and radius.
    /// Avoids gimbal lock at extreme pitch angles (±PI/2) where world-space
    /// decomposition via `atan2` loses yaw information.
    ToOrbitalLookAt {
        /// World-space point the camera looks at.
        target:   Vec3,
        /// Target yaw in radians.
        yaw:      f32,
        /// Target pitch in radians.
        pitch:    f32,
        /// Target orbital radius.
        radius:   f32,
        /// Optional `FreeCam` roll target. `OrbitCam` ignores this field.
        roll:     Option<Roll>,
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
            Self::ToLookAt { duration, .. } | Self::ToOrbitalLookAt { duration, .. } => *duration,
        }
    }

    /// Returns the duration in milliseconds.
    #[must_use]
    pub const fn duration_ms(&self) -> f32 { self.duration().as_secs_f32() * MILLIS_PER_SECOND }

    /// Returns the easing function for this movement step.
    #[must_use]
    pub const fn easing(&self) -> EaseFunction {
        match self {
            Self::ToLookAt { easing, .. } | Self::ToOrbitalLookAt { easing, .. } => *easing,
        }
    }

    /// Returns the look target point for this movement step.
    #[must_use]
    pub const fn target(&self) -> Vec3 {
        match self {
            Self::ToLookAt { target, .. } | Self::ToOrbitalLookAt { target, .. } => *target,
        }
    }

    /// Returns the optional roll target for camera kinds that support roll.
    #[must_use]
    pub const fn roll(&self) -> Option<Roll> {
        match self {
            Self::ToLookAt { roll, .. } | Self::ToOrbitalLookAt { roll, .. } => *roll,
        }
    }

    /// Returns the world-space camera position for this move.
    /// For `ToOrbitalLookAt`, computes the position from orbital parameters.
    #[must_use]
    pub fn position(&self) -> Vec3 {
        match self {
            Self::ToLookAt { position, .. } => *position,
            Self::ToOrbitalLookAt {
                target,
                yaw,
                pitch,
                radius,
                ..
            } => {
                let yaw_rotation = Quat::from_axis_angle(Vec3::Y, *yaw);
                let pitch_rotation = Quat::from_axis_angle(Vec3::X, -*pitch);
                let rotation = yaw_rotation * pitch_rotation;
                *target + rotation * Vec3::new(0.0, 0.0, *radius)
            },
        }
    }

    /// Returns the target orbital parameters (yaw, pitch, radius).
    /// For `ToLookAt`, decomposes from the world-space offset (may lose yaw at ±PI/2 pitch).
    fn orbital_parameters(&self) -> (f32, f32, f32) {
        match self {
            Self::ToLookAt {
                position, target, ..
            } => orbital_parameters_from_offset(Displacement(*position - *target)),
            Self::ToOrbitalLookAt {
                yaw, pitch, radius, ..
            } => (*yaw, *pitch, *radius),
        }
    }
}

/// Decomposes an offset vector (camera position minus focus) into orbital parameters.
/// Returns `(yaw, pitch, radius)`. May lose yaw information at ±PI/2 pitch due to `atan2`.
pub(crate) fn orbital_parameters_from_offset(offset: Displacement) -> (f32, f32, f32) {
    let radius = offset.length();
    let yaw = offset.x.atan2(offset.z);
    let horizontal_distance = offset.x.hypot(offset.z);
    let pitch = offset.y.atan2(horizontal_distance);
    (yaw, pitch, radius)
}

/// State tracking for the current camera movement
#[derive(Clone, Reflect, Default, Debug)]
enum MoveState {
    OrbitInProgress {
        elapsed_millis: f32,
        start:          OrbitSnapshot,
        /// Values written by the animation last frame — if the camera's current
        /// values differ, external input occurred and the animation may interrupt
        /// depending on `CameraInputInterruptBehavior`.
        last_written:   OrbitSnapshot,
    },
    FreeInProgress {
        elapsed_millis: f32,
        start:          FreeSnapshot,
        /// Values written by the animation last frame — if the camera's current
        /// values differ, external input occurred and the animation may interrupt
        /// depending on `CameraInputInterruptBehavior`.
        last_written:   FreeSnapshot,
    },
    #[default]
    Ready,
}

impl MoveState {
    /// Returns `true` if the camera's orbital parameters have been modified by
    /// something other than the animation system since the last frame.
    fn orbit_externally_modified(&self, camera: &OrbitCam) -> bool {
        match self {
            Self::OrbitInProgress { last_written, .. } => {
                let focus_changed =
                    last_written.focus.distance(camera.pan.target().0) > EXTERNAL_INPUT_TOLERANCE;
                let yaw_changed =
                    (last_written.yaw - camera.orbit.target().yaw).abs() > EXTERNAL_INPUT_TOLERANCE;
                let pitch_changed = (last_written.pitch - camera.orbit.target().pitch).abs()
                    > EXTERNAL_INPUT_TOLERANCE;
                let radius_changed =
                    (last_written.radius - camera.zoom.target().0).abs() > EXTERNAL_INPUT_TOLERANCE;
                focus_changed || yaw_changed || pitch_changed || radius_changed
            },
            Self::FreeInProgress { .. } | Self::Ready => false,
        }
    }

    /// Returns `true` if the free-camera pose has been modified by something
    /// other than the animation system since the last frame.
    fn free_externally_modified(&self, camera: &FreeCam) -> bool {
        match self {
            Self::FreeInProgress { last_written, .. } => {
                let position_changed = last_written
                    .position
                    .0
                    .distance(camera.translate.target().0)
                    > EXTERNAL_INPUT_TOLERANCE;
                let yaw_changed = (last_written.look.yaw - camera.look.target().yaw).abs()
                    > EXTERNAL_INPUT_TOLERANCE;
                let pitch_changed = (last_written.look.pitch - camera.look.target().pitch).abs()
                    > EXTERNAL_INPUT_TOLERANCE;
                let roll_changed =
                    (last_written.roll.0 - camera.roll.target().0).abs() > EXTERNAL_INPUT_TOLERANCE;
                position_changed || yaw_changed || pitch_changed || roll_changed
            },
            Self::OrbitInProgress { .. } | Self::Ready => false,
        }
    }
}

/// Controls what happens when user input occurs during an in-flight animation.
///
/// Specifically, this governs **user input to the camera** while an animation is
/// playing: orbit/pan/zoom for `OrbitCam`, or translate/look/roll for `FreeCam`.
///
/// This is a required component on [`CameraMoveList`] — if not explicitly inserted, it
/// defaults to [`Ignore`](CameraInputInterruptBehavior::Ignore).
///
/// This component is orthogonal to [`AnimationConflictPolicy`](crate::AnimationConflictPolicy) —
/// `CameraInputInterruptBehavior` handles physical camera input during an animation, while
/// `AnimationConflictPolicy` handles programmatic animation requests that arrive while one is
/// already playing.
///
/// - [`Ignore`](CameraInputInterruptBehavior::Ignore) — disable camera input while animating and
///   keep animating uninterrupted. No interrupt lifecycle events are emitted.
/// - [`Cancel`](CameraInputInterruptBehavior::Cancel) — stop the camera where it is and fire
///   `*Cancelled` events
/// - [`Complete`](CameraInputInterruptBehavior::Complete) — jump to the final position of the
///   entire queue and fire normal `*End` events
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component, Default)]
pub enum CameraInputInterruptBehavior {
    /// Disable camera input and keep animating uninterrupted.
    #[default]
    Ignore,
    /// Stop the camera at its current position. Fires `AnimationEnd` /
    /// `ZoomEnd` with `reason: Cancelled`.
    Cancel,
    /// Jump to the final queued position. Fires `AnimationEnd` / `ZoomEnd`
    /// with `reason: Completed`.
    Complete,
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
    move_state:       MoveState,
}

impl CameraMoveList {
    /// Creates a new `CameraMoveList` from a queue of movements.
    #[must_use]
    pub const fn new(camera_moves: VecDeque<CameraMove>) -> Self {
        Self {
            camera_moves,
            move_state: MoveState::Ready,
        }
    }

    /// Calculates total remaining time in milliseconds for all queued `camera_moves`.
    pub fn remaining_time_ms(&self) -> f32 {
        // Get remaining time for current move
        let current_remaining = match &self.move_state {
            MoveState::Ready => self
                .camera_moves
                .front()
                .map_or(0.0, CameraMove::duration_ms),
            MoveState::FreeInProgress { elapsed_millis, .. }
            | MoveState::OrbitInProgress { elapsed_millis, .. } => {
                self.camera_moves.front().map_or(0.0, |current_move| {
                    (current_move.duration_ms() - elapsed_millis).max(0.0)
                })
            },
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

struct AnimationFinishContext<'a> {
    source:      AnimationSource,
    target:      Option<Entity>,
    zoom_marker: Option<&'a ZoomAnimationMarker>,
}

/// Removes animation components and fires the matching animation/zoom end events.
fn finish_animation(
    commands: &mut Commands,
    entity: Entity,
    finish_context: AnimationFinishContext<'_>,
    reason: AnimationReason,
    zoom_reason: ZoomReason,
) {
    // Remove components BEFORE triggering events — observers may re-insert
    // `CameraMoveList` (e.g. splash animation chains hold → zoom → spins),
    // and a deferred removal after the trigger would wipe the new one.
    commands
        .entity(entity)
        .remove::<(CameraMoveList, AnimationSourceMarker)>();
    commands.trigger(AnimationEnd {
        camera: entity,
        source: finish_context.source,
        target: finish_context.target,
        reason,
    });
    if let Some(marker) = finish_context.zoom_marker {
        commands.entity(entity).remove::<ZoomAnimationMarker>();
        commands.trigger(ZoomEnd {
            camera:   entity,
            target:   marker.0.target,
            margin:   marker.0.margin,
            duration: marker.0.duration,
            easing:   marker.0.easing,
            reason:   zoom_reason,
        });
    }
}

/// Fires end events when the queue is exhausted and removes animation components.
fn handle_empty_queue(
    commands: &mut Commands,
    entity: Entity,
    source: AnimationSource,
    target: Option<Entity>,
    zoom_marker: Option<&ZoomAnimationMarker>,
) {
    finish_animation(
        commands,
        entity,
        AnimationFinishContext {
            source,
            target,
            zoom_marker,
        },
        AnimationReason::Completed,
        ZoomReason::Completed,
    );
}

/// Animation state needed to resolve an input interrupt.
struct InterruptContext<'a> {
    queue:              &'a CameraMoveList,
    interrupt_behavior: CameraInputInterruptBehavior,
    source:             AnimationSource,
    target:             Option<Entity>,
    current_move:       &'a CameraMove,
    zoom_marker:        Option<&'a ZoomAnimationMarker>,
}

/// Handles external camera input according to `CameraInputInterruptBehavior`.
/// Returns the concrete handling outcome for this frame.
fn handle_orbit_camera_input_interrupt(
    commands: &mut Commands,
    entity: Entity,
    pan_orbit: &mut OrbitCam,
    interrupt_context: &InterruptContext,
) -> CameraInputInterruptBehavior {
    let interrupt_behavior = interrupt_context.interrupt_behavior;
    let source = interrupt_context.source;
    let target = interrupt_context.target;
    let current_move = interrupt_context.current_move;
    let zoom_marker = interrupt_context.zoom_marker;
    match interrupt_behavior {
        CameraInputInterruptBehavior::Ignore => CameraInputInterruptBehavior::Ignore,
        CameraInputInterruptBehavior::Cancel => {
            finish_animation(
                commands,
                entity,
                AnimationFinishContext {
                    source,
                    target,
                    zoom_marker,
                },
                AnimationReason::Cancelled {
                    interrupted_move: current_move.clone(),
                },
                ZoomReason::Cancelled,
            );
            CameraInputInterruptBehavior::Cancel
        },
        CameraInputInterruptBehavior::Complete => {
            // Jump to the final position of the entire queue
            if let Some(final_move) = interrupt_context.queue.camera_moves.back() {
                let (yaw, pitch, radius) = final_move.orbital_parameters();
                pan_orbit.pan.set_target(final_move.target());
                pan_orbit.orbit.set_target(OrbitAngles { yaw, pitch });
                pan_orbit.zoom.set_target(radius);
            }
            finish_animation(
                commands,
                entity,
                AnimationFinishContext {
                    source,
                    target,
                    zoom_marker,
                },
                AnimationReason::Completed,
                ZoomReason::Completed,
            );
            CameraInputInterruptBehavior::Complete
        },
    }
}

#[derive(PartialEq, Eq)]
enum ReadyMoveOutcome {
    ConsumedZeroDuration,
    StartedTimedMove,
}

/// Handles the `Ready` state: zero-duration fast path and transition to `InProgress`.
/// Reports whether a zero-duration move was consumed or a timed move was started.
fn handle_orbit_ready_state(
    commands: &mut Commands,
    entity: Entity,
    pan_orbit: &mut OrbitCam,
    queue: &mut CameraMoveList,
    current_move: &CameraMove,
) -> ReadyMoveOutcome {
    if current_move.duration().is_zero() {
        commands.trigger(CameraMoveBegin {
            camera:      entity,
            camera_move: current_move.clone(),
        });

        let (target_yaw, target_pitch, target_radius) = current_move.orbital_parameters();
        pan_orbit.pan.set_target(current_move.target());
        pan_orbit.zoom.set_target(target_radius);
        pan_orbit.orbit.set_target(OrbitAngles {
            yaw:   target_yaw,
            pitch: target_pitch,
        });

        commands.trigger(CameraMoveEnd {
            camera:      entity,
            camera_move: current_move.clone(),
        });
        queue.camera_moves.pop_front();
        return ReadyMoveOutcome::ConsumedZeroDuration;
    }

    // Transition to `InProgress` with captured starting orbital parameters
    let current_orbit = OrbitSnapshot::new(
        KanaPosition(pan_orbit.pan.target().0),
        pan_orbit.orbit.target().yaw,
        pan_orbit.orbit.target().pitch,
        pan_orbit.zoom.target().0,
    );
    queue.move_state = MoveState::OrbitInProgress {
        elapsed_millis: 0.0,
        start:          current_orbit.clone(),
        last_written:   current_orbit,
    };

    commands.trigger(CameraMoveBegin {
        camera:      entity,
        camera_move: current_move.clone(),
    });

    ReadyMoveOutcome::StartedTimedMove
}
/// Interpolates the current move, applies easing with angle unwrapping, and advances
/// the queue when the frame completes.
fn handle_orbit_in_progress(
    commands: &mut Commands,
    entity: Entity,
    pan_orbit: &mut OrbitCam,
    queue: &mut CameraMoveList,
    current_move: &CameraMove,
    delta_secs: f32,
) {
    let MoveState::OrbitInProgress {
        elapsed_millis,
        start,
        last_written,
    } = &mut queue.move_state
    else {
        return;
    };

    // Update elapsed time
    *elapsed_millis = delta_secs.mul_add(MILLIS_PER_SECOND, *elapsed_millis);

    // Calculate interpolation factor (0.0 to 1.0)
    let duration_ms = current_move.duration_ms();
    let t = if duration_ms <= 0.0 {
        1.0
    } else {
        (*elapsed_millis / duration_ms).min(1.0)
    };

    let is_final_frame = t >= 1.0;

    // Extract target orbital parameters
    // `ToOrbitalLookAt` provides them directly; `ToLookAt` decomposes via atan2
    let (canonical_yaw, canonical_pitch, canonical_radius) = current_move.orbital_parameters();

    // Apply easing function from the move
    let interpolation_factor = current_move.easing().sample_unchecked(t);

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

    // `ToLookAt` and `ToOrbitalLookAt` are both normalized to orbital parameters above
    pan_orbit.pan.set_target(Vec3::lerp(
        *start.focus,
        current_move.target(),
        interpolation_factor,
    ));
    pan_orbit
        .zoom
        .set_target((canonical_radius - start.radius).mul_add(interpolation_factor, start.radius));
    pan_orbit.orbit.set_target(OrbitAngles {
        yaw:   yaw_diff.mul_add(interpolation_factor, start.yaw),
        pitch: pitch_diff.mul_add(interpolation_factor, start.pitch),
    });

    // Save what we wrote so we can detect external changes next frame
    last_written.focus = KanaPosition(pan_orbit.pan.target().0);
    last_written.yaw = pan_orbit.orbit.target().yaw;
    last_written.pitch = pan_orbit.orbit.target().pitch;
    last_written.radius = pan_orbit.zoom.target().0;

    // Check if move complete and advance to next
    if is_final_frame {
        commands.trigger(CameraMoveEnd {
            camera:      entity,
            camera_move: current_move.clone(),
        });
        queue.camera_moves.pop_front();
        queue.move_state = MoveState::Ready;
    }
}

fn free_orbit_translation(
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    radius: f32,
    basis: CameraBasis,
) -> Position {
    let yaw_rotation = Quat::from_rotation_y(yaw);
    let pitch_rotation = Quat::from_rotation_x(-pitch);
    Position(focus + basis.rotation() * yaw_rotation * pitch_rotation * Vec3::new(0.0, 0.0, radius))
}

fn free_look_at(translation: Vec3, focus: Vec3, basis: CameraBasis) -> LookAngles {
    let local_offset = basis.rotation().inverse() * (translation - focus);
    let (yaw, pitch, _) = orbital_parameters_from_offset(Displacement(local_offset));
    LookAngles { yaw, pitch }
}

fn free_target_snapshot(
    free_cam: &FreeCam,
    current_move: &CameraMove,
    basis: CameraBasis,
) -> FreeSnapshot {
    let roll = current_move
        .roll()
        .unwrap_or_else(|| free_cam.roll.target());
    match current_move {
        CameraMove::ToLookAt {
            position, target, ..
        } => FreeSnapshot::new(
            Position(*position),
            free_look_at(*position, *target, basis),
            roll,
        ),
        CameraMove::ToOrbitalLookAt {
            target,
            yaw,
            pitch,
            radius,
            ..
        } => FreeSnapshot::new(
            free_orbit_translation(*target, *yaw, *pitch, *radius, basis),
            LookAngles {
                yaw:   *yaw,
                pitch: *pitch,
            },
            roll,
        ),
    }
}

fn apply_free_snapshot(free_cam: &mut FreeCam, snapshot: FreeSnapshot) {
    free_cam.translate.set_target(snapshot.position);
    free_cam.look.set_target(snapshot.look);
    free_cam.roll.set_target(snapshot.roll);
    free_cam.force_update();
}

fn lerp_angle(start: f32, target: f32, factor: f32) -> f32 {
    let mut diff = target - start;
    diff = std::f32::consts::TAU.mul_add(
        -((diff + std::f32::consts::PI) / std::f32::consts::TAU).floor(),
        diff,
    );
    diff.mul_add(factor, start)
}

fn lerp_free_snapshot(start: FreeSnapshot, target: FreeSnapshot, factor: f32) -> FreeSnapshot {
    FreeSnapshot::new(
        Position(start.position.0.lerp(target.position.0, factor)),
        LookAngles {
            yaw:   lerp_angle(start.look.yaw, target.look.yaw, factor),
            pitch: lerp_angle(start.look.pitch, target.look.pitch, factor),
        },
        Roll(lerp_angle(start.roll.0, target.roll.0, factor)),
    )
}

fn handle_free_camera_input_interrupt(
    commands: &mut Commands,
    entity: Entity,
    free_cam: &mut FreeCam,
    basis: CameraBasis,
    interrupt_context: &InterruptContext,
) -> CameraInputInterruptBehavior {
    let interrupt_behavior = interrupt_context.interrupt_behavior;
    let source = interrupt_context.source;
    let target = interrupt_context.target;
    let current_move = interrupt_context.current_move;
    let zoom_marker = interrupt_context.zoom_marker;
    match interrupt_behavior {
        CameraInputInterruptBehavior::Ignore => CameraInputInterruptBehavior::Ignore,
        CameraInputInterruptBehavior::Cancel => {
            finish_animation(
                commands,
                entity,
                AnimationFinishContext {
                    source,
                    target,
                    zoom_marker,
                },
                AnimationReason::Cancelled {
                    interrupted_move: current_move.clone(),
                },
                ZoomReason::Cancelled,
            );
            CameraInputInterruptBehavior::Cancel
        },
        CameraInputInterruptBehavior::Complete => {
            if let Some(final_move) = interrupt_context.queue.camera_moves.back() {
                let snapshot = free_target_snapshot(free_cam, final_move, basis);
                apply_free_snapshot(free_cam, snapshot);
            }
            finish_animation(
                commands,
                entity,
                AnimationFinishContext {
                    source,
                    target,
                    zoom_marker,
                },
                AnimationReason::Completed,
                ZoomReason::Completed,
            );
            CameraInputInterruptBehavior::Complete
        },
    }
}

fn handle_free_ready_state(
    commands: &mut Commands,
    entity: Entity,
    free_cam: &mut FreeCam,
    basis: CameraBasis,
    queue: &mut CameraMoveList,
    current_move: &CameraMove,
) -> ReadyMoveOutcome {
    if current_move.duration().is_zero() {
        commands.trigger(CameraMoveBegin {
            camera:      entity,
            camera_move: current_move.clone(),
        });

        let snapshot = free_target_snapshot(free_cam, current_move, basis);
        apply_free_snapshot(free_cam, snapshot);

        commands.trigger(CameraMoveEnd {
            camera:      entity,
            camera_move: current_move.clone(),
        });
        queue.camera_moves.pop_front();
        return ReadyMoveOutcome::ConsumedZeroDuration;
    }

    let snapshot = FreeSnapshot::new(
        free_cam.translate.target(),
        free_cam.look.target(),
        free_cam.roll.target(),
    );
    queue.move_state = MoveState::FreeInProgress {
        elapsed_millis: 0.0,
        start:          snapshot.clone(),
        last_written:   snapshot,
    };

    commands.trigger(CameraMoveBegin {
        camera:      entity,
        camera_move: current_move.clone(),
    });

    ReadyMoveOutcome::StartedTimedMove
}

fn handle_free_in_progress(
    commands: &mut Commands,
    entity: Entity,
    free_cam: &mut FreeCam,
    basis: CameraBasis,
    queue: &mut CameraMoveList,
    current_move: &CameraMove,
    delta_secs: f32,
) {
    let MoveState::FreeInProgress {
        elapsed_millis,
        start,
        last_written,
    } = &mut queue.move_state
    else {
        return;
    };

    *elapsed_millis = delta_secs.mul_add(MILLIS_PER_SECOND, *elapsed_millis);
    let duration_ms = current_move.duration_ms();
    let progress = if duration_ms <= 0.0 {
        1.0
    } else {
        (*elapsed_millis / duration_ms).min(1.0)
    };
    let factor = current_move.easing().sample_unchecked(progress);
    let target = free_target_snapshot(free_cam, current_move, basis);
    let snapshot = lerp_free_snapshot(start.clone(), target, factor);
    apply_free_snapshot(free_cam, snapshot.clone());
    *last_written = snapshot;

    if progress >= 1.0 {
        commands.trigger(CameraMoveEnd {
            camera:      entity,
            camera_move: current_move.clone(),
        });
        queue.camera_moves.pop_front();
        queue.move_state = MoveState::Ready;
    }
}

/// System that processes camera movement queues with duration-based interpolation.
///
/// When a `OrbitCam` has a `CameraMoveList`, interpolates toward the target over
/// the specified duration with easing. When a move completes, automatically moves to the
/// next. Removes the `CameraMoveList` component when all moves are complete.
pub(crate) fn process_orbit_camera_move_list(
    mut commands: Commands,
    time: Res<Time>,
    mut camera_query: Query<(
        Entity,
        &mut OrbitCam,
        &mut CameraMoveList,
        &mut OrbitCamInput,
        &CameraInputInterruptBehavior,
        Option<&ZoomAnimationMarker>,
        Option<&AnimationSourceMarker>,
    )>,
) {
    for (
        entity,
        mut pan_orbit,
        mut queue,
        mut input,
        interrupt_behavior,
        zoom_marker,
        source_marker,
    ) in &mut camera_query
    {
        let source = source_marker.map_or(AnimationSource::PlayAnimation, |m| m.source);
        let target = source_marker.and_then(|m| m.target);

        let Some(current_move) = queue.camera_moves.front().cloned() else {
            handle_empty_queue(&mut commands, entity, source, target, zoom_marker);
            continue;
        };

        if input.has_input() || queue.move_state.orbit_externally_modified(&pan_orbit) {
            let outcome = handle_orbit_camera_input_interrupt(
                &mut commands,
                entity,
                &mut pan_orbit,
                &InterruptContext {
                    queue: &queue,
                    interrupt_behavior: *interrupt_behavior,
                    source,
                    target,
                    current_move: &current_move,
                    zoom_marker,
                },
            );
            match outcome {
                CameraInputInterruptBehavior::Ignore => {
                    input.clear();
                },
                CameraInputInterruptBehavior::Cancel | CameraInputInterruptBehavior::Complete => {
                    if outcome == CameraInputInterruptBehavior::Complete {
                        input.clear();
                    }
                    continue;
                },
            }
        }

        match &queue.move_state {
            MoveState::Ready => {
                handle_orbit_ready_state(
                    &mut commands,
                    entity,
                    &mut pan_orbit,
                    &mut queue,
                    &current_move,
                );
            },
            MoveState::OrbitInProgress { .. } => {
                handle_orbit_in_progress(
                    &mut commands,
                    entity,
                    &mut pan_orbit,
                    &mut queue,
                    &current_move,
                    time.delta_secs(),
                );
            },
            MoveState::FreeInProgress { .. } => {},
        }
    }
}

/// System that processes `FreeCam` movement queues with duration-based interpolation.
pub(crate) fn process_free_camera_move_list(
    mut commands: Commands,
    time: Res<Time>,
    mut camera_query: Query<(
        Entity,
        &mut FreeCam,
        &CameraBasis,
        &mut CameraMoveList,
        &mut FreeCamInput,
        &CameraInputInterruptBehavior,
        Option<&ZoomAnimationMarker>,
        Option<&AnimationSourceMarker>,
    )>,
) {
    for (
        entity,
        mut free_cam,
        basis,
        mut queue,
        mut input,
        interrupt_behavior,
        zoom_marker,
        source_marker,
    ) in &mut camera_query
    {
        let source = source_marker.map_or(AnimationSource::PlayAnimation, |m| m.source);
        let target = source_marker.and_then(|m| m.target);

        let Some(current_move) = queue.camera_moves.front().cloned() else {
            handle_empty_queue(&mut commands, entity, source, target, zoom_marker);
            continue;
        };

        if input.has_input() || queue.move_state.free_externally_modified(&free_cam) {
            let outcome = handle_free_camera_input_interrupt(
                &mut commands,
                entity,
                &mut free_cam,
                *basis,
                &InterruptContext {
                    queue: &queue,
                    interrupt_behavior: *interrupt_behavior,
                    source,
                    target,
                    current_move: &current_move,
                    zoom_marker,
                },
            );
            match outcome {
                CameraInputInterruptBehavior::Ignore => {
                    input.clear();
                },
                CameraInputInterruptBehavior::Cancel | CameraInputInterruptBehavior::Complete => {
                    if outcome == CameraInputInterruptBehavior::Complete {
                        input.clear();
                    }
                    continue;
                },
            }
        }

        match &queue.move_state {
            MoveState::Ready => {
                handle_free_ready_state(
                    &mut commands,
                    entity,
                    &mut free_cam,
                    *basis,
                    &mut queue,
                    &current_move,
                );
            },
            MoveState::FreeInProgress { .. } => {
                handle_free_in_progress(
                    &mut commands,
                    entity,
                    &mut free_cam,
                    *basis,
                    &mut queue,
                    &current_move,
                    time.delta_secs(),
                );
            },
            MoveState::OrbitInProgress { .. } => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use bevy::camera::RenderTarget;
    use bevy::input::gestures::PinchGesture;
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::input::mouse::AccumulatedMouseScroll;
    use bevy::input::mouse::MouseScrollUnit;
    use bevy::window::WindowRef;
    use bevy_enhanced_input::prelude::EnhancedInputPlugin;
    use bevy_enhanced_input::prelude::InputContextAppExt;

    use super::*;
    use crate::input::CameraInputLifecyclePlugin;
    use crate::input::CameraInputModesPlugin;
    use crate::input::CameraInputRoutingConfig;
    use crate::input::CameraInputRoutingPlugin;
    use crate::input::InputGain;
    use crate::input::InteractionSources;
    use crate::input::OrbitCamInputContext;
    use crate::input::OrbitCamInputGain;
    use crate::input::OrbitCamInputMode;
    use crate::input::OrbitCamPreset;
    use crate::input::OrbitCamSimpleMousePreset;
    use crate::input::TouchTracker;
    use crate::orbit_cam::OrbitCamInputAdapterPlugin;
    use crate::system_sets::LagrangeSystemSetsPlugin;

    const ANIMATION_FOCUS: Vec3 = Vec3::ZERO;
    const ANIMATION_PITCH: f32 = 0.0;
    const ANIMATION_RADIUS: f32 = 2.0;
    const ANIMATION_YAW: f32 = 1.0;
    const MOVE_DURATION_MILLIS: u64 = 1_000;
    const INTERRUPT_DELTA: Vec2 = Vec2::X;
    const FINAL_FOCUS: Vec3 = Vec3::new(1.0, 2.0, 3.0);
    const FINAL_YAW: f32 = 0.75;
    const FINAL_PITCH: f32 = 0.25;
    const FINAL_RADIUS: f32 = 5.0;
    const FREE_INITIAL_POSITION: Vec3 = Vec3::new(0.0, 0.0, 8.0);
    const FREE_TARGET_FOCUS: Vec3 = Vec3::ZERO;
    const FREE_TARGET_POSITION: Vec3 = Vec3::new(1.0, 2.0, 3.0);
    const FREE_TARGET_ROLL: f32 = 0.5;
    const SMOOTH_SCROLL_DELTA: Vec2 = Vec2::new(0.0, 8.0);

    #[derive(Resource, Default)]
    struct AnimationEventCounts {
        cancelled: usize,
        completed: usize,
    }

    type TestResult = Result<(), &'static str>;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<AnimationEventCounts>()
            .add_systems(Update, process_orbit_camera_move_list)
            .add_systems(Update, process_free_camera_move_list);
        app
    }

    fn input_pipeline_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            EnhancedInputPlugin,
            LagrangeSystemSetsPlugin,
            CameraInputModesPlugin,
            CameraInputRoutingPlugin,
            OrbitCamInputAdapterPlugin,
            CameraInputLifecyclePlugin,
        ))
        .add_input_context::<OrbitCamInputContext>()
        .init_resource::<AnimationEventCounts>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AccumulatedMouseMotion>()
        .init_resource::<AccumulatedMouseScroll>()
        .init_resource::<TouchTracker>()
        .add_message::<PinchGesture>()
        .add_systems(Update, process_orbit_camera_move_list);
        app.finish();
        app
    }

    fn camera_move(focus: Vec3, yaw: f32, pitch: f32, radius: f32) -> CameraMove {
        CameraMove::ToOrbitalLookAt {
            target: focus,
            yaw,
            pitch,
            radius,
            roll: None,
            duration: Duration::from_millis(MOVE_DURATION_MILLIS),
            easing: EaseFunction::Linear,
        }
    }

    fn free_camera_move() -> CameraMove {
        CameraMove::ToLookAt {
            position: FREE_TARGET_POSITION,
            target:   FREE_TARGET_FOCUS,
            roll:     Some(Roll(FREE_TARGET_ROLL)),
            duration: Duration::ZERO,
            easing:   EaseFunction::Linear,
        }
    }

    fn spawn_animated_camera(
        app: &mut App,
        interrupt_behavior: CameraInputInterruptBehavior,
        camera_moves: VecDeque<CameraMove>,
    ) -> Entity {
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                CameraMoveList::new(camera_moves),
                interrupt_behavior,
            ))
            .id();
        observe_animation_events(app.world_mut(), camera);
        camera
    }

    fn spawn_pipeline_animated_camera(
        app: &mut App,
        interrupt_behavior: CameraInputInterruptBehavior,
        orbit_cam_input_mode: OrbitCamInputMode,
    ) -> Entity {
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                CameraMoveList::new(VecDeque::from([camera_move(
                    ANIMATION_FOCUS,
                    ANIMATION_YAW,
                    ANIMATION_PITCH,
                    ANIMATION_RADIUS,
                )])),
                interrupt_behavior,
                orbit_cam_input_mode,
            ))
            .id();
        observe_animation_events(app.world_mut(), camera);
        camera
    }

    fn observe_animation_events(world: &mut World, camera: Entity) {
        world.entity_mut(camera).observe(
            |event: On<AnimationEnd>, mut counts: ResMut<AnimationEventCounts>| match event.reason {
                AnimationReason::Cancelled { .. } => counts.cancelled += 1,
                AnimationReason::Completed => counts.completed += 1,
            },
        );
    }

    #[test]
    fn free_camera_consumes_zero_duration_move_list() -> TestResult {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((
                FreeCam::from_pose(
                    FREE_INITIAL_POSITION,
                    LookAngles::default(),
                    Roll::default(),
                ),
                CameraBasis::default(),
                FreeCamInput::default(),
                CameraMoveList::new(VecDeque::from([free_camera_move()])),
                CameraInputInterruptBehavior::Ignore,
            ))
            .id();

        app.update();

        let free_cam = app
            .world()
            .get::<FreeCam>(camera)
            .ok_or("camera missing FreeCam")?;
        assert_eq!(free_cam.translate.target(), Position(FREE_TARGET_POSITION));
        assert_eq!(free_cam.roll.target(), Roll(FREE_TARGET_ROLL));

        let queue = app
            .world()
            .get::<CameraMoveList>(camera)
            .ok_or("camera missing CameraMoveList")?;
        assert!(queue.camera_moves.is_empty());

        app.update();

        assert!(app.world().get::<CameraMoveList>(camera).is_none());
        Ok(())
    }

    fn add_interrupt_input(app: &mut App, camera: Entity) -> TestResult {
        app.world_mut()
            .get_mut::<OrbitCamInput>(camera)
            .ok_or("camera missing OrbitCamInput")?
            .add_orbit_with_sources(INTERRUPT_DELTA, InteractionSources::MOUSE);
        Ok(())
    }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    fn add_mouse_drag_and_smooth_scroll_input(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = INTERRUPT_DELTA;
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: SMOOTH_SCROLL_DELTA,
        };
    }

    fn zero_sensitive_simple_mouse_mode() -> OrbitCamInputMode {
        let disabled = InputGain::DISABLED.0;
        OrbitCamInputMode::with_preset(
            OrbitCamSimpleMousePreset::default()
                .mouse_input_gain(OrbitCamInputGain::uniform(disabled))
                .smooth_scroll_input_gain(OrbitCamInputGain::uniform(disabled)),
        )
    }

    #[test]
    fn cancel_interrupt_removes_animation_and_emits_cancelled_event() -> TestResult {
        let mut app = test_app();
        let camera = spawn_animated_camera(
            &mut app,
            CameraInputInterruptBehavior::Cancel,
            VecDeque::from([camera_move(Vec3::ZERO, 1.0, 0.0, 2.0)]),
        );
        add_interrupt_input(&mut app, camera)?;

        app.update();

        let counts = app.world().resource::<AnimationEventCounts>();
        assert_eq!(counts.cancelled, 1);
        assert_eq!(counts.completed, 0);
        assert!(app.world().get::<CameraMoveList>(camera).is_none());
        Ok(())
    }

    #[test]
    fn complete_interrupt_jumps_to_final_move_and_clears_input() -> TestResult {
        let mut app = test_app();
        let camera = spawn_animated_camera(
            &mut app,
            CameraInputInterruptBehavior::Complete,
            VecDeque::from([
                camera_move(Vec3::ZERO, 1.0, 0.0, 2.0),
                camera_move(FINAL_FOCUS, FINAL_YAW, FINAL_PITCH, FINAL_RADIUS),
            ]),
        );
        add_interrupt_input(&mut app, camera)?;

        app.update();

        let counts = app.world().resource::<AnimationEventCounts>();
        assert_eq!(counts.cancelled, 0);
        assert_eq!(counts.completed, 1);
        assert!(app.world().get::<CameraMoveList>(camera).is_none());
        assert!(
            !app.world()
                .get::<OrbitCamInput>(camera)
                .ok_or("camera missing OrbitCamInput")?
                .has_input()
        );

        let orbit_cam = app
            .world()
            .get::<OrbitCam>(camera)
            .ok_or("camera missing OrbitCam")?;
        assert_eq!(orbit_cam.pan.target().0, FINAL_FOCUS);
        assert_f32_close(orbit_cam.orbit.target().yaw, FINAL_YAW);
        assert_f32_close(orbit_cam.orbit.target().pitch, FINAL_PITCH);
        assert_f32_close(orbit_cam.zoom.target().0, FINAL_RADIUS);
        Ok(())
    }

    #[test]
    fn ignore_interrupt_clears_input_and_keeps_animation() -> TestResult {
        let mut app = test_app();
        let camera = spawn_animated_camera(
            &mut app,
            CameraInputInterruptBehavior::Ignore,
            VecDeque::from([camera_move(Vec3::ZERO, 1.0, 0.0, 2.0)]),
        );
        add_interrupt_input(&mut app, camera)?;

        app.update();

        let counts = app.world().resource::<AnimationEventCounts>();
        assert_eq!(counts.cancelled, 0);
        assert_eq!(counts.completed, 0);
        assert!(app.world().get::<CameraMoveList>(camera).is_some());
        assert!(
            !app.world()
                .get::<OrbitCamInput>(camera)
                .ok_or("camera missing OrbitCamInput")?
                .has_input()
        );
        Ok(())
    }

    #[test]
    fn zero_sensitive_preset_input_does_not_interrupt_animation() -> TestResult {
        for interrupt_behavior in [
            CameraInputInterruptBehavior::Cancel,
            CameraInputInterruptBehavior::Complete,
            CameraInputInterruptBehavior::Ignore,
        ] {
            let mut app = input_pipeline_app();
            let camera = spawn_pipeline_animated_camera(
                &mut app,
                interrupt_behavior,
                zero_sensitive_simple_mouse_mode(),
            );
            app.insert_resource(CameraInputRoutingConfig::explicit(camera));
            app.update();

            add_mouse_drag_and_smooth_scroll_input(&mut app);
            app.update();

            let counts = app.world().resource::<AnimationEventCounts>();
            assert_eq!(counts.cancelled, 0);
            assert_eq!(counts.completed, 0);
            assert!(app.world().get::<CameraMoveList>(camera).is_some());
            assert!(
                !app.world()
                    .get::<OrbitCamInput>(camera)
                    .ok_or("camera missing OrbitCamInput")?
                    .has_input()
            );
        }
        Ok(())
    }

    #[test]
    fn nonzero_preset_input_interrupts_animation() -> TestResult {
        for interrupt_behavior in [
            CameraInputInterruptBehavior::Cancel,
            CameraInputInterruptBehavior::Complete,
            CameraInputInterruptBehavior::Ignore,
        ] {
            let mut app = input_pipeline_app();
            let camera = spawn_pipeline_animated_camera(
                &mut app,
                interrupt_behavior,
                OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
            );
            app.insert_resource(CameraInputRoutingConfig::explicit(camera));
            app.update();

            add_mouse_drag_and_smooth_scroll_input(&mut app);
            app.update();

            let counts = app.world().resource::<AnimationEventCounts>();
            match interrupt_behavior {
                CameraInputInterruptBehavior::Cancel => {
                    assert_eq!(counts.cancelled, 1);
                    assert_eq!(counts.completed, 0);
                    assert!(app.world().get::<CameraMoveList>(camera).is_none());
                },
                CameraInputInterruptBehavior::Complete => {
                    assert_eq!(counts.cancelled, 0);
                    assert_eq!(counts.completed, 1);
                    assert!(app.world().get::<CameraMoveList>(camera).is_none());
                    assert!(
                        !app.world()
                            .get::<OrbitCamInput>(camera)
                            .ok_or("camera missing OrbitCamInput")?
                            .has_input()
                    );
                },
                CameraInputInterruptBehavior::Ignore => {
                    assert_eq!(counts.cancelled, 0);
                    assert_eq!(counts.completed, 0);
                    assert!(app.world().get::<CameraMoveList>(camera).is_some());
                    assert!(
                        !app.world()
                            .get::<OrbitCamInput>(camera)
                            .ok_or("camera missing OrbitCamInput")?
                            .has_input()
                    );
                },
            }
        }
        Ok(())
    }
}

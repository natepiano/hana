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

use super::OrbitCam;
use super::constants::EXTERNAL_INPUT_TOLERANCE;
use super::constants::MILLIS_PER_SECOND;
use super::events::AnimationEnd;
use super::events::AnimationReason;
use super::events::AnimationSource;
use super::events::CameraMoveBegin;
use super::events::CameraMoveEnd;
use super::events::ZoomContext;
use super::events::ZoomEnd;
use super::events::ZoomReason;
use super::input::OrbitCamInput;

/// Tracks a zoom-to-fit operation routed through the animation system.
///
/// When `AnimationEnd` fires on an entity with this marker, `ZoomEnd` is triggered and the
/// marker is removed. Wraps the [`ZoomContext`] that originated the zoom.
#[derive(Component, Clone)]
pub(crate) struct ZoomAnimationMarker(pub(crate) ZoomContext);

/// Tracks which trigger source started the current animation.
///
/// Records whether the animation was triggered by [`PlayAnimation`](crate::PlayAnimation),
/// [`ZoomToFit`](crate::ZoomToFit), or [`AnimateToFit`](crate::AnimateToFit), plus the entity it
/// frames (if any). Inserted alongside [`CameraMoveList`] and removed when the animation ends or is
/// cancelled. The recorded `target` lets the end events report the framed entity even though the
/// queue no longer carries it.
#[derive(Component)]
pub(crate) struct AnimationSourceMarker {
    pub(crate) source: AnimationSource,
    pub(crate) target: Option<Entity>,
}

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
#[derive(Clone, Debug, Reflect)]
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
                let yaw_rotation = Quat::from_axis_angle(Vec3::Y, *yaw);
                let pitch_rotation = Quat::from_axis_angle(Vec3::X, -*pitch);
                let rotation = yaw_rotation * pitch_rotation;
                *focus + rotation * Vec3::new(0.0, 0.0, *radius)
            },
        }
    }

    /// Returns the target orbital parameters (yaw, pitch, radius).
    /// For `ToPosition`, decomposes from the world-space offset (may lose yaw at ±PI/2 pitch).
    fn orbital_parameters(&self) -> (f32, f32, f32) {
        match self {
            Self::ToPosition {
                translation, focus, ..
            } => orbital_parameters_from_offset(Displacement(*translation - *focus)),
            Self::ToOrbit {
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
    InProgress {
        elapsed_millis: f32,
        start:          OrbitSnapshot,
        /// Values written by the animation last frame — if the camera's current
        /// values differ, external input occurred and the animation may interrupt
        /// depending on `CameraInputInterruptBehavior`.
        last_written:   OrbitSnapshot,
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

/// Controls what happens when user input occurs during an in-flight animation.
///
/// Specifically, this governs **user input to the camera** (orbit, pan, zoom) while an
/// animation is playing.
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
            MoveState::InProgress { elapsed_millis, .. } => {
                self.camera_moves.front().map_or(0.0, |current_move| {
                    (current_move.duration_ms() - elapsed_millis).max(0.0)
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
    target: Option<Entity>,
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
        target,
        reason: AnimationReason::Completed,
    });
    if let Some(marker) = zoom_marker {
        commands.entity(entity).remove::<ZoomAnimationMarker>();
        commands.trigger(ZoomEnd {
            camera:   entity,
            target:   marker.0.target,
            margin:   marker.0.margin,
            duration: marker.0.duration,
            easing:   marker.0.easing,
            reason:   ZoomReason::Completed,
        });
    }
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
fn handle_camera_input_interrupt(
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
            // Stop where we are — fire end events with `Cancelled` reason
            commands
                .entity(entity)
                .remove::<(CameraMoveList, AnimationSourceMarker)>();
            commands.trigger(AnimationEnd {
                camera: entity,
                source,
                target,
                reason: AnimationReason::Cancelled {
                    interrupted_move: current_move.clone(),
                },
            });
            if let Some(marker) = zoom_marker {
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
            CameraInputInterruptBehavior::Cancel
        },
        CameraInputInterruptBehavior::Complete => {
            // Jump to the final position of the entire queue
            if let Some(final_move) = interrupt_context.queue.camera_moves.back() {
                let (yaw, pitch, radius) = final_move.orbital_parameters();
                pan_orbit.target_focus = final_move.focus();
                pan_orbit.target_yaw = yaw;
                pan_orbit.target_pitch = pitch;
                pan_orbit.target_radius = radius;
            }
            // Fire normal end events
            commands
                .entity(entity)
                .remove::<(CameraMoveList, AnimationSourceMarker)>();
            commands.trigger(AnimationEnd {
                camera: entity,
                source,
                target,
                reason: AnimationReason::Completed,
            });
            if let Some(marker) = zoom_marker {
                commands.entity(entity).remove::<ZoomAnimationMarker>();
                commands.trigger(ZoomEnd {
                    camera:   entity,
                    target:   marker.0.target,
                    margin:   marker.0.margin,
                    duration: marker.0.duration,
                    easing:   marker.0.easing,
                    reason:   ZoomReason::Completed,
                });
            }
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
fn handle_ready_state(
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
        pan_orbit.target_focus = current_move.focus();
        pan_orbit.target_radius = target_radius;
        pan_orbit.target_yaw = target_yaw;
        pan_orbit.target_pitch = target_pitch;

        commands.trigger(CameraMoveEnd {
            camera:      entity,
            camera_move: current_move.clone(),
        });
        queue.camera_moves.pop_front();
        return ReadyMoveOutcome::ConsumedZeroDuration;
    }

    // Transition to `InProgress` with captured starting orbital parameters
    let current_orbit = OrbitSnapshot::new(
        Position(pan_orbit.target_focus),
        pan_orbit.target_yaw,
        pan_orbit.target_pitch,
        pan_orbit.target_radius,
    );
    queue.move_state = MoveState::InProgress {
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
fn handle_in_progress(
    commands: &mut Commands,
    entity: Entity,
    pan_orbit: &mut OrbitCam,
    queue: &mut CameraMoveList,
    current_move: &CameraMove,
    delta_secs: f32,
) {
    let MoveState::InProgress {
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
    // `ToOrbit` provides them directly; `ToPosition` decomposes via atan2
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

    // `ToPosition` and `ToOrbit` are both normalized to orbital parameters above
    pan_orbit.target_focus = Vec3::lerp(*start.focus, current_move.focus(), interpolation_factor);
    pan_orbit.target_radius =
        (canonical_radius - start.radius).mul_add(interpolation_factor, start.radius);
    pan_orbit.target_yaw = yaw_diff.mul_add(interpolation_factor, start.yaw);
    pan_orbit.target_pitch = pitch_diff.mul_add(interpolation_factor, start.pitch);

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
        queue.move_state = MoveState::Ready;
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

        if input.has_input() || queue.move_state.externally_modified(&pan_orbit) {
            let outcome = handle_camera_input_interrupt(
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
    use crate::input::CameraInputRoutingConfig;
    use crate::input::CameraInteractionSources;
    use crate::input::InputGain;
    use crate::input::OrbitCamInputAdapterPlugin;
    use crate::input::OrbitCamInputContext;
    use crate::input::OrbitCamInputGain;
    use crate::input::OrbitCamInputLifecyclePlugin;
    use crate::input::OrbitCamInputMode;
    use crate::input::OrbitCamInputModesPlugin;
    use crate::input::OrbitCamPreset;
    use crate::input::OrbitCamRoutingPlugin;
    use crate::input::OrbitCamSimpleMousePreset;
    use crate::input::TouchTracker;
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
            .add_systems(Update, process_camera_move_list);
        app
    }

    fn input_pipeline_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            EnhancedInputPlugin,
            LagrangeSystemSetsPlugin,
            OrbitCamInputModesPlugin,
            OrbitCamRoutingPlugin,
            OrbitCamInputAdapterPlugin,
            OrbitCamInputLifecyclePlugin,
        ))
        .add_input_context::<OrbitCamInputContext>()
        .init_resource::<AnimationEventCounts>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AccumulatedMouseMotion>()
        .init_resource::<AccumulatedMouseScroll>()
        .init_resource::<TouchTracker>()
        .add_message::<PinchGesture>()
        .add_systems(Update, process_camera_move_list);
        app.finish();
        app
    }

    fn camera_move(focus: Vec3, yaw: f32, pitch: f32, radius: f32) -> CameraMove {
        CameraMove::ToOrbit {
            focus,
            yaw,
            pitch,
            radius,
            duration: Duration::from_millis(MOVE_DURATION_MILLIS),
            easing: EaseFunction::Linear,
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
        mode: OrbitCamInputMode,
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
                mode,
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

    fn add_interrupt_input(app: &mut App, camera: Entity) -> TestResult {
        app.world_mut()
            .get_mut::<OrbitCamInput>(camera)
            .ok_or("camera missing OrbitCamInput")?
            .orbit_pixels_with_sources(INTERRUPT_DELTA, CameraInteractionSources::MOUSE);
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
        assert_eq!(orbit_cam.target_focus, FINAL_FOCUS);
        assert_f32_close(orbit_cam.target_yaw, FINAL_YAW);
        assert_f32_close(orbit_cam.target_pitch, FINAL_PITCH);
        assert_f32_close(orbit_cam.target_radius, FINAL_RADIUS);
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

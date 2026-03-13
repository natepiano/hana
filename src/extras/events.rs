//! Events for camera animations and zoom operations.
//!
//! Events are organized by feature. Each group starts with the **trigger** event
//! (fire with `commands.trigger(...)`) followed by the **fired** events it produces
//! (observe with `.add_observer(...)`).
//!
//! # Common patterns
//!
//! **Duration** — several events accept a `duration` field. When set to
//! `Duration::ZERO` the operation completes instantly — the camera snaps to its
//! final position and only the **operation-level** begin/end events fire (see
//! [instant paths](#instant-operations) below). When `duration > Duration::ZERO`
//! the operation animates over time through [`PlayAnimation`], so the full nested
//! event sequence fires.
//!
//! **Easing** — events that animate also accept an `easing` field
//! ([`EaseFunction`]) that controls the interpolation curve. This only has an effect
//! when `duration > Duration::ZERO`.
//!
//! # Event ordering
//!
//! Events nest from outermost (operation-level) to innermost (move-level). Every
//! animated path goes through [`PlayAnimation`], so [`AnimationBegin`]/[`AnimationEnd`]
//! and [`CameraMoveBegin`]/[`CameraMoveEnd`] fire for **all** animated operations —
//! including [`ZoomToFit`] and [`AnimateToFit`].
//!
//! ## `PlayAnimation` — normal completion
//!
//! ```text
//! AnimationBegin → CameraMoveBegin → CameraMoveEnd → … → AnimationEnd
//! ```
//!
//! ## `ZoomToFit` (animated) — normal completion
//!
//! `Zoom*` events wrap the animation lifecycle:
//!
//! ```text
//! ZoomBegin → AnimationBegin → CameraMoveBegin → CameraMoveEnd → AnimationEnd → ZoomEnd
//! ```
//!
//! ## `AnimateToFit` (animated) — normal completion
//!
//! No extra wrapping events — uses `source: AnimationSource::AnimateToFit` to
//! distinguish from a plain [`PlayAnimation`]:
//!
//! ```text
//! AnimationBegin → CameraMoveBegin → CameraMoveEnd → AnimationEnd
//! ```
//!
//! ## Instant operations
//!
//! When `duration` is `Duration::ZERO`, the animation system is bypassed entirely.
//! Only the operation-level events fire — no [`AnimationBegin`]/[`AnimationEnd`] or
//! [`CameraMoveBegin`]/[`CameraMoveEnd`].
//!
//! ### `ZoomToFit` (instant)
//!
//! ```text
//! ZoomBegin → ZoomEnd
//! ```
//!
//! ### `AnimateToFit` (instant)
//!
//! Fires animation-level events (to notify observers) but no camera-move-level events:
//!
//! ```text
//! AnimationBegin → AnimationEnd
//! ```
//!
//! ## User input interruption ([`CameraInputInterruptBehavior`](crate::CameraInputInterruptBehavior))
//!
//! When the user physically moves the camera during an animation:
//!
//! - **`Ignore`** (default) — temporarily disables camera input and continues animating:
//!
//!   ```text
//!   … (no interrupt lifecycle event)
//!   ```
//!
//! - **`Cancel`** — stops where it is:
//!
//!   ```text
//!   … → AnimationCancelled → ZoomCancelled (if zoom)
//!   ```
//!
//! - **`Complete`** — jumps to the final position:
//!
//!   ```text
//!   … → AnimationEnd → ZoomEnd (if zoom)
//!   ```
//!
//! ## Animation conflict ([`AnimationConflictPolicy`](crate::AnimationConflictPolicy))
//!
//! When a new animation request arrives while one is already in-flight:
//!
//! - **`LastWins`** (default) — cancels the in-flight animation, then starts the new one.
//!   `AnimationCancelled` always fires; `ZoomCancelled` additionally fires if the in-flight
//!   operation is a zoom:
//!
//!   ```text
//!   AnimationCancelled → ZoomCancelled (if zoom) → AnimationBegin (new) → …
//!   ```
//!
//! - **`FirstWins`** — rejects the incoming request. No zoom lifecycle events fire — the rejection
//!   is detected before `ZoomBegin`:
//!
//!   ```text
//!   AnimationRejected
//!   ```
//!
//!   The [`AnimationRejected::source`] field identifies what was rejected
//!   ([`AnimationSource::PlayAnimation`], [`AnimationSource::ZoomToFit`], or
//!   [`AnimationSource::AnimateToFit`]).
//!
//! # Emitted event data
//!
//! Reference of data carried by events — for comparison purposes.
//!
//! | Event                    | `camera` | `target` | `margin` | `duration` | `easing` | `source` | `camera_move` |
//! |--------------------------|-----------------|-----------------|----------|------------|----------|----------|---------------|
//! | [`ZoomBegin`]            | yes             | yes             | yes      | yes        | yes      | —        | —             |
//! | [`ZoomEnd`]              | yes             | yes             | yes      | yes        | yes      | —        | —             |
//! | [`ZoomCancelled`]        | yes             | yes             | yes      | yes        | yes      | —        | —             |
//! | [`AnimationBegin`]       | yes             | —               | —        | —          | —        | yes      | —             |
//! | [`AnimationEnd`]         | yes             | —               | —        | —          | —        | yes      | —             |
//! | [`AnimationCancelled`]   | yes             | —               | —        | —          | —        | yes      | yes           |
//! | [`AnimationRejected`]    | yes             | —               | —        | —          | —        | yes      | —             |
//! | [`CameraMoveBegin`]      | yes             | —               | —        | —          | —        | —        | yes           |
//! | [`CameraMoveEnd`]        | yes             | —               | —        | —          | —        | —        | yes           |

use std::collections::VecDeque;
use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use super::animation::CameraMove;

/// Context for a zoom-to-fit operation, passed through [`PlayAnimation`] so
/// that `on_play_animation` can fire [`ZoomBegin`] and insert
/// [`ZoomAnimationMarker`](super::components::ZoomAnimationMarker) at the
/// single point where conflict resolution has already completed.
#[derive(Clone, Reflect)]
pub struct ZoomContext {
    /// The entity being framed.
    pub target: Entity,
    /// The margin from the triggering [`ZoomToFit`].
    pub margin: f32,
    /// The duration from the triggering [`ZoomToFit`].
    pub duration: Duration,
    /// The easing curve from the triggering [`ZoomToFit`].
    pub easing: EaseFunction,
}

/// Identifies which event triggered an animation lifecycle.
///
/// Carried by [`AnimationBegin`], [`AnimationEnd`], [`AnimationCancelled`], and
/// [`AnimationRejected`] so observers know whether the animation originated from
/// [`PlayAnimation`], [`ZoomToFit`], or [`AnimateToFit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum AnimationSource {
    /// Animation was triggered by [`PlayAnimation`].
    PlayAnimation,
    /// Animation was triggered by [`ZoomToFit`].
    ZoomToFit,
    /// Animation was triggered by [`AnimateToFit`].
    AnimateToFit,
    /// Animation was triggered by [`LookAt`].
    LookAt,
    /// Animation was triggered by [`LookAtAndZoomToFit`].
    LookAtAndZoomToFit,
}

/// `ZoomToFit` — frames a target entity in the camera view without changing the
/// camera's viewing angle.
///
/// The camera's yaw and pitch stay fixed. Only the focus and radius change so
/// that the target fills the viewport with the requested margin. Because the
/// viewing angle is preserved, the camera *translates* to a new position rather
/// than rotating — if the target is off to the side, the view slides over to it.
///
/// # See also
///
/// - [`LookAt`] — keeps the camera in place and *rotates* to face the target (no framing / radius
///   adjustment).
/// - [`LookAtAndZoomToFit`] — *rotates* to face the target and adjusts radius to frame it. Use this
///   when you want the camera to turn toward the target instead of sliding.
/// - [`AnimateToFit`] — frames the target from a caller-specified viewing angle.
///
/// # Fields
///
/// - `camera` — the entity with a `PanOrbitCamera` component.
/// - `target` — the entity to frame; must have a `Mesh3d` (direct or on descendants).
/// - `margin` — total fraction of the screen to leave as space between the target's screen-space
///   bounding box and the screen edge, split equally across both sides of the constraining
///   dimension (e.g. `0.25` → ~12.5% each side).
/// - `duration` — see module-level docs on **Duration**.
/// - `easing` — see module-level docs on **Easing**.
///
/// Animated zooms route through [`PlayAnimation`], so the full event sequence is
/// `ZoomBegin` → `AnimationBegin` → `CameraMoveBegin` → `CameraMoveEnd` →
/// `AnimationEnd` → `ZoomEnd`. See the [module-level event ordering](self#event-ordering)
/// docs for interruption and conflict scenarios.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomToFit {
    /// The camera entity to zoom.
    #[event_target]
    pub camera: Entity,
    /// The entity to frame.
    pub target: Entity,
    /// Fraction of screen to leave as margin.
    pub margin: f32,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing: EaseFunction,
}

impl ZoomToFit {
    /// Creates a new `ZoomToFit` event with default margin, instant duration, and cubic-out easing.
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            margin: 0.1,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the margin.
    pub const fn margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Sets the animation duration.
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the easing function.
    pub const fn easing(mut self, easing: EaseFunction) -> Self {
        self.easing = easing;
        self
    }
}

/// `ZoomBegin` — emitted when a [`ZoomToFit`] operation begins.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomBegin {
    /// The camera that is zooming.
    #[event_target]
    pub camera: Entity,
    /// The entity being framed.
    pub target: Entity,
    /// The margin from the triggering [`ZoomToFit`].
    pub margin: f32,
    /// The duration from the triggering [`ZoomToFit`].
    pub duration: Duration,
    /// The easing curve from the triggering [`ZoomToFit`].
    pub easing: EaseFunction,
}

/// `ZoomEnd` — emitted when a [`ZoomToFit`] operation completes (both animated and instant).
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomEnd {
    /// The camera that finished zooming.
    #[event_target]
    pub camera: Entity,
    /// The entity that was framed.
    pub target: Entity,
    /// The margin from the triggering [`ZoomToFit`].
    pub margin: f32,
    /// The duration from the triggering [`ZoomToFit`].
    pub duration: Duration,
    /// The easing curve from the triggering [`ZoomToFit`].
    pub easing: EaseFunction,
}

/// `ZoomCancelled` — emitted when a [`ZoomToFit`] animation is cancelled before completion.
/// The camera stays at its current position — no snap to final.
///
/// Cancellation happens in two scenarios:
/// - **User input** — the user physically moves the camera while
///   [`CameraInputInterruptBehavior::Cancel`](crate::CameraInputInterruptBehavior::Cancel) is
///   active.
/// - **Animation conflict** — a new animation request arrives while
///   [`AnimationConflictPolicy::LastWins`](crate::AnimationConflictPolicy::LastWins) is active,
///   cancelling the in-flight zoom.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomCancelled {
    /// The camera whose zoom was cancelled.
    #[event_target]
    pub camera: Entity,
    /// The entity that was being framed.
    pub target: Entity,
    /// The margin from the triggering [`ZoomToFit`].
    pub margin: f32,
    /// The duration from the triggering [`ZoomToFit`].
    pub duration: Duration,
    /// The easing curve from the triggering [`ZoomToFit`].
    pub easing: EaseFunction,
}

/// `PlayAnimation` — plays a queued sequence of [`CameraMove`] steps.
///
/// Fires `AnimationBegin` → (`CameraMoveBegin` → `CameraMoveEnd`) × N → `AnimationEnd`.
/// See the [module-level event ordering](self#event-ordering) docs for interruption and
/// conflict scenarios.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct PlayAnimation {
    /// The camera entity to animate.
    #[event_target]
    pub camera: Entity,
    /// The queue of camera movements.
    pub camera_moves: VecDeque<CameraMove>,
    /// The source of this animation.
    pub source: AnimationSource,
    /// Optional zoom context when this animation originates from [`ZoomToFit`].
    pub zoom_context: Option<ZoomContext>,
}

impl PlayAnimation {
    /// Creates a new `PlayAnimation` event.
    pub fn new(camera: Entity, camera_moves: impl IntoIterator<Item = CameraMove>) -> Self {
        Self {
            camera,
            camera_moves: camera_moves.into_iter().collect(),
            source: AnimationSource::PlayAnimation,
            zoom_context: None,
        }
    }

    /// Sets the animation source.
    pub fn source(mut self, source: AnimationSource) -> Self {
        self.source = source;
        self
    }

    /// Sets the zoom context (implies `AnimationSource::ZoomToFit`).
    pub fn zoom_context(mut self, ctx: ZoomContext) -> Self {
        self.zoom_context = Some(ctx);
        self.source = AnimationSource::ZoomToFit;
        self
    }
}

/// `AnimationBegin` — emitted when a `CameraMoveList` begins processing.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimationBegin {
    /// The camera being animated.
    #[event_target]
    pub camera: Entity,
    /// Whether this animation originated from [`PlayAnimation`], [`ZoomToFit`], or
    /// [`AnimateToFit`].
    pub source: AnimationSource,
}

/// `AnimationEnd` — emitted when a `CameraMoveList` finishes all its queued moves.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimationEnd {
    /// The camera that finished animating.
    #[event_target]
    pub camera: Entity,
    /// Whether this animation originated from [`PlayAnimation`], [`ZoomToFit`], or
    /// [`AnimateToFit`].
    pub source: AnimationSource,
}

/// `AnimationCancelled` — emitted when a [`PlayAnimation`], [`ZoomToFit`], or [`AnimateToFit`] is
/// cancelled before completion. The camera stays at its current position — no snap to final.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimationCancelled {
    /// The camera whose animation was cancelled.
    #[event_target]
    pub camera: Entity,
    /// Whether this animation originated from [`PlayAnimation`], [`ZoomToFit`], or
    /// [`AnimateToFit`].
    pub source: AnimationSource,
    /// The [`CameraMove`] that was in progress when cancelled.
    pub camera_move: CameraMove,
}

/// `AnimationRejected` — emitted when an incoming animation request is rejected because
/// [`AnimationConflictPolicy::FirstWins`](crate::AnimationConflictPolicy::FirstWins) is
/// active and an animation is already in-flight.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimationRejected {
    /// The camera that rejected the animation.
    #[event_target]
    pub camera: Entity,
    /// The [`AnimationSource`] of the rejected request.
    pub source: AnimationSource,
}

/// `CameraMoveBegin` — emitted when an individual [`CameraMove`] begins.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraMoveBegin {
    /// The camera being animated.
    #[event_target]
    pub camera: Entity,
    /// The [`CameraMove`] step that is starting.
    pub camera_move: CameraMove,
}

/// `CameraMoveEnd` — emitted when an individual [`CameraMove`] completes.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraMoveEnd {
    /// The camera that finished this move step.
    #[event_target]
    pub camera: Entity,
    /// The [`CameraMove`] step that completed.
    pub camera_move: CameraMove,
}

/// `AnimateToFit` — animates the camera to a caller-specified orientation while
/// framing a target entity in view.
///
/// You specify the exact yaw and pitch the camera should end up at, and the
/// system computes the radius needed to frame the target from that angle.
///
/// # See also
///
/// - [`LookAtAndZoomToFit`] — like `AnimateToFit` but the yaw/pitch are automatically back-solved
///   from the camera's current position, so you don't specify them.
/// - [`ZoomToFit`] — keeps the current viewing angle, only adjusts focus and radius.
/// - [`LookAt`] — rotates to face the target without framing.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimateToFit {
    /// The camera entity.
    #[event_target]
    pub camera: Entity,
    /// The entity to frame.
    pub target: Entity,
    /// Final yaw in radians.
    pub yaw: f32,
    /// Final pitch in radians.
    pub pitch: f32,
    /// Fraction of screen to leave as margin.
    pub margin: f32,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing: EaseFunction,
}

impl AnimateToFit {
    /// Creates a new `AnimateToFit` with default parameters.
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            yaw: 0.0,
            pitch: 0.0,
            margin: 0.1,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the target yaw.
    pub const fn yaw(mut self, yaw: f32) -> Self {
        self.yaw = yaw;
        self
    }

    /// Sets the target pitch.
    pub const fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    /// Sets the margin.
    pub const fn margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Sets the animation duration.
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the easing function.
    pub const fn easing(mut self, easing: EaseFunction) -> Self {
        self.easing = easing;
        self
    }
}

/// `LookAt` — rotates the camera in place to face a target entity.
///
/// The camera stays at its current world position and turns to look at the target.
/// The orbit pivot re-anchors to the target entity's [`GlobalTransform`] translation,
/// and yaw/pitch/radius are back-solved so the camera does not move — only its
/// orientation changes.
///
/// # See also
///
/// - [`LookAtAndZoomToFit`] — same rotation, but also adjusts radius to frame the target in view.
/// - [`ZoomToFit`] — keeps the viewing angle, moves the camera to frame the target.
/// - [`AnimateToFit`] — frames the target from a caller-specified viewing angle.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct LookAt {
    /// The camera entity.
    #[event_target]
    pub camera: Entity,
    /// The entity to look at.
    pub target: Entity,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing: EaseFunction,
}

impl LookAt {
    /// Creates a new `LookAt` with instant duration and cubic-out easing.
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the animation duration.
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the easing function.
    pub const fn easing(mut self, easing: EaseFunction) -> Self {
        self.easing = easing;
        self
    }
}

/// `LookAtAndZoomToFit` — rotates the camera to face a target entity and adjusts
/// the radius to frame it in view, all in one fluid motion.
///
/// Combines [`LookAt`] (turn in place) with [`ZoomToFit`] (frame the target).
/// The yaw and pitch are back-solved from the camera's current world position
/// relative to the target's bounds center — you don't specify them.
///
/// # See also
///
/// - [`LookAt`] — same rotation without the zoom-to-fit radius adjustment.
/// - [`ZoomToFit`] — keeps the viewing angle, moves the camera to frame the target.
/// - [`AnimateToFit`] — frames the target from a caller-specified viewing angle.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct LookAtAndZoomToFit {
    /// The camera entity.
    #[event_target]
    pub camera: Entity,
    /// The entity to frame.
    pub target: Entity,
    /// Fraction of screen to leave as margin.
    pub margin: f32,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing: EaseFunction,
}

impl LookAtAndZoomToFit {
    /// Creates a new `LookAtAndZoomToFit` with default parameters.
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            margin: 0.1,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the margin.
    pub const fn margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Sets the animation duration.
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the easing function.
    pub const fn easing(mut self, easing: EaseFunction) -> Self {
        self.easing = easing;
        self
    }
}

/// `SetFitTarget` — sets the visualization target without triggering a zoom. Allows you
/// to inspect bounds before triggering [`ZoomToFit`].
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct SetFitTarget {
    /// The camera entity.
    #[event_target]
    pub camera: Entity,
    /// The entity whose bounds to visualize.
    pub target: Entity,
}

impl SetFitTarget {
    /// Creates a new `SetFitTarget` event.
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self { camera, target }
    }
}

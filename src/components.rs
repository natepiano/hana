//! Components used by the camera extension system.

use bevy::prelude::*;

use super::events::AnimationSource;
use super::events::ZoomContext;

/// Controls what happens when user input occurs during an in-flight animation.
///
/// Specifically, this governs **user input to the camera** (orbit, pan, zoom) while an
/// animation is playing.
///
/// This is a required component on [`CameraMoveList`](crate::CameraMoveList) — if not
/// explicitly inserted, it defaults to [`Ignore`](CameraInputInterruptBehavior::Ignore).
///
/// This component is orthogonal to [`AnimationConflictPolicy`] — `CameraInputInterruptBehavior`
/// handles physical camera input during an animation, while `AnimationConflictPolicy`
/// handles programmatic animation requests that arrive while one is already playing.
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
    /// Stop the camera at its current position. Fires `AnimationCancelled` or `ZoomCancelled`.
    Cancel,
    /// Jump to the final queued position. Fires `AnimationEnd` or `ZoomEnd`.
    Complete,
}

/// Controls what happens when a new animation request conflicts with an active one.
///
/// Insert this component on a camera entity to configure conflict resolution. If not
/// present, defaults to [`LastWins`](AnimationConflictPolicy::LastWins).
///
/// This component is orthogonal to [`CameraInputInterruptBehavior`] — `AnimationConflictPolicy`
/// handles programmatic animation requests (e.g. [`ZoomToFit`](crate::ZoomToFit),
/// [`PlayAnimation`](crate::PlayAnimation)) that conflict with an active animation, while
/// `CameraInputInterruptBehavior` handles physical user input interrupting an animation.
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

/// Marks the entity that the camera is currently fitted to.
///
/// Persists after fit completes to enable persistent debug overlay.
#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct CurrentFitTarget(
    /// The entity being fitted.
    pub Entity,
);

/// Tracks a zoom-to-fit operation routed through the animation system.
///
/// When `AnimationEnd` fires on an entity with this marker, `ZoomEnd` is triggered and the
/// marker is removed. Wraps the [`ZoomContext`] that originated the zoom.
#[derive(Component, Clone)]
pub(super) struct ZoomAnimationMarker(pub ZoomContext);

/// Tracks which trigger source started the current animation.
///
/// Records whether the animation was triggered by [`PlayAnimation`](crate::PlayAnimation),
/// [`ZoomToFit`](crate::ZoomToFit), or [`AnimateToFit`](crate::AnimateToFit). Inserted alongside
/// [`CameraMoveList`](crate::CameraMoveList) and removed when the animation ends or is cancelled.
#[derive(Component)]
pub(super) struct AnimationSourceMarker(pub AnimationSource);

/// Component that stores camera runtime state values during animations.
///
/// When camera animations are active (via `CameraMoveList`), the smoothness values are
/// temporarily set to 0.0 for instant movement. Depending on
/// [`CameraInputInterruptBehavior`], camera input may also be temporarily disabled.
/// Original values are stored here and restored when the animation completes.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(super) struct OrbitCamStash {
    pub zoom:    f32,
    pub pan:     f32,
    pub orbit:   f32,
    pub enabled: bool,
}

/// Enables fit target debug overlay on a camera entity.
///
/// Insert this component to enable overlay, remove it to disable.
/// The presence or absence of the component is the toggle — no boolean field needed.
#[cfg(feature = "fit_overlay")]
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct FitOverlay;

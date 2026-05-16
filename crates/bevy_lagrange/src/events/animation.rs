#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]

use std::collections::VecDeque;

use bevy::prelude::*;

use super::zoom::ZoomContext;
use crate::animation::CameraMove;

/// Identifies which event triggered an animation lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum AnimationSource {
    /// Animation was triggered by `PlayAnimation`.
    PlayAnimation,
    /// Animation was triggered by `ZoomToFit`.
    ZoomToFit,
    /// Animation was triggered by `AnimateToFit`.
    AnimateToFit,
    /// Animation was triggered by `LookAt`.
    LookAt,
    /// Animation was triggered by `LookAtAndZoomToFit`.
    LookAtAndZoomToFit,
}

/// Plays a queued sequence of `CameraMove` steps.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct PlayAnimation {
    /// The camera entity to animate.
    #[event_target]
    pub camera:       Entity,
    /// The queue of camera movements.
    pub camera_moves: VecDeque<CameraMove>,
    /// The source of this animation.
    pub source:       AnimationSource,
    /// Optional zoom context when this animation originates from `ZoomToFit`.
    pub zoom_context: Option<ZoomContext>,
}

impl PlayAnimation {
    /// Creates a new `PlayAnimation` event.
    #[must_use]
    pub fn new(camera: Entity, camera_moves: impl IntoIterator<Item = CameraMove>) -> Self {
        Self {
            camera,
            camera_moves: camera_moves.into_iter().collect(),
            source: AnimationSource::PlayAnimation,
            zoom_context: None,
        }
    }

    /// Sets the animation source.
    #[must_use]
    pub const fn source(mut self, source: AnimationSource) -> Self {
        self.source = source;
        self
    }

    /// Sets the zoom context and marks the source as `ZoomToFit`.
    #[must_use]
    pub const fn zoom_context(mut self, zoom_context: ZoomContext) -> Self {
        self.zoom_context = Some(zoom_context);
        self.source = AnimationSource::ZoomToFit;
        self
    }
}

/// Emitted when a `CameraMoveList` begins processing.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimationBegin {
    /// The camera being animated.
    #[event_target]
    pub camera: Entity,
    /// Whether this animation originated from `PlayAnimation`, `ZoomToFit`, `AnimateToFit`,
    /// `LookAt`, or `LookAtAndZoomToFit`.
    pub source: AnimationSource,
}

/// Emitted when an animation stops running, either by completing naturally or
/// by being cancelled. Inspect [`AnimationEnd::reason`] to distinguish.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimationEnd {
    /// The camera that stopped animating.
    #[event_target]
    pub camera: Entity,
    /// Whether this animation originated from `PlayAnimation`, `ZoomToFit`, `AnimateToFit`,
    /// `LookAt`, or `LookAtAndZoomToFit`.
    pub source: AnimationSource,
    /// Why the animation stopped: completed naturally, or cancelled.
    pub reason: AnimationReason,
}

/// Why an [`AnimationEnd`] fired.
#[derive(Clone, Debug, Reflect)]
pub enum AnimationReason {
    /// The animation queue ran to completion.
    Completed,
    /// The animation was interrupted before it could complete (either by
    /// external camera input or by a new `PlayAnimation` superseding it).
    Cancelled {
        /// The [`CameraMove`] that was in progress when the animation was
        /// cancelled.
        interrupted_move: CameraMove,
    },
}

/// Emitted when an incoming animation request is rejected.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimationRejected {
    /// The camera that rejected the animation.
    #[event_target]
    pub camera: Entity,
    /// The source of the rejected request.
    pub source: AnimationSource,
}

/// Emitted when an individual `CameraMove` begins.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraMoveBegin {
    /// The camera being animated.
    #[event_target]
    pub camera:      Entity,
    /// The `CameraMove` step that is starting.
    pub camera_move: CameraMove,
}

/// Emitted when an individual `CameraMove` completes.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraMoveEnd {
    /// The camera that finished this move step.
    #[event_target]
    pub camera:      Entity,
    /// The `CameraMove` step that completed.
    pub camera_move: CameraMove,
}

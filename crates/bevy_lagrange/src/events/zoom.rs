use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use crate::constants::DEFAULT_FIT_MARGIN;

/// Context for a zoom-to-fit operation routed through `PlayAnimation`.
#[derive(Clone, Reflect)]
pub struct ZoomContext {
    /// The entity being framed.
    pub target:   Entity,
    /// The margin from the triggering `ZoomToFit`.
    pub margin:   f32,
    /// The duration from the triggering `ZoomToFit`.
    pub duration: Duration,
    /// The easing curve from the triggering `ZoomToFit`.
    pub easing:   EaseFunction,
}

/// Frames a target entity in the camera view while preserving the current viewing angle.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomToFit {
    /// The camera entity to zoom.
    #[event_target]
    pub camera:   Entity,
    /// The entity to frame.
    pub target:   Entity,
    /// Fraction of screen to leave as margin.
    pub margin:   f32,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing:   EaseFunction,
}

impl ZoomToFit {
    /// Creates a new `ZoomToFit` event with default margin, instant duration, and cubic-out easing.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            margin: DEFAULT_FIT_MARGIN,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the margin.
    #[must_use]
    pub const fn margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Sets the animation duration.
    #[must_use]
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the easing function.
    #[must_use]
    pub const fn easing(mut self, easing: EaseFunction) -> Self {
        self.easing = easing;
        self
    }
}

/// Emitted when a `ZoomToFit` operation begins.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomBegin {
    /// The camera that is zooming.
    #[event_target]
    pub camera:   Entity,
    /// The entity being framed.
    pub target:   Entity,
    /// The margin from the triggering `ZoomToFit`.
    pub margin:   f32,
    /// The duration from the triggering `ZoomToFit`.
    pub duration: Duration,
    /// The easing curve from the triggering `ZoomToFit`.
    pub easing:   EaseFunction,
}

/// Emitted when a `ZoomToFit` operation completes.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomEnd {
    /// The camera that finished zooming.
    #[event_target]
    pub camera:   Entity,
    /// The entity that was framed.
    pub target:   Entity,
    /// The margin from the triggering `ZoomToFit`.
    pub margin:   f32,
    /// The duration from the triggering `ZoomToFit`.
    pub duration: Duration,
    /// The easing curve from the triggering `ZoomToFit`.
    pub easing:   EaseFunction,
}

/// Emitted when a `ZoomToFit` animation is cancelled before completion.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomCancelled {
    /// The camera whose zoom was cancelled.
    #[event_target]
    pub camera:   Entity,
    /// The entity that was being framed.
    pub target:   Entity,
    /// The margin from the triggering `ZoomToFit`.
    pub margin:   f32,
    /// The duration from the triggering `ZoomToFit`.
    pub duration: Duration,
    /// The easing curve from the triggering `ZoomToFit`.
    pub easing:   EaseFunction,
}

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

/// Rotates the camera in place to face a target entity.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct LookAt {
    /// The camera entity.
    #[event_target]
    pub camera:   Entity,
    /// The entity to look at.
    pub target:   Entity,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing:   EaseFunction,
}

impl LookAt {
    /// Creates a new `LookAt` with instant duration and cubic-out easing.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
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

/// Rotates the camera to face a target entity and frames it in view.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct LookAtAndZoomToFit {
    /// The camera entity.
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

impl LookAtAndZoomToFit {
    /// Creates a new `LookAtAndZoomToFit` with default parameters.
    #[must_use]
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

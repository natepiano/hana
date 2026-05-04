use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use crate::constants::DEFAULT_FIT_MARGIN;

/// Animates the camera to a caller-specified orientation while framing a target entity.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimateToFit {
    /// The camera entity.
    #[event_target]
    pub camera:   Entity,
    /// The entity to frame.
    pub target:   Entity,
    /// Final yaw in radians.
    pub yaw:      f32,
    /// Final pitch in radians.
    pub pitch:    f32,
    /// Fraction of screen to leave as margin.
    pub margin:   f32,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing:   EaseFunction,
}

impl AnimateToFit {
    /// Creates a new `AnimateToFit` with default parameters.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            yaw: 0.0,
            pitch: 0.0,
            margin: DEFAULT_FIT_MARGIN,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the target yaw.
    #[must_use]
    pub const fn yaw(mut self, yaw: f32) -> Self {
        self.yaw = yaw;
        self
    }

    /// Sets the target pitch.
    #[must_use]
    pub const fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
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

/// Sets the debug overlay target without triggering a zoom.
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
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self { Self { camera, target } }
}

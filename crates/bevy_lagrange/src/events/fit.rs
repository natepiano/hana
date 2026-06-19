use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use super::constants::DEFAULT_ANIMATE_TO_FIT_PITCH;
use super::constants::DEFAULT_ANIMATE_TO_FIT_YAW;
use super::fit_anchor::FitAnchor;
use crate::constants::DEFAULT_FIT_MARGIN;

/// Animates the camera to a caller-specified orientation while framing a target entity.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimateToFit {
    /// The camera entity.
    #[event_target]
    pub(crate) camera:    Entity,
    /// The entity to frame.
    pub(crate) target:    Entity,
    /// Final yaw in radians.
    pub(crate) yaw:       f32,
    /// Final pitch in radians.
    pub(crate) pitch:     f32,
    /// Fraction of screen to leave as margin.
    pub(crate) margin:    f32,
    /// Screen-space anchor used after the target has been fitted.
    pub(crate) anchor:    FitAnchor,
    /// Pixel offset from the selected anchor, using positive x right and positive y down.
    pub(crate) offset_px: Vec2,
    /// Animation duration (`ZERO` for instant).
    pub(crate) duration:  Duration,
    /// Easing curve for the animation.
    pub(crate) easing:    EaseFunction,
}

impl AnimateToFit {
    /// Creates a new `AnimateToFit` with default parameters.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            yaw: DEFAULT_ANIMATE_TO_FIT_YAW,
            pitch: DEFAULT_ANIMATE_TO_FIT_PITCH,
            margin: DEFAULT_FIT_MARGIN,
            anchor: FitAnchor::Center,
            offset_px: Vec2::ZERO,
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

    /// Sets which fitted bounds point should land on the matching viewport point.
    #[must_use]
    pub const fn anchor(mut self, anchor: FitAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Sets a pixel offset from the selected anchor.
    ///
    /// Positive x moves the fitted bounds right. Positive y moves them down,
    /// matching Bevy's screen-space coordinate convention.
    #[must_use]
    pub const fn offset_px(mut self, offset_px: Vec2) -> Self {
        self.offset_px = offset_px;
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

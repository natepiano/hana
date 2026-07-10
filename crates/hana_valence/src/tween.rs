//! `bevy_tween` adapters for valence components.

use bevy_math::Quat;
use bevy_tween::interpolate::CurrentValue;
use bevy_tween::interpolate::Interpolator;
use bevy_tween::interpolate::PreviousValue;

use crate::AnchorPose;
use crate::Hinge;

/// Interpolates [`Hinge::angle`] for `bevy_tween` component tweens.
///
/// Register `bevy_tween::tween::component_tween_system::<HingeAngleLens>()`
/// so `bevy_tween::TweenSystemSet::ApplyTween` runs in
/// [`AnchorSystems::AnimatePose`](crate::AnchorSystems::AnimatePose). When
/// [`hinge_to_pose`](crate::hinge_to_pose) is also registered, run
/// `bevy_tween::TweenSystemSet::ApplyTween` before `hinge_to_pose` so the
/// updated angle is converted to [`AnchorPose`] in the same frame.
///
/// Do not pair a [`Hinge`] component with [`AnchorPoseLens`] on the same
/// entity. `hinge_to_pose` overwrites the whole [`AnchorPose`] every frame and
/// writes [`AnchorPose::translation`] from optional
/// [`HingePivot`](crate::HingePivot) compensation, or
/// [`bevy_math::Vec3::ZERO`] when no pivot is present. Direct pose tweens on
/// that entity are discarded. Debug builds warn when `hinge_to_pose` sees an
/// earlier same-frame `AnchorPose` change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HingeAngleLens {
    /// Starting fold angle in radians.
    pub start: f32,
    /// Ending fold angle in radians.
    pub end:   f32,
}

impl Interpolator for HingeAngleLens {
    type Item = Hinge;

    fn interpolate(&self, hinge: &mut Self::Item, value: CurrentValue, _: PreviousValue) {
        hinge.angle = lerp(self.start, self.end, value);
    }
}

/// Interpolates [`AnchorPose`] for `bevy_tween` component tweens.
///
/// [`AnchorPoseLens`] spherical-linearly interpolates [`AnchorPose::rotation`]
/// and linearly interpolates [`AnchorPose::translation`]. Register
/// `bevy_tween::tween::component_tween_system::<AnchorPoseLens>()` so
/// `bevy_tween::TweenSystemSet::ApplyTween` runs in
/// [`AnchorSystems::AnimatePose`](crate::AnchorSystems::AnimatePose), before
/// [`resolve_anchors`](crate::resolve_anchors) runs in
/// [`AnchorSystems::Resolve`](crate::AnchorSystems::Resolve).
///
/// Do not pair a [`Hinge`] component with [`AnchorPoseLens`] on the same
/// entity. [`hinge_to_pose`](crate::hinge_to_pose) overwrites the whole
/// [`AnchorPose`] every frame and writes [`AnchorPose::translation`] from
/// optional [`HingePivot`](crate::HingePivot) compensation, or
/// [`bevy_math::Vec3::ZERO`] when no pivot is present. Direct pose tweens on
/// that entity are discarded. Debug builds warn when `hinge_to_pose` sees an
/// earlier same-frame `AnchorPose` change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorPoseLens {
    /// Starting local pose.
    pub start: AnchorPose,
    /// Ending local pose.
    pub end:   AnchorPose,
}

impl Interpolator for AnchorPoseLens {
    type Item = AnchorPose;

    fn interpolate(&self, pose: &mut Self::Item, value: CurrentValue, _: PreviousValue) {
        pose.rotation = slerp(self.start.rotation, self.end.rotation, value);
        pose.translation = self.start.translation.lerp(self.end.translation, value);
    }
}

fn lerp(start: f32, end: f32, value: f32) -> f32 { start.mul_add(1.0 - value, end * value) }

fn slerp(start: Quat, end: Quat, value: f32) -> Quat { start.slerp(end, value) }

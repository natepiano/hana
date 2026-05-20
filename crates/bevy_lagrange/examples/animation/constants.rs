use std::f32::consts::TAU;
use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::Color;
use bevy::prelude::Vec3;

// animation
pub(super) const ANIMATE_TO_FIT_DURATION: Duration = Duration::from_millis(1200);
pub(super) const ANIMATE_TO_FIT_MARGIN: f32 = 0.15;
pub(super) const ANIMATE_TO_FIT_PITCH: f32 = TAU / 12.0;
pub(super) const ANIMATE_TO_FIT_YAW: f32 = TAU / 8.0;
pub(super) const MANUAL_MODE_SMOOTHNESS_ACTIVE: f32 = 0.0;
pub(super) const MANUAL_MODE_SMOOTHNESS_INACTIVE: f32 = 0.8;
pub(super) const MANUAL_ORBIT_PITCH_AMPLITUDE: f32 = TAU * 0.1;
pub(super) const MANUAL_ORBIT_RADIUS_BASE: f32 = 4.0;
pub(super) const MANUAL_ORBIT_RADIUS_DELTA: f32 = 2.0;
pub(super) const MANUAL_ORBIT_RADIUS_FREQUENCY: f32 = 2.0;
pub(super) const MANUAL_ORBIT_YAW_RADIANS_PER_SECOND: f32 = TAU / 24.0;

// camera
pub(super) const START_POS: Vec3 = Vec3::new(0.0, 3.0, 8.0);

// instructions
pub(super) const INSTRUCTIONS_FONT_SIZE: f32 = 18.0;
pub(super) const INSTRUCTIONS_TEXT: &str = "M - Toggle manual orbit animation\n\
             Space - PlayAnimation (5-step sequence)\n\
             A - AnimateToFit (yaw=45 pitch=30)\n\
             R - Reset camera";

// play animation
#[derive(Clone, Copy)]
pub(super) struct OrbitAnimationStep {
    pub(super) duration: Duration,
    pub(super) easing:   EaseFunction,
    pub(super) pitch:    f32,
    pub(super) radius:   f32,
    pub(super) yaw:      f32,
}

pub(super) const PLAY_ANIMATION_FOCUS: Vec3 = Vec3::new(0.0, 0.75, 0.0);
pub(super) const PLAY_ANIMATION_STEPS: [OrbitAnimationStep; 5] = [
    OrbitAnimationStep {
        duration: Duration::from_millis(800),
        easing:   EaseFunction::CubicInOut,
        pitch:    0.2,
        radius:   4.0,
        yaw:      1.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::CubicIn,
        pitch:    1.3,
        radius:   20.0,
        yaw:      2.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::SineInOut,
        pitch:    0.6,
        radius:   14.0,
        yaw:      4.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_secs(1),
        easing:   EaseFunction::CubicIn,
        pitch:    0.1,
        radius:   2.0,
        yaw:      5.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::BounceOut,
        pitch:    0.3,
        radius:   8.0,
        yaw:      0.0,
    },
];

// scene
pub(super) const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
pub(super) const GROUND_SIZE: f32 = 10.0;
pub(super) const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);
pub(super) const TARGET_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
pub(super) const TARGET_SIZE: Vec3 = Vec3::splat(1.5);
pub(super) const TARGET_TRANSLATION: Vec3 = Vec3::new(0.0, 0.75, 0.0);

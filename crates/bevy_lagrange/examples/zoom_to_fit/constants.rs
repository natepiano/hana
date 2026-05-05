use std::time::Duration;

use bevy::prelude::Color;
use bevy::prelude::Vec3;

// animation
pub(super) const FIT_DURATION: Duration = Duration::from_millis(800);
pub(super) const FIT_MARGIN: f32 = 0.15;
pub(super) const LOOK_AT_DURATION: Duration = Duration::from_millis(600);

// camera
pub(super) const START_POS: Vec3 = Vec3::new(0.0, 1.5, 3.0);

// scene
pub(super) const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
pub(super) const GROUND_SIZE: f32 = 10.0;
pub(super) const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);
pub(super) const REFERENCE_CUBE_COLOR: Color = Color::srgb(0.5, 0.5, 0.5);
pub(super) const REFERENCE_CUBE_SIZE: Vec3 = Vec3::splat(0.5);
pub(super) const REFERENCE_CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.25, 0.0);
pub(super) const TARGET_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
pub(super) const TARGET_SIZE: Vec3 = Vec3::splat(1.0);
pub(super) const TARGET_TRANSLATION: Vec3 = Vec3::new(3.5, 0.5, 0.0);

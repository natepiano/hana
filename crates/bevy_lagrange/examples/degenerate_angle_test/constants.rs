use std::time::Duration;

use bevy::prelude::Color;
use bevy::prelude::Vec3;

// camera
pub(super) const CAMERA_FOCUS: Vec3 = Vec3::new(0.0, 1.0, 0.0);
pub(super) const CAMERA_PITCH: f32 = 0.0;
pub(super) const CAMERA_RADIUS: f32 = 5.0;
pub(super) const CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 1.0, 5.0);
pub(super) const CAMERA_YAW: f32 = 0.0;

// scene
pub(super) const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
pub(super) const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 1.0, 0.0);
pub(super) const GROUND_ALPHA: f32 = 0.8;
pub(super) const GROUND_COLOR: Color = Color::srgba(0.3, 0.5, 0.3, GROUND_ALPHA);
pub(super) const GROUND_SIZE: f32 = 12.0;
pub(super) const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);

// zoom
pub(super) const ZOOM_DURATION: Duration = Duration::from_secs(1);
pub(super) const ZOOM_MARGIN_MESH: f32 = 0.15;
pub(super) const ZOOM_MARGIN_SCENE: f32 = 0.08;

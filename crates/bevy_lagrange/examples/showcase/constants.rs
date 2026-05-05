use std::f32::consts::PI;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::Color;
use bevy::prelude::Vec3;

// camera home
pub(super) const CAMERA_START_PITCH: f32 = 0.4;
pub(super) const CAMERA_START_YAW: f32 = -0.2;

// durations
pub(super) const ANIMATE_FIT_DURATION_MILLIS: u64 = 1200;
pub(super) const LOOK_AT_DURATION_MILLIS: u64 = 800;
pub(super) const ORBIT_MOVE_DURATION_MILLIS: u64 = 800;
pub(super) const ZOOM_DURATION_MILLIS: u64 = 1000;

// easings
pub(super) const ALL_EASINGS: &[EaseFunction] = &[
    EaseFunction::Linear,
    EaseFunction::QuadraticIn,
    EaseFunction::QuadraticOut,
    EaseFunction::QuadraticInOut,
    EaseFunction::CubicIn,
    EaseFunction::CubicOut,
    EaseFunction::CubicInOut,
    EaseFunction::QuarticIn,
    EaseFunction::QuarticOut,
    EaseFunction::QuarticInOut,
    EaseFunction::QuinticIn,
    EaseFunction::QuinticOut,
    EaseFunction::QuinticInOut,
    EaseFunction::SmoothStepIn,
    EaseFunction::SmoothStepOut,
    EaseFunction::SmoothStep,
    EaseFunction::SmootherStepIn,
    EaseFunction::SmootherStepOut,
    EaseFunction::SmootherStep,
    EaseFunction::SineIn,
    EaseFunction::SineOut,
    EaseFunction::SineInOut,
    EaseFunction::CircularIn,
    EaseFunction::CircularOut,
    EaseFunction::CircularInOut,
    EaseFunction::ExponentialIn,
    EaseFunction::ExponentialOut,
    EaseFunction::ExponentialInOut,
    EaseFunction::ElasticIn,
    EaseFunction::ElasticOut,
    EaseFunction::ElasticInOut,
    EaseFunction::BackIn,
    EaseFunction::BackOut,
    EaseFunction::BackInOut,
    EaseFunction::BounceIn,
    EaseFunction::BounceOut,
    EaseFunction::BounceInOut,
];

// event log
pub(super) const EVENT_LOG_COLOR: Color = Color::srgba(0.0, 1.0, 0.0, 0.9);
pub(super) const EVENT_LOG_ERROR_COLOR: Color = Color::srgba(1.0, 0.3, 0.3, 0.9);
pub(super) const EVENT_LOG_FONT_SIZE: f32 = 14.0;
pub(super) const EVENT_LOG_HINT_BOTTOM_PIXELS: f32 = 28.0;
pub(super) const EVENT_LOG_PANEL_BOTTOM_PIXELS: f32 = 72.0;
pub(super) const EVENT_LOG_SCROLL_SPEED: f32 = 120.0;
pub(super) const EVENT_LOG_SEPARATOR: &str = "- - - - - - - - - - - -";
pub(super) const EVENT_LOG_WIDTH: f32 = 300.0;

// hints
pub(super) const HINT_TEXT_COLOR: Color = Color::srgba(0.7, 0.7, 0.7, 0.7);

// mesh settings
pub(super) const GIZMO_DEPTH_BIAS: f32 = -0.005;
pub(super) const GIZMO_LINE_WIDTH: f32 = 2.0;
pub(super) const GIZMO_SCALE: f32 = 1.001;
pub(super) const MESH_CENTER_Y: f32 = 1.0;
pub(super) const SELECTION_GIZMO_LAYER: usize = 1;
pub(super) const ZOOM_MARGIN_MESH: f32 = 0.15;
pub(super) const ZOOM_MARGIN_SCENE: f32 = 0.08;

// paused overlay
pub(super) const FULL_WIDTH_PERCENT: f32 = 100.0;
pub(super) const OVERLAY_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.4);
pub(super) const PAUSED_OVERLAY_FONT_SIZE: f32 = 48.0;
pub(super) const PAUSED_OVERLAY_TOP_PERCENT: f32 = 46.0;

// projection
pub(super) const ORTHOGRAPHIC_FAR_PLANE: f32 = 40.0;
pub(super) const ORTHOGRAPHIC_VIEWPORT_HEIGHT: f32 = 1.0;

// scene
pub(super) const GROUND_ALPHA: f32 = 0.85;
pub(super) const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
pub(super) const GROUND_SIZE: f32 = 12.0;
pub(super) const MESH_CUBOID_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
pub(super) const MESH_CUBOID_SIZE: Vec3 = Vec3::splat(1.0);
pub(super) const MESH_CUBOID_TRANSLATION: Vec3 = Vec3::new(-2.5, MESH_CENTER_Y, 0.0);
pub(super) const MESH_SPHERE_COLOR: Color = Color::srgb(0.9, 0.3, 0.2);
pub(super) const MESH_SPHERE_LATITUDES: u32 = 64;
pub(super) const MESH_SPHERE_LONGITUDES: u32 = 128;
pub(super) const MESH_SPHERE_RADIUS: f32 = 0.5;
pub(super) const MESH_SPHERE_TRANSLATION: Vec3 = Vec3::new(0.0, MESH_CENTER_Y, 0.0);
pub(super) const MESH_TORUS_COLOR: Color = Color::srgb(0.9, 0.5, 0.1);
pub(super) const MESH_TORUS_MAJOR_RADIUS: f32 = 0.75;
pub(super) const MESH_TORUS_MAJOR_RESOLUTION: usize = 64;
pub(super) const MESH_TORUS_MINOR_RADIUS: f32 = 0.25;
pub(super) const MESH_TORUS_MINOR_RESOLUTION: usize = 64;
pub(super) const MESH_TORUS_TRANSLATION: Vec3 = Vec3::new(2.5, MESH_CENTER_Y, 0.0);
pub(super) const SCENE_LIGHT_ILLUMINANCE: f32 = 3000.0;
pub(super) const SCENE_LIGHT_ROTATION_X: f32 = -PI / 4.0;
pub(super) const SCENE_LIGHT_ROTATION_Y: f32 = PI / 4.0;
pub(super) const SCENE_LIGHT_ROTATION_Z: f32 = 0.0;
pub(super) const UNDERSIDE_PLANE_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
pub(super) const UNDERSIDE_PLANE_ROTATION_X: f32 = PI;

// sensitivity
pub(super) const DRAG_SENSITIVITY: f32 = 0.02;

// ui layout
pub(super) const CONFLICT_POLICY_HINT_BOTTOM_PIXELS: f32 = 32.0;
pub(super) const UI_FONT_SIZE: f32 = 13.0;
pub(super) const UI_SCREEN_PADDING_PIXELS: f32 = 12.0;

// window label
pub(super) const WINDOW_LABEL_DURATION_SECS: f32 = 2.0;

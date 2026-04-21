use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::Color;

// camera home
pub(super) const CAMERA_START_PITCH: f32 = 0.4;
pub(super) const CAMERA_START_YAW: f32 = -0.2;

// durations
pub(super) const ANIMATE_FIT_DURATION_MS: u64 = 1200;
pub(super) const LOOK_AT_DURATION_MS: u64 = 800;
pub(super) const ORBIT_MOVE_DURATION_MS: u64 = 800;
pub(super) const ZOOM_DURATION_MS: u64 = 1000;

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
pub(super) const EVENT_LOG_HINT_BOTTOM_PX: f32 = 28.0;
pub(super) const EVENT_LOG_PANEL_BOTTOM_PX: f32 = 72.0;
pub(super) const EVENT_LOG_SCROLL_SPEED: f32 = 120.0;
pub(super) const EVENT_LOG_SEPARATOR: &str = "- - - - - - - - - - - -";
pub(super) const EVENT_LOG_WIDTH: f32 = 300.0;

// mesh settings
pub(super) const GIZMO_DEPTH_BIAS: f32 = -0.005;
pub(super) const GIZMO_LINE_WIDTH: f32 = 2.0;
pub(super) const GIZMO_SCALE: f32 = 1.001;
pub(super) const MESH_CENTER_Y: f32 = 1.0;
pub(super) const SELECTION_GIZMO_LAYER: usize = 1;
pub(super) const ZOOM_MARGIN_MESH: f32 = 0.15;
pub(super) const ZOOM_MARGIN_SCENE: f32 = 0.08;

// paused overlay
pub(super) const PAUSED_OVERLAY_FONT_SIZE: f32 = 48.0;
pub(super) const PAUSED_OVERLAY_TOP_PERCENT: f32 = 46.0;

// sensitivity
pub(super) const DRAG_SENSITIVITY: f32 = 0.02;

// ui layout
pub(super) const CONFLICT_POLICY_HINT_BOTTOM_PX: f32 = 32.0;
pub(super) const UI_FONT_SIZE: f32 = 13.0;
pub(super) const UI_SCREEN_PADDING_PX: f32 = 12.0;

// window label
pub(super) const WINDOW_LABEL_DURATION_SECS: f32 = 2.0;

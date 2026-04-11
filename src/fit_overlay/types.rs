use bevy::prelude::*;

use crate::support::ScreenSpaceBounds;

/// Gizmo config group for fit target visualization (screen-aligned overlay).
/// Toggle by inserting/removing the `FitVisualization` component on the camera entity.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub(super) struct FitTargetGizmo;

/// Current screen-space margin percentages for the fit target.
/// Updated every frame by the visualization system.
/// Removed when fit target visualization is disabled.
#[derive(Component, Reflect, Debug, Default, Clone)]
#[reflect(Component)]
pub struct FitTargetViewportMarginPcts {
    /// Left margin as a percentage of screen width.
    pub left:   f32,
    /// Right margin as a percentage of screen width.
    pub right:  f32,
    /// Top margin as a percentage of screen height.
    pub top:    f32,
    /// Bottom margin as a percentage of screen height.
    pub bottom: f32,
}

impl FitTargetViewportMarginPcts {
    /// Constructs margin percentages from screen-space bounds, computing
    /// screen dimensions once rather than per-edge.
    pub fn from_bounds(bounds: &ScreenSpaceBounds) -> Self {
        let screen_width = 2.0 * bounds.half_extent_x;
        let screen_height = 2.0 * bounds.half_extent_y;
        Self {
            left:   (bounds.left_margin / screen_width) * 100.0,
            right:  (bounds.right_margin / screen_width) * 100.0,
            top:    (bounds.top_margin / screen_height) * 100.0,
            bottom: (bounds.bottom_margin / screen_height) * 100.0,
        }
    }
}

/// Configuration for fit target visualization colors and appearance.
#[derive(Resource, Reflect, Debug, Clone)]
#[reflect(Resource)]
pub struct FitTargetOverlayConfig {
    /// Color for the screen-aligned bounding rectangle.
    pub rectangle_color:  Color,
    /// Color for the silhouette convex hull.
    pub silhouette_color: Color,
    /// Color for balanced margins (left ≈ right, top ≈ bottom).
    pub balanced_color:   Color,
    /// Color for unbalanced margins.
    pub unbalanced_color: Color,
    /// Line width for gizmo rendering.
    pub line_width:       f32,
}

impl Default for FitTargetOverlayConfig {
    fn default() -> Self {
        Self {
            rectangle_color:  Color::srgb(1.0, 1.0, 0.0), // Yellow
            silhouette_color: Color::srgb(1.0, 0.5, 0.0), // Orange
            balanced_color:   Color::srgb(0.0, 1.0, 0.0), // Green
            unbalanced_color: Color::srgb(1.0, 0.0, 0.0), // Red
            line_width:       2.0,
        }
    }
}

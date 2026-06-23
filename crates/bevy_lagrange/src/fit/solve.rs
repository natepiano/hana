//! Fit algorithm for framing objects in the camera view.
//!
//! Provides screen-space projection, margin calculation, and a binary search convergence
//! loop that finds the optimal camera radius and focus to frame a set of mesh vertices
//! with a specified margin.

use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;

use bevy::prelude::*;
use bevy_kana::Position;

use super::projection;
use super::projection::ProjectionMode;
use super::projection::ScreenSpaceBounds;
use crate::constants::CENTERING_MAX_ITERATIONS;
use crate::constants::CENTERING_TOLERANCE;
use crate::constants::DEGENERATE_EXTENT_THRESHOLD;
use crate::constants::HORIZONTAL_DIMENSION_LABEL;
use crate::constants::INITIAL_RADIUS_MULTIPLIER;
use crate::constants::MAX_ITERATIONS;
use crate::constants::MAX_MARGIN;
use crate::constants::MAX_RADIUS_MULTIPLIER;
use crate::constants::MIN_MARGIN;
use crate::constants::MIN_RADIUS_MULTIPLIER;
use crate::constants::TOLERANCE;
use crate::constants::VERTICAL_DIMENSION_LABEL;
use crate::events::FitAnchor;

/// Returns the zoom margin multiplier (1.0 / (1.0 - margin)).
/// For example, a margin of 0.08 returns 1.087 (8% margin).
pub(crate) const fn zoom_margin_multiplier(margin: f32) -> f32 { 1.0 / (1.0 - margin) }

// ============================================================================
// Types
// ============================================================================

/// Screen edge identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub(crate) enum Edge {
    /// Left screen edge.
    Left,
    /// Right screen edge.
    Right,
    /// Top screen edge.
    Top,
    /// Bottom screen edge.
    Bottom,
}

/// Tracks whether the binary search ever saw projectable bounds.
#[derive(Debug, Clone, Copy)]
enum BoundsSearch {
    NeverProjectable,
    Projectable,
}

/// Successful fit output: camera orbit radius and centered focus point.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FitSolution {
    /// The optimal orbital radius.
    pub radius: f32,
    /// The centered focus point.
    pub focus:  Position,
}

/// Explicit fit calculation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FitError {
    /// Camera viewport size/aspect ratio is unavailable.
    NoViewport,
    /// All candidate fits projected points behind the camera.
    PointsBehindCamera,
    /// Projection variant is not supported (e.g. `Projection::Custom`).
    UnsupportedProjection,
}

impl Display for FitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoViewport => write!(f, "camera viewport size is unavailable"),
            Self::PointsBehindCamera => {
                write!(f, "all candidate fits project points behind camera")
            },
            Self::UnsupportedProjection => write!(f, "projection variant is not supported"),
        }
    }
}

// ============================================================================
// Target margin calculation
// ============================================================================

/// Computes the target margins for the constraining dimension based on aspect ratios.
/// Returns `(target_margin_x, target_margin_y)`.
const fn calculate_target_margins(bounds: &ScreenSpaceBounds, zoom_multiplier: f32) -> (f32, f32) {
    let horizontal_extent = bounds.max_normalized_x - bounds.min_normalized_x;
    let vertical_extent = bounds.max_normalized_y - bounds.min_normalized_y;

    // Guard against degenerate screen-space extents (edge-on flat objects).
    // When one dimension is near-zero, fit based on the non-degenerate dimension only.
    // Setting the target margin to the full half-extent ensures the degenerate
    // dimension never constrains the binary search.
    if vertical_extent < DEGENERATE_EXTENT_THRESHOLD {
        let target_x = bounds.half_extent_x / zoom_multiplier;
        (bounds.half_extent_x - target_x, bounds.half_extent_y)
    } else if horizontal_extent < DEGENERATE_EXTENT_THRESHOLD {
        let target_y = bounds.half_extent_y / zoom_multiplier;
        (bounds.half_extent_x, bounds.half_extent_y - target_y)
    } else {
        let boundary_aspect = horizontal_extent / vertical_extent;
        let screen_aspect = bounds.half_extent_x / bounds.half_extent_y;

        // If boundary is wider (relative to height) than screen, width constrains.
        let width_constrains = boundary_aspect > screen_aspect;

        let (target_edge_x, target_edge_y) = if width_constrains {
            let target_x = bounds.half_extent_x / zoom_multiplier;
            let target_y = target_x / boundary_aspect;
            (target_x, target_y)
        } else {
            let target_y = bounds.half_extent_y / zoom_multiplier;
            let target_x = target_y * boundary_aspect;
            (target_x, target_y)
        };

        (
            bounds.half_extent_x - target_edge_x,
            bounds.half_extent_y - target_edge_y,
        )
    }
}

// ============================================================================
// Convergence algorithm
// ============================================================================

/// Pre-computed parameters for the fit binary search.
struct FitParameters {
    rotation:                    Quat,
    aspect_ratio:                f32,
    orthographic_fixed_distance: Option<f32>,
    projection_mode:             ProjectionMode,
    zoom_multiplier:             f32,
    viewport_size:               Option<Vec2>,
}

/// Calculates the optimal radius and centered focus to fit pre-extracted vertices in the camera
/// view. The focus is adjusted so the projected mesh silhouette is centered in the viewport.
///
/// For each candidate radius, computes the focus that centers the projected silhouette in the
/// viewport (since the geometric center doesn't project to screen center from off-axis angles),
/// then evaluates margins at that centered position. Returns the fit solution where
/// the constraining margin equals the target and the silhouette is centered.
///
/// Note: A lateral camera shift doesn't change point depths, so the centering is geometrically
/// exact for the constraining margin check.
pub(crate) fn calculate_fit(
    points: &[Vec3],
    geometric_center: Vec3,
    yaw: f32,
    pitch: f32,
    margin: f32,
    anchor: FitAnchor,
    offset_px: Vec2,
    projection: &Projection,
    camera: &Camera,
) -> Result<FitSolution, FitError> {
    let clamped_margin = if margin.is_nan() {
        MIN_MARGIN
    } else {
        margin.clamp(MIN_MARGIN, MAX_MARGIN)
    };
    #[allow(
        clippy::float_cmp,
        reason = "clamp returns input unchanged when in bounds — bitwise identical"
    )]
    if clamped_margin != margin {
        warn!(
            "calculate_fit: clamped margin from {margin} to {clamped_margin} (expected [{MIN_MARGIN}, {MAX_MARGIN}])"
        );
    }

    let mode_and_distance = match projection {
        Projection::Perspective(_) => Some((ProjectionMode::Perspective, None)),
        Projection::Orthographic(o) => {
            Some((ProjectionMode::Orthographic, Some((o.near + o.far) * 0.5)))
        },
        Projection::Custom(_) => None,
    };
    let Some((projection_mode, orthographic_fixed_distance)) = mode_and_distance else {
        return Err(FitError::UnsupportedProjection);
    };

    let viewport_size = camera.logical_viewport_size();
    if has_pixel_offset(offset_px) && !viewport_can_map_pixels(viewport_size) {
        return Err(FitError::NoViewport);
    }

    let aspect_ratio = projection::projection_aspect_ratio(projection, viewport_size)
        .ok_or(FitError::NoViewport)?;

    let parameters = FitParameters {
        rotation: Quat::from_euler(EulerRot::YXZ, yaw, -pitch, 0.0),
        aspect_ratio,
        orthographic_fixed_distance,
        projection_mode,
        zoom_multiplier: zoom_margin_multiplier(clamped_margin),
        viewport_size,
    };

    let object_radius = points
        .iter()
        .map(|c| (*c - geometric_center).length())
        .fold(0.0_f32, f32::max);

    binary_search_for_fit(
        points,
        geometric_center,
        object_radius,
        projection,
        &parameters,
        anchor,
        offset_px,
    )
}

fn has_pixel_offset(offset_px: Vec2) -> bool { offset_px.length_squared() > f32::EPSILON }

fn viewport_can_map_pixels(viewport_size: Option<Vec2>) -> bool {
    viewport_size.is_some_and(|size| size.x > f32::EPSILON && size.y > f32::EPSILON)
}

/// Determines which screen dimension constrains the fit and returns the current margin,
/// target margin, and dimension label.
const fn find_constraining_margin(
    bounds: &ScreenSpaceBounds,
    target_margin_x: f32,
    target_margin_y: f32,
) -> (f32, f32, &'static str) {
    let horizontal_min_margin = bounds.left_margin.min(bounds.right_margin);
    let vertical_min_margin = bounds.top_margin.min(bounds.bottom_margin);
    let vertical_extent = bounds.max_normalized_y - bounds.min_normalized_y;
    let horizontal_extent = bounds.max_normalized_x - bounds.min_normalized_x;

    if vertical_extent < DEGENERATE_EXTENT_THRESHOLD {
        (
            horizontal_min_margin,
            target_margin_x,
            HORIZONTAL_DIMENSION_LABEL,
        )
    } else if horizontal_extent < DEGENERATE_EXTENT_THRESHOLD {
        (
            vertical_min_margin,
            target_margin_y,
            VERTICAL_DIMENSION_LABEL,
        )
    } else if horizontal_min_margin < vertical_min_margin {
        (
            horizontal_min_margin,
            target_margin_x,
            HORIZONTAL_DIMENSION_LABEL,
        )
    } else {
        (
            vertical_min_margin,
            target_margin_y,
            VERTICAL_DIMENSION_LABEL,
        )
    }
}

/// Binary search for the camera radius that produces the target margin.
///
/// For perspective: radius = camera distance (changes apparent size).
/// For ortho: `OrbitCam` maps radius → `OrthographicProjection::scale`,
/// so searching over radius effectively searches over scale.
fn binary_search_for_fit(
    points: &[Vec3],
    geometric_center: Vec3,
    object_radius: f32,
    projection: &Projection,
    parameters: &FitParameters,
    anchor: FitAnchor,
    offset_px: Vec2,
) -> Result<FitSolution, FitError> {
    let mut min_radius = object_radius * MIN_RADIUS_MULTIPLIER;
    let mut max_radius = object_radius * MAX_RADIUS_MULTIPLIER;
    let mut best_radius = object_radius * INITIAL_RADIUS_MULTIPLIER;
    let mut best_focus = Position(geometric_center);
    let mut best_error = f32::INFINITY;
    let mut bounds_search = BoundsSearch::NeverProjectable;
    let mut converged = false;

    debug!("Binary search starting: range [{min_radius:.1}, {max_radius:.1}]");

    for iteration in 0..MAX_ITERATIONS {
        let test_radius = (min_radius + max_radius) * 0.5;
        let test_projection = build_test_projection(projection, test_radius);

        let centered_focus = refine_focus_centering(
            points,
            geometric_center,
            test_radius,
            &test_projection,
            parameters,
        );

        let camera_distance = parameters
            .orthographic_fixed_distance
            .unwrap_or(test_radius);
        let camera_position =
            centered_focus + parameters.rotation * Vec3::new(0.0, 0.0, camera_distance);
        let camera_global = GlobalTransform::from(
            Transform::from_translation(camera_position).with_rotation(parameters.rotation),
        );

        let Some((bounds, _)) = ScreenSpaceBounds::from_points(
            points,
            &camera_global,
            &test_projection,
            parameters.aspect_ratio,
        ) else {
            debug!(
                "Iteration {iteration}: Points behind camera at radius {test_radius:.1}, searching higher"
            );
            min_radius = test_radius;
            continue;
        };
        bounds_search = BoundsSearch::Projectable;

        let (target_margin_x, target_margin_y) =
            calculate_target_margins(&bounds, parameters.zoom_multiplier);
        let (current_margin, target_margin, dimension) =
            find_constraining_margin(&bounds, target_margin_x, target_margin_y);

        debug!(
            "Iteration {iteration}: radius={test_radius:.1} | {dimension} margin={current_margin:.3} \
             target={target_margin:.3} | L={:.3} R={:.3} T={:.3} B={:.3} | range=[{min_radius:.1}, {max_radius:.1}]",
            bounds.left_margin, bounds.right_margin, bounds.top_margin, bounds.bottom_margin
        );

        let margin_error = (current_margin - target_margin).abs();
        if margin_error < best_error {
            best_error = margin_error;
            best_radius = test_radius;
            best_focus = Position(centered_focus);
        }

        if current_margin > target_margin {
            max_radius = test_radius;
        } else {
            min_radius = test_radius;
        }

        if (max_radius - min_radius) < TOLERANCE {
            debug!(
                "Iteration {iteration}: Converged to best radius {best_radius:.3} error={best_error:.5}"
            );
            converged = true;
            break;
        }
    }

    if matches!(bounds_search, BoundsSearch::NeverProjectable) {
        return Err(FitError::PointsBehindCamera);
    }

    if !converged {
        warn!(
            "Binary search did not converge in {MAX_ITERATIONS} iterations. Using best radius {best_radius:.1}"
        );
    }

    let focus = refine_focus_anchoring(
        points,
        *best_focus,
        best_radius,
        projection,
        parameters,
        anchor,
        offset_px,
    );

    Ok(FitSolution {
        radius: best_radius,
        focus:  Position(focus),
    })
}

/// Builds a test projection with the given radius/scale for binary search iterations.
///
/// For perspective, returns the original projection unchanged.
/// For orthographic, creates a modified projection with `area` recomputed for the test scale,
/// since `OrbitCam` maps `radius` → `OrthographicProjection::scale`.
fn build_test_projection(projection: &Projection, test_radius: f32) -> Projection {
    match projection {
        Projection::Orthographic(ortho) => {
            // Compute what the area would be at this scale.
            // The current area is `base_size * current_scale`, so base_size = area / scale.
            // At test scale: new_area = base_size * test_radius.
            let current_scale = ortho.scale;
            let scale_ratio = if current_scale.abs() > f32::EPSILON {
                test_radius / current_scale
            } else {
                1.0
            };
            let new_area = Rect::new(
                ortho.area.min.x * scale_ratio,
                ortho.area.min.y * scale_ratio,
                ortho.area.max.x * scale_ratio,
                ortho.area.max.y * scale_ratio,
            );
            Projection::Orthographic(OrthographicProjection {
                scale: test_radius,
                area: new_area,
                ..*ortho
            })
        },
        Projection::Perspective(_) | Projection::Custom(_) => projection.clone(),
    }
}

/// Shifts the camera focus so the projected bounding box is centered on screen.
///
/// For perspective, each correction step uses the harmonic mean of the depths of the two
/// extreme points per dimension. This is the exact inverse of perspective projection.
///
/// For orthographic, centering is depth-independent (`centering_depth` = 1.0), so the shift
/// is a direct 1:1 world-unit correction.
fn refine_focus_centering(
    points: &[Vec3],
    initial_focus: Vec3,
    radius: f32,
    projection: &Projection,
    parameters: &FitParameters,
) -> Vec3 {
    refine_focus_anchoring(
        points,
        initial_focus,
        radius,
        projection,
        parameters,
        FitAnchor::Center,
        Vec2::ZERO,
    )
}

/// Shifts the camera focus so the selected projected bounds anchor lands on
/// the matching viewport anchor.
///
/// The final `offset_px` uses screen coordinates: positive x moves right,
/// positive y moves down.
fn refine_focus_anchoring(
    points: &[Vec3],
    initial_focus: Vec3,
    radius: f32,
    projection: &Projection,
    parameters: &FitParameters,
    anchor: FitAnchor,
    offset_px: Vec2,
) -> Vec3 {
    let rotation = parameters.rotation;
    let aspect_ratio = parameters.aspect_ratio;
    let orthographic_fixed_distance = parameters.orthographic_fixed_distance;
    let projection_mode = parameters.projection_mode;
    let camera_right = rotation * Vec3::X;
    let camera_up = rotation * Vec3::Y;

    let camera_distance = orthographic_fixed_distance.unwrap_or(radius);

    let mut focus = initial_focus;
    for _ in 0..CENTERING_MAX_ITERATIONS {
        let camera_position = focus + rotation * Vec3::new(0.0, 0.0, camera_distance);
        let camera_global = GlobalTransform::from(
            Transform::from_translation(camera_position).with_rotation(rotation),
        );
        let Some((bounds, depths)) =
            ScreenSpaceBounds::from_points(points, &camera_global, projection, aspect_ratio)
        else {
            break;
        };
        let current_anchor = bounds_anchor_point(&bounds, anchor);
        let target_anchor =
            viewport_anchor_point(&bounds, parameters.viewport_size, anchor, offset_px);
        let delta = current_anchor - target_anchor;
        if delta.x.abs() < CENTERING_TOLERANCE && delta.y.abs() < CENTERING_TOLERANCE {
            break;
        }

        // Lateral correction depths: perspective uses harmonic mean for
        // perspective-correct placement. Ortho uses 1.0 since projection is
        // depth-independent.
        let (correction_depth_x, correction_depth_y) = match projection_mode {
            ProjectionMode::Orthographic => (1.0, 1.0),
            ProjectionMode::Perspective => (
                2.0 * depths.min_x * depths.max_x / (depths.min_x + depths.max_x),
                2.0 * depths.min_y * depths.max_y / (depths.min_y + depths.max_y),
            ),
        };

        focus +=
            camera_right * delta.x * correction_depth_x + camera_up * delta.y * correction_depth_y;
    }
    focus
}

fn bounds_anchor_point(bounds: &ScreenSpaceBounds, anchor: FitAnchor) -> Vec2 {
    let (anchor_x, anchor_y) = anchor.offset_fraction();
    let x = (bounds.max_normalized_x - bounds.min_normalized_x)
        .mul_add(anchor_x, bounds.min_normalized_x);
    let y = (bounds.min_normalized_y - bounds.max_normalized_y)
        .mul_add(anchor_y, bounds.max_normalized_y);
    Vec2::new(x, y)
}

fn viewport_anchor_point(
    bounds: &ScreenSpaceBounds,
    viewport_size: Option<Vec2>,
    anchor: FitAnchor,
    offset_px: Vec2,
) -> Vec2 {
    let (anchor_x, anchor_y) = anchor.offset_fraction();
    let viewport_width = bounds.half_extent_x * 2.0;
    let viewport_height = bounds.half_extent_y * 2.0;
    let x = -bounds.half_extent_x + viewport_width * anchor_x;
    let y = bounds.half_extent_y - viewport_height * anchor_y;
    let offset = normalized_pixel_offset(bounds, viewport_size, offset_px);

    Vec2::new(x + offset.x, y + offset.y)
}

fn normalized_pixel_offset(
    bounds: &ScreenSpaceBounds,
    viewport_size: Option<Vec2>,
    offset_px: Vec2,
) -> Vec2 {
    let Some(viewport_size) = viewport_size else {
        return Vec2::ZERO;
    };
    if !viewport_can_map_pixels(Some(viewport_size)) {
        return Vec2::ZERO;
    }

    Vec2::new(
        offset_px.x / viewport_size.x * bounds.half_extent_x * 2.0,
        -offset_px.y / viewport_size.y * bounds.half_extent_y * 2.0,
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "expect is idiomatic for test assertions"
)]
mod tests {
    use bevy::prelude::Camera;
    use bevy::prelude::EulerRot;
    use bevy::prelude::GlobalTransform;
    use bevy::prelude::OrthographicProjection;
    use bevy::prelude::PerspectiveProjection;
    use bevy::prelude::Projection;
    use bevy::prelude::Quat;
    use bevy::prelude::Rect;
    use bevy::prelude::Transform;
    use bevy::prelude::Vec2;
    use bevy::prelude::Vec3;

    use super::CENTERING_TOLERANCE;
    use super::FitError;
    use super::FitSolution;
    use super::ScreenSpaceBounds;
    use super::calculate_fit;
    use super::projection;
    use crate::constants::DEFAULT_FIT_MARGIN;
    use crate::events::FitAnchor;

    fn default_perspective() -> Projection {
        Projection::Perspective(PerspectiveProjection::default())
    }

    fn projected_bounds(
        points: &[Vec3],
        fit: FitSolution,
        yaw: f32,
        pitch: f32,
        projection: &Projection,
        camera: &Camera,
    ) -> ScreenSpaceBounds {
        let aspect_ratio =
            projection::projection_aspect_ratio(projection, camera.logical_viewport_size())
                .expect("test projection should have an aspect ratio");
        let rotation = Quat::from_euler(EulerRot::YXZ, yaw, -pitch, 0.0);
        let camera_position = *fit.focus + rotation * Vec3::new(0.0, 0.0, fit.radius);
        let camera_global = GlobalTransform::from(
            Transform::from_translation(camera_position).with_rotation(rotation),
        );
        let (bounds, _) =
            ScreenSpaceBounds::from_points(points, &camera_global, projection, aspect_ratio)
                .expect("test fit should project its points");
        bounds
    }

    #[test]
    fn calculate_fit_returns_no_viewport_for_invalid_ortho_area() {
        let projection = Projection::Orthographic(OrthographicProjection {
            area: Rect::new(-1.0, 0.0, 1.0, 0.0),
            ..OrthographicProjection::default_3d()
        });
        let camera = Camera::default();

        let result = calculate_fit(
            &[Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)],
            Vec3::ZERO,
            0.0,
            0.0,
            DEFAULT_FIT_MARGIN,
            FitAnchor::Center,
            Vec2::ZERO,
            &projection,
            &camera,
        );

        assert!(matches!(result, Err(FitError::NoViewport)));
    }

    #[test]
    fn calculate_fit_returns_points_behind_camera_for_degenerate_point_cloud() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [Vec3::ZERO, Vec3::ZERO, Vec3::ZERO];

        let result = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.0,
            DEFAULT_FIT_MARGIN,
            FitAnchor::Center,
            Vec2::ZERO,
            &projection,
            &camera,
        );

        assert!(matches!(result, Err(FitError::PointsBehindCamera)));
    }

    #[test]
    fn calculate_fit_clamps_out_of_range_margin_and_still_returns_solution() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];

        let result = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.0,
            5.0,
            FitAnchor::Center,
            Vec2::ZERO,
            &projection,
            &camera,
        );

        let fit = result.expect("fit should succeed with clamped margin");
        assert!(fit.radius.is_finite());
        assert!(fit.focus.is_finite());
    }

    #[test]
    fn calculate_fit_handles_nan_margin_by_clamping_to_zero() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];

        let result = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.0,
            f32::NAN,
            FitAnchor::Center,
            Vec2::ZERO,
            &projection,
            &camera,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn calculate_fit_can_anchor_bounds_top_left() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];

        let fit = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.0,
            DEFAULT_FIT_MARGIN,
            FitAnchor::TopLeft,
            Vec2::ZERO,
            &projection,
            &camera,
        )
        .expect("top-left anchored fit should succeed");
        let bounds = projected_bounds(&points, fit, 0.0, 0.0, &projection, &camera);

        assert!(
            (bounds.min_normalized_x + bounds.half_extent_x).abs() < CENTERING_TOLERANCE,
            "left edge should land on viewport left edge: {bounds:?}",
        );
        assert!(
            (bounds.max_normalized_y - bounds.half_extent_y).abs() < CENTERING_TOLERANCE,
            "top edge should land on viewport top edge: {bounds:?}",
        );
    }

    #[test]
    fn calculate_fit_requires_viewport_for_pixel_offset() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];

        let result = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.0,
            DEFAULT_FIT_MARGIN,
            FitAnchor::TopLeft,
            Vec2::new(16.0, 16.0),
            &projection,
            &camera,
        );

        assert!(matches!(result, Err(FitError::NoViewport)));
    }

    /// Flat quad in XZ at Y=0, camera at pitch=0 (edge-on). The vertical screen
    /// extent is zero, which previously caused `calculate_target_margins` to
    /// divide by zero and the binary search to converge on an absurd radius.
    #[test]
    fn edge_on_flat_plane_produces_reasonable_radius() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [
            Vec3::new(-0.5, 0.0, -0.5),
            Vec3::new(0.5, 0.0, -0.5),
            Vec3::new(-0.5, 0.0, 0.5),
            Vec3::new(0.5, 0.0, 0.5),
        ];
        let object_radius = points
            .iter()
            .copied()
            .map(Vec3::length)
            .fold(0.0_f32, f32::max);

        let fit = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.0,
            DEFAULT_FIT_MARGIN,
            FitAnchor::Center,
            Vec2::ZERO,
            &projection,
            &camera,
        )
        .expect("edge-on flat plane should produce a valid fit");

        assert!(
            fit.radius < object_radius * 10.0,
            "radius {:.1} should be less than 10x object_radius {:.3}",
            fit.radius,
            object_radius,
        );
    }

    /// Same flat quad but with a tiny pitch (near-degenerate). Should converge
    /// to a similar radius as a non-degenerate case.
    #[test]
    fn near_edge_on_flat_plane_still_converges() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [
            Vec3::new(-0.5, 0.0, -0.5),
            Vec3::new(0.5, 0.0, -0.5),
            Vec3::new(-0.5, 0.0, 0.5),
            Vec3::new(0.5, 0.0, 0.5),
        ];
        let object_radius = points
            .iter()
            .copied()
            .map(Vec3::length)
            .fold(0.0_f32, f32::max);

        let fit = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.001,
            DEFAULT_FIT_MARGIN,
            FitAnchor::Center,
            Vec2::ZERO,
            &projection,
            &camera,
        )
        .expect("near-edge-on flat plane should produce a valid fit");

        assert!(
            fit.radius < object_radius * 10.0,
            "radius {:.1} should be less than 10x object_radius {:.3}",
            fit.radius,
            object_radius,
        );
    }

    /// Vertical line segment (zero horizontal extent) viewed head-on.
    /// Mirror of the edge-on plane case — ensures the degenerate guard is symmetric.
    #[test]
    fn vertical_line_zero_horizontal_extent_produces_reasonable_radius() {
        let projection = default_perspective();
        let camera = Camera::default();
        let points = [Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)];
        let object_radius = 1.0;

        let fit = calculate_fit(
            &points,
            Vec3::ZERO,
            0.0,
            0.0,
            DEFAULT_FIT_MARGIN,
            FitAnchor::Center,
            Vec2::ZERO,
            &projection,
            &camera,
        )
        .expect("vertical line should produce a valid fit");

        assert!(
            fit.radius < object_radius * 10.0,
            "radius {:.1} should be less than 10x object_radius {:.1}",
            fit.radius,
            object_radius,
        );
    }
}

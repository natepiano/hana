//! Fit algorithm for framing objects in the camera view.
//!
//! Provides screen-space projection, margin calculation, and a binary search convergence
//! loop that finds the optimal camera radius and focus to frame a set of mesh vertices
//! with a specified margin.

use core::fmt;

use bevy::prelude::*;

use super::support::projection_aspect_ratio;
use super::support::ScreenSpaceBounds;

// ============================================================================
// Constants
// ============================================================================

/// Maximum binary search iterations.
pub const MAX_ITERATIONS: usize = 200;
/// Convergence tolerance (0.1% of search range).
pub const TOLERANCE: f32 = 0.001;
/// Maximum centering iterations per candidate radius.
pub const CENTERING_MAX_ITERATIONS: usize = 10;
/// Normalized screen-space center offset tolerance.
pub const CENTERING_TOLERANCE: f32 = 0.0001;
/// Minimum allowed margin value.
pub const MIN_MARGIN: f32 = 0.0;
/// Maximum allowed margin value.
pub const MAX_MARGIN: f32 = 0.9999;
/// Minimum search radius as a fraction of the object radius (0.1x).
pub const MIN_RADIUS_MULTIPLIER: f32 = 0.1;
/// Maximum search radius as a multiple of the object radius (100x).
pub const MAX_RADIUS_MULTIPLIER: f32 = 100.0;
/// Initial best-guess radius as a multiple of the object radius (2x).
pub const INITIAL_RADIUS_MULTIPLIER: f32 = 2.0;
/// Minimum screen-space extent before treating a dimension as degenerate (edge-on).
/// Below this threshold the dimension is ignored for fit purposes.
pub const DEGENERATE_EXTENT_THRESHOLD: f32 = 1e-6;

/// Returns the zoom margin multiplier (1.0 / (1.0 - margin)).
/// For example, a margin of 0.08 returns 1.087 (8% margin).
pub const fn zoom_margin_multiplier(margin: f32) -> f32 {
    1.0 / (1.0 - margin)
}

// ============================================================================
// Types
// ============================================================================

/// Screen edge identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum Edge {
    /// Left screen edge.
    Left,
    /// Right screen edge.
    Right,
    /// Top screen edge.
    Top,
    /// Bottom screen edge.
    Bottom,
}

/// Successful fit output: camera orbit radius and centered focus point.
#[derive(Debug, Clone, Copy)]
pub struct FitSolution {
    /// The optimal orbital radius.
    pub radius: f32,
    /// The centered focus point.
    pub focus: Vec3,
}

/// Explicit fit calculation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitError {
    /// Camera viewport size/aspect ratio is unavailable.
    NoViewport,
    /// All candidate fits projected points behind the camera.
    PointsBehindCamera,
}

impl fmt::Display for FitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoViewport => write!(f, "camera viewport size is unavailable"),
            Self::PointsBehindCamera => {
                write!(f, "all candidate fits project points behind camera")
            }
        }
    }
}

// ============================================================================
// Target margin calculation
// ============================================================================

/// Computes the target margins for the constraining dimension based on aspect ratios.
/// Returns `(target_margin_x, target_margin_y)`.
fn calculate_target_margins(bounds: &ScreenSpaceBounds, zoom_multiplier: f32) -> (f32, f32) {
    let horizontal_extent = bounds.max_norm_x - bounds.min_norm_x;
    let vertical_extent = bounds.max_norm_y - bounds.min_norm_y;

    // Guard against degenerate screen-space extents (edge-on flat objects).
    // When one dimension is near-zero, fit based on the non-degenerate dimension only.
    // Setting the target margin to the full half-extent ensures the degenerate
    // dimension never constrains the binary search.
    if vertical_extent < DEGENERATE_EXTENT_THRESHOLD {
        let target_x = bounds.half_extent_x / zoom_multiplier;
        return (bounds.half_extent_x - target_x, bounds.half_extent_y);
    }
    if horizontal_extent < DEGENERATE_EXTENT_THRESHOLD {
        let target_y = bounds.half_extent_y / zoom_multiplier;
        return (bounds.half_extent_x, bounds.half_extent_y - target_y);
    }

    let boundary_aspect = horizontal_extent / vertical_extent;
    let screen_aspect = bounds.half_extent_x / bounds.half_extent_y;

    // If boundary is wider (relative to height) than screen, width constrains
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

// ============================================================================
// Convergence algorithm
// ============================================================================

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
pub fn calculate_fit(
    points: &[Vec3],
    geometric_center: Vec3,
    yaw: f32,
    pitch: f32,
    margin: f32,
    projection: &Projection,
    camera: &Camera,
) -> Result<FitSolution, FitError> {
    let clamped_margin = if margin.is_nan() {
        MIN_MARGIN
    } else {
        margin.clamp(MIN_MARGIN, MAX_MARGIN)
    };
    if clamped_margin != margin {
        warn!(
            "calculate_fit: clamped margin from {margin} to {clamped_margin} (expected [{MIN_MARGIN}, {MAX_MARGIN}])"
        );
    }

    let aspect_ratio = projection_aspect_ratio(projection, camera.logical_viewport_size())
        .ok_or(FitError::NoViewport)?;

    // For ortho, the camera is always at a fixed distance from focus.
    // PanOrbitCamera sets this to `(near + far) / 2.0`.
    let ortho_fixed_distance = match projection {
        Projection::Orthographic(o) => Some((o.near + o.far) * 0.5),
        _ => None,
    };

    let is_ortho = ortho_fixed_distance.is_some();
    let zoom_multiplier = zoom_margin_multiplier(clamped_margin);

    let rot = Quat::from_euler(EulerRot::YXZ, yaw, -pitch, 0.0);

    // Compute the object's bounding sphere radius from points for sensible search bounds.
    // The search range is based purely on object size to ensure deterministic results
    // regardless of the camera's current radius.
    let object_radius = points
        .iter()
        .map(|c| (*c - geometric_center).length())
        .fold(0.0_f32, f32::max);

    // Binary search for the correct radius.
    // For perspective: radius = camera distance (changes apparent size).
    // For ortho: PanOrbitCamera maps radius → `OrthographicProjection::scale`,
    //   so searching over radius effectively searches over scale.
    let mut min_radius = object_radius * MIN_RADIUS_MULTIPLIER;
    let mut max_radius = object_radius * MAX_RADIUS_MULTIPLIER;
    let mut best_radius = object_radius * INITIAL_RADIUS_MULTIPLIER;
    let mut best_focus = geometric_center;
    let mut best_error = f32::INFINITY;
    let mut found_projectable_bounds = false;

    debug!("Binary search starting: range [{min_radius:.1}, {max_radius:.1}]");

    for iteration in 0..MAX_ITERATIONS {
        let test_radius = (min_radius + max_radius) * 0.5;

        // Build the projection to use for this iteration.
        // For ortho, we need to compute what `area` would be at this test scale.
        let test_projection = build_test_projection(projection, test_radius);

        // Step 1: find the centered focus using accurate depth-based centering
        let centered_focus = refine_focus_centering(
            points,
            geometric_center,
            test_radius,
            rot,
            &test_projection,
            aspect_ratio,
            ortho_fixed_distance,
            is_ortho,
        );

        // Step 2: evaluate margins at the centered focus position.
        // For ortho, the camera distance is fixed regardless of test_radius.
        let cam_distance = ortho_fixed_distance.unwrap_or(test_radius);
        let cam_pos = centered_focus + rot * Vec3::new(0.0, 0.0, cam_distance);
        let cam_global =
            GlobalTransform::from(Transform::from_translation(cam_pos).with_rotation(rot));

        let Some((bounds, _)) =
            ScreenSpaceBounds::from_points(points, &cam_global, &test_projection, aspect_ratio)
        else {
            warn!(
                "Iteration {iteration}: Points behind camera at radius {test_radius:.1}, searching higher"
            );
            min_radius = test_radius;
            continue;
        };
        found_projectable_bounds = true;

        let (target_margin_x, target_margin_y) = calculate_target_margins(&bounds, zoom_multiplier);

        // Find constraining dimension (minimum margin).
        // When a dimension has degenerate (near-zero) screen extent, force the
        // other dimension to constrain — the degenerate dimension has no
        // meaningful projection to fit against.
        let h_min = bounds.left_margin.min(bounds.right_margin);
        let v_min = bounds.top_margin.min(bounds.bottom_margin);
        let vertical_extent = bounds.max_norm_y - bounds.min_norm_y;
        let horizontal_extent = bounds.max_norm_x - bounds.min_norm_x;

        let (current_margin, target_margin, dimension) =
            if vertical_extent < DEGENERATE_EXTENT_THRESHOLD {
                (h_min, target_margin_x, "H")
            } else if horizontal_extent < DEGENERATE_EXTENT_THRESHOLD {
                (v_min, target_margin_y, "V")
            } else if h_min < v_min {
                (h_min, target_margin_x, "H")
            } else {
                (v_min, target_margin_y, "V")
            };

        debug!(
            "Iteration {iteration}: radius={test_radius:.1} | {dimension} margin={current_margin:.3} \
             target={target_margin:.3} | L={:.3} R={:.3} T={:.3} B={:.3} | range=[{min_radius:.1}, {max_radius:.1}]",
            bounds.left_margin, bounds.right_margin, bounds.top_margin, bounds.bottom_margin
        );

        // Track the closest match to target margin
        let margin_error = (current_margin - target_margin).abs();
        if margin_error < best_error {
            best_error = margin_error;
            best_radius = test_radius;
            best_focus = centered_focus;
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
            return Ok(FitSolution {
                radius: best_radius,
                focus: best_focus,
            });
        }
    }

    if !found_projectable_bounds {
        return Err(FitError::PointsBehindCamera);
    }

    warn!(
        "Binary search did not converge in {MAX_ITERATIONS} iterations. Using best radius {best_radius:.1}"
    );

    Ok(FitSolution {
        radius: best_radius,
        focus: best_focus,
    })
}

/// Builds a test projection with the given radius/scale for binary search iterations.
///
/// For perspective, returns the original projection unchanged.
/// For orthographic, creates a modified projection with `area` recomputed for the test scale,
/// since `PanOrbitCamera` maps `radius` → `OrthographicProjection::scale`.
fn build_test_projection(projection: &Projection, test_radius: f32) -> Projection {
    match projection {
        Projection::Perspective(_) => projection.clone(),
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
        }
        _ => projection.clone(),
    }
}

/// Shifts the camera focus so the projected bounding box is centered on screen.
///
/// For perspective, each correction step uses the harmonic mean of the depths of the two
/// extreme points per dimension. This is the exact inverse of perspective projection.
///
/// For orthographic, centering is depth-independent (`centering_depth` = 1.0), so the shift
/// is a direct 1:1 world-unit correction.
#[allow(clippy::too_many_arguments)]
fn refine_focus_centering(
    points: &[Vec3],
    initial_focus: Vec3,
    radius: f32,
    rot: Quat,
    projection: &Projection,
    aspect_ratio: f32,
    ortho_fixed_distance: Option<f32>,
    is_ortho: bool,
) -> Vec3 {
    let cam_right = rot * Vec3::X;
    let cam_up = rot * Vec3::Y;

    let cam_distance = ortho_fixed_distance.unwrap_or(radius);

    let mut focus = initial_focus;
    for _ in 0..CENTERING_MAX_ITERATIONS {
        let cam_pos = focus + rot * Vec3::new(0.0, 0.0, cam_distance);
        let cam_global =
            GlobalTransform::from(Transform::from_translation(cam_pos).with_rotation(rot));
        let Some((bounds, depths)) =
            ScreenSpaceBounds::from_points(points, &cam_global, projection, aspect_ratio)
        else {
            break;
        };
        let (cx, cy) = bounds.center();
        if cx.abs() < CENTERING_TOLERANCE && cy.abs() < CENTERING_TOLERANCE {
            break;
        }

        // Centering depths: perspective uses harmonic mean for perspective-correct
        // centering. Ortho uses 1.0 since projection is depth-independent.
        let (centering_depth_x, centering_depth_y) = if is_ortho {
            (1.0, 1.0)
        } else {
            (
                2.0 * depths.min_x_depth * depths.max_x_depth
                    / (depths.min_x_depth + depths.max_x_depth),
                2.0 * depths.min_y_depth * depths.max_y_depth
                    / (depths.min_y_depth + depths.max_y_depth),
            )
        };

        focus += cam_right * cx * centering_depth_x + cam_up * cy * centering_depth_y;
    }
    focus
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_perspective() -> Projection {
        Projection::Perspective(PerspectiveProjection::default())
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
            0.1,
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

        let result = calculate_fit(&points, Vec3::ZERO, 0.0, 0.0, 0.1, &projection, &camera);

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

        let result = calculate_fit(&points, Vec3::ZERO, 0.0, 0.0, 5.0, &projection, &camera);

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
            &projection,
            &camera,
        );

        assert!(result.is_ok());
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
        let object_radius = points.iter().map(|p| p.length()).fold(0.0_f32, f32::max);

        let fit = calculate_fit(&points, Vec3::ZERO, 0.0, 0.0, 0.1, &projection, &camera)
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
        let object_radius = points.iter().map(|p| p.length()).fold(0.0_f32, f32::max);

        let fit = calculate_fit(&points, Vec3::ZERO, 0.0, 0.001, 0.1, &projection, &camera)
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

        let fit = calculate_fit(&points, Vec3::ZERO, 0.0, 0.0, 0.1, &projection, &camera)
            .expect("vertical line should produce a valid fit");

        assert!(
            fit.radius < object_radius * 10.0,
            "radius {:.1} should be less than 10x object_radius {:.1}",
            fit.radius,
            object_radius,
        );
    }
}

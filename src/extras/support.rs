//! Shared utility functions used across multiple modules.

use bevy::prelude::*;

// ============================================================================
// Camera basis
// ============================================================================

/// Camera basis vectors extracted from a `GlobalTransform`.
/// Bundles the position and orientation vectors that are frequently passed together.
pub(super) struct CameraBasis {
    pub pos:     Vec3,
    pub right:   Vec3,
    pub up:      Vec3,
    pub forward: Vec3,
}

impl CameraBasis {
    pub fn from_global_transform(global: &GlobalTransform) -> Self {
        let rot = global.rotation();
        Self {
            pos:     global.translation(),
            right:   rot * Vec3::X,
            up:      rot * Vec3::Y,
            forward: rot * Vec3::NEG_Z,
        }
    }
}

// ============================================================================
// Projection utilities
// ============================================================================

/// Minimum depth for a point to be considered in front of the camera.
/// Points at or below this depth are treated as behind the camera in perspective projection.
pub(super) const MIN_VISIBLE_DEPTH: f32 = 0.1;

/// Projection-derived parameters for screen-space normalization.
/// Consolidates the extraction of half extents and projection type from a `Projection`.
pub(super) struct ProjectionParams {
    /// Half visible extent in x (perspective: `half_tan_hfov`, ortho: `area.width()/2`)
    pub half_extent_x: f32,
    /// Half visible extent in y (perspective: `half_tan_vfov`, ortho: `area.height()/2`)
    pub half_extent_y: f32,
    /// Whether this uses orthographic projection
    pub is_ortho:      bool,
}

impl ProjectionParams {
    /// Extracts projection parameters from a `Projection` and viewport aspect ratio.
    /// Returns `None` for unsupported projection variants.
    pub fn from_projection(projection: &Projection, viewport_aspect: f32) -> Option<Self> {
        let is_ortho = matches!(projection, Projection::Orthographic(_));
        let (half_extent_x, half_extent_y) = match projection {
            Projection::Perspective(p) => {
                let half_tan_vfov = (p.fov * 0.5).tan();
                (half_tan_vfov * viewport_aspect, half_tan_vfov)
            },
            Projection::Orthographic(o) => (o.area.width() * 0.5, o.area.height() * 0.5),
            Projection::Custom(_) => return None,
        };
        Some(Self {
            half_extent_x,
            half_extent_y,
            is_ortho,
        })
    }
}

/// Projects a world-space point to normalized screen coordinates.
///
/// Returns `(norm_x, norm_y, depth)` or `None` if the point is behind the camera
/// (perspective only — orthographic points are always valid).
pub(super) fn project_point(
    point: Vec3,
    cam: &CameraBasis,
    is_ortho: bool,
) -> Option<(f32, f32, f32)> {
    let relative = point - cam.pos;
    let depth = relative.dot(cam.forward);
    if !is_ortho && depth <= MIN_VISIBLE_DEPTH {
        return None;
    }
    let x = relative.dot(cam.right);
    let y = relative.dot(cam.up);
    let (norm_x, norm_y) = if is_ortho {
        (x, y)
    } else {
        (x / depth, y / depth)
    };
    Some((norm_x, norm_y, depth))
}

/// Extracts the aspect ratio from a `Projection`, using `viewport_size` for
/// perspective when available, falling back to `PerspectiveProjection::aspect_ratio`.
///
/// Returns `None` for orthographic projections with zero-height area or unknown
/// projection variants.
pub(super) fn projection_aspect_ratio(
    projection: &Projection,
    viewport_size: Option<Vec2>,
) -> Option<f32> {
    match projection {
        Projection::Perspective(p) => Some(viewport_size.map_or(p.aspect_ratio, |s| s.x / s.y)),
        Projection::Orthographic(o) => {
            let area = o.area;
            if area.height().abs() < f32::EPSILON {
                return None;
            }
            Some(area.width() / area.height())
        },
        Projection::Custom(_) => None,
    }
}

// ============================================================================
// Screen-space bounds
// ============================================================================

/// Depths of the extreme projected points, tracked during the projection loop.
/// Used by the fit algorithm for perspective-correct centering (harmonic mean)
/// and by visualization for average depth (gizmo placement).
#[derive(Debug, Clone)]
pub(super) struct PointDepths {
    pub min_x_depth: f32,
    pub max_x_depth: f32,
    pub min_y_depth: f32,
    pub max_y_depth: f32,
    #[cfg(feature = "extras_debug")]
    pub depth_sum:   f32,
    #[cfg(feature = "extras_debug")]
    pub point_count: usize,
}

/// Screen-space bounds of a set of projected points, with margin distances
/// from each screen edge.
#[derive(Debug, Clone)]
pub(super) struct ScreenSpaceBounds {
    /// Distance from left edge (positive = inside, negative = outside)
    pub left_margin:   f32,
    /// Distance from right edge (positive = inside, negative = outside)
    pub right_margin:  f32,
    /// Distance from top edge (positive = inside, negative = outside)
    pub top_margin:    f32,
    /// Distance from bottom edge (positive = inside, negative = outside)
    pub bottom_margin: f32,
    /// Minimum normalized x coordinate in screen space
    pub min_norm_x:    f32,
    /// Maximum normalized x coordinate in screen space
    pub max_norm_x:    f32,
    /// Minimum normalized y coordinate in screen space
    pub min_norm_y:    f32,
    /// Maximum normalized y coordinate in screen space
    pub max_norm_y:    f32,
    /// Half visible extent in x (perspective: `half_tan_hfov`, ortho: `area.width()/2`)
    pub half_extent_x: f32,
    /// Half visible extent in y (perspective: `half_tan_vfov`, ortho: `area.height()/2`)
    pub half_extent_y: f32,
}

impl ScreenSpaceBounds {
    /// Projects world-space points to normalized screen space and computes margins.
    /// Returns `None` if any point is behind the camera (perspective only).
    #[allow(clippy::similar_names)]
    pub fn from_points(
        points: &[Vec3],
        cam_global: &GlobalTransform,
        projection: &Projection,
        viewport_aspect: f32,
    ) -> Option<(Self, PointDepths)> {
        let ProjectionParams {
            half_extent_x,
            half_extent_y,
            is_ortho,
        } = ProjectionParams::from_projection(projection, viewport_aspect)?;

        let cam = CameraBasis::from_global_transform(cam_global);

        let mut min_norm_x = f32::INFINITY;
        let mut max_norm_x = f32::NEG_INFINITY;
        let mut min_norm_y = f32::INFINITY;
        let mut max_norm_y = f32::NEG_INFINITY;
        let mut min_x_depth = 0.0_f32;
        let mut max_x_depth = 0.0_f32;
        let mut min_y_depth = 0.0_f32;
        let mut max_y_depth = 0.0_f32;
        #[cfg(feature = "extras_debug")]
        let mut depth_sum = 0.0_f32;

        for point in points {
            let (norm_x, norm_y, depth) = project_point(*point, &cam, is_ortho)?;

            #[cfg(feature = "extras_debug")]
            {
                depth_sum += depth;
            }

            if norm_x < min_norm_x {
                min_norm_x = norm_x;
                min_x_depth = depth;
            }
            if norm_x > max_norm_x {
                max_norm_x = norm_x;
                max_x_depth = depth;
            }
            if norm_y < min_norm_y {
                min_norm_y = norm_y;
                min_y_depth = depth;
            }
            if norm_y > max_norm_y {
                max_norm_y = norm_y;
                max_y_depth = depth;
            }
        }

        let left_margin = min_norm_x - (-half_extent_x);
        let right_margin = half_extent_x - max_norm_x;
        let bottom_margin = min_norm_y - (-half_extent_y);
        let top_margin = half_extent_y - max_norm_y;

        let bounds = Self {
            left_margin,
            right_margin,
            top_margin,
            bottom_margin,
            min_norm_x,
            max_norm_x,
            min_norm_y,
            max_norm_y,
            half_extent_x,
            half_extent_y,
        };

        let depths = PointDepths {
            min_x_depth,
            max_x_depth,
            min_y_depth,
            max_y_depth,
            #[cfg(feature = "extras_debug")]
            depth_sum,
            #[cfg(feature = "extras_debug")]
            point_count: points.len(),
        };

        Some((bounds, depths))
    }

    /// Returns the center of the bounds in normalized screen space.
    pub const fn center(&self) -> (f32, f32) {
        let center_x = (self.min_norm_x + self.max_norm_x) * 0.5;
        let center_y = (self.min_norm_y + self.max_norm_y) * 0.5;
        (center_x, center_y)
    }
}

// ============================================================================
// Mesh utilities
// ============================================================================

/// Extracts world-space vertex positions from all meshes on an entity and its descendants.
/// Returns `(vertices, geometric_center)` where `geometric_center` is the root entity's
/// `GlobalTransform` translation.
pub(super) fn extract_mesh_vertices(
    entity: Entity,
    children_query: &Query<&Children>,
    mesh_query: &Query<&Mesh3d>,
    global_transform_query: &Query<&GlobalTransform>,
    meshes: &Assets<Mesh>,
) -> Option<(Vec<Vec3>, Vec3)> {
    let mesh_entities: Vec<Entity> = std::iter::once(entity)
        .chain(children_query.iter_descendants(entity))
        .filter(|e| mesh_query.get(*e).is_ok())
        .collect();

    if mesh_entities.is_empty() {
        return None;
    }

    let mut all_vertices = Vec::new();

    for mesh_entity in &mesh_entities {
        let Ok(mesh3d) = mesh_query.get(*mesh_entity) else {
            continue;
        };
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue;
        };
        let Ok(global_transform) = global_transform_query.get(*mesh_entity) else {
            continue;
        };
        let Some(positions) = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
        else {
            continue;
        };

        all_vertices.extend(
            positions
                .iter()
                .map(|pos| global_transform.transform_point(Vec3::from_array(*pos))),
        );
    }

    if all_vertices.is_empty() {
        return None;
    }

    let geometric_center = global_transform_query
        .get(entity)
        .map_or(Vec3::ZERO, GlobalTransform::translation);

    Some((all_vertices, geometric_center))
}

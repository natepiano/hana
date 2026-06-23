use bevy::prelude::*;
use bevy_kana::ToF32;

use super::context::FitOverlayCameraContext;
use super::context::FitOverlayEmptyReason;
use crate::fit::projection;
use crate::fit::projection::CameraBasis;
use crate::fit::projection::ProjectionMode;
use crate::fit::projection::ScreenSpaceBounds;

/// Desired overlay frame for one source camera.
pub(super) enum FitOverlayFrame {
    /// The source camera has enough data to render the overlay.
    Visible(Box<FitOverlayLayout>),
    /// The source camera should have no generated visuals this frame.
    Empty(FitOverlayEmptyReason),
}

/// Computed overlay layout for one visible source camera.
pub(super) struct FitOverlayLayout {
    /// Source camera render context.
    pub(super) context:         FitOverlayCameraContext,
    /// Extracted world-space target vertices.
    pub(super) vertices:        Vec<Vec3>,
    /// Projected target bounds in screen space.
    pub(super) bounds:          ScreenSpaceBounds,
    /// Source camera basis vectors.
    pub(super) camera_basis:    CameraBasis,
    /// Average projected target depth used for overlay placement.
    pub(super) average_depth:   f32,
    /// Projection mode used by overlay coordinate conversion.
    pub(super) projection_mode: ProjectionMode,
    /// Logical viewport size for label placement.
    pub(super) viewport_size:   Vec2,
}

impl FitOverlayLayout {
    pub(super) fn from_vertices(
        context: FitOverlayCameraContext,
        camera_global: &GlobalTransform,
        projection: &Projection,
        vertices: Vec<Vec3>,
    ) -> FitOverlayFrame {
        let viewport_size = context.viewport_size();
        let Some(aspect_ratio) =
            projection::projection_aspect_ratio(projection, Some(viewport_size))
        else {
            return FitOverlayFrame::Empty(FitOverlayEmptyReason::UnsupportedProjection);
        };

        let Some((bounds, depths)) =
            ScreenSpaceBounds::from_points(&vertices, camera_global, projection, aspect_ratio)
        else {
            return FitOverlayFrame::Empty(FitOverlayEmptyReason::UnprojectableBounds);
        };

        if depths.count == 0 {
            return FitOverlayFrame::Empty(FitOverlayEmptyReason::MissingDepths);
        }

        let projection_mode = match projection {
            Projection::Perspective(_) => ProjectionMode::Perspective,
            Projection::Orthographic(_) => ProjectionMode::Orthographic,
            Projection::Custom(_) => {
                return FitOverlayFrame::Empty(FitOverlayEmptyReason::UnsupportedProjection);
            },
        };

        FitOverlayFrame::Visible(Box::new(Self {
            context,
            vertices,
            bounds,
            camera_basis: CameraBasis::from(camera_global),
            average_depth: depths.sum / depths.count.to_f32(),
            projection_mode,
            viewport_size,
        }))
    }
}

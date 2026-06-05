use bevy::prelude::*;

use crate::fit::Edge;

/// Generated fit-overlay visual owned by a source camera.
///
/// The `camera` identifies the entity with `FitOverlay` that requested this
/// visual. The retained overlay renderer uses that camera's effective
/// `RenderLayers`; any camera with intersecting effective layers may render the
/// visual in its normal camera pass.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
pub(super) struct FitOverlayVisual {
    /// Camera entity that owns update and cleanup for this visual.
    pub(super) camera: Entity,
    /// Stable identity for the visual within its source camera's overlay.
    pub(super) kind:   FitOverlayVisualKind,
}

/// Stable identity for a generated fit-overlay visual.
#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FitOverlayVisualKind {
    /// Screen-aligned fit target rectangle.
    Rectangle,
    /// Convex hull silhouette of the current fit target.
    Silhouette,
    /// Margin line from a projected target edge to the viewport edge.
    MarginLine {
        /// Margin edge represented by this visual.
        edge: Edge,
    },
    /// Margin percentage label for a viewport edge.
    MarginLabel {
        /// Margin edge represented by this label.
        edge: Edge,
    },
    /// Label for the projected screen-space bounds.
    BoundsLabel,
}

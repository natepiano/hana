//! Shared projection and conversion errors.

use bevy::transform::helper::ComputeGlobalTransformError;

use crate::panel::PanelAnchorGeometryError;

/// Why a panel could not be projected or converted.
#[derive(thiserror::Error, Clone, Copy, Debug, Eq, PartialEq)]
pub enum PanelProjectionError {
    /// The panel entity has no [`DiegeticPanel`](crate::DiegeticPanel).
    #[error("panel is missing")]
    PanelMissing,
    /// The camera entity has no [`Camera`](bevy::prelude::Camera).
    #[error("camera is missing")]
    CameraMissing,
    /// The camera does not render to a window target.
    #[error("camera target is not a window")]
    UnsupportedCameraTarget,
    /// The target window could not be resolved.
    #[error("window is missing")]
    WindowMissing,
    /// The camera has no usable viewport size yet.
    #[error("camera viewport size is unavailable")]
    NoViewportSize,
    /// A transform needed for projection could not be computed.
    #[error("transform is unavailable")]
    TransformUnavailable,
    /// The panel dimensions were non-finite or non-positive.
    #[error("panel size is invalid")]
    InvalidPanelSize,
    /// The panel's world plane was degenerate.
    #[error("panel plane is invalid")]
    InvalidPanelPlane,
    /// The world target was missing a usable plane or size.
    #[error("world target is invalid")]
    InvalidWorldTarget,
    /// The panel has no saved screen handoff camera/depth.
    #[error("screen handoff is missing")]
    ScreenHandoffMissing,
    /// The panel has no saved world-authored state.
    #[error("saved world state is missing")]
    SavedWorldStateMissing,
    /// The camera could not project or unproject the panel.
    #[error("panel projection failed")]
    ProjectionFailed,
    /// The resulting projection was non-finite or zero-sized.
    #[error("panel projection is invalid")]
    InvalidProjection,
}

impl From<ComputeGlobalTransformError> for PanelProjectionError {
    fn from(_: ComputeGlobalTransformError) -> Self { Self::TransformUnavailable }
}

impl From<PanelAnchorGeometryError> for PanelProjectionError {
    fn from(error: PanelAnchorGeometryError) -> Self {
        match error {
            PanelAnchorGeometryError::PanelMissing => Self::PanelMissing,
            PanelAnchorGeometryError::WindowMissing => Self::WindowMissing,
            PanelAnchorGeometryError::WindowZeroSized => Self::NoViewportSize,
            PanelAnchorGeometryError::TransformUnavailable => Self::TransformUnavailable,
            PanelAnchorGeometryError::InvalidPanelSize => Self::InvalidPanelSize,
            PanelAnchorGeometryError::InvalidPanelPlane => Self::InvalidPanelPlane,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use bevy::prelude::Entity;

    use super::*;

    #[test]
    fn panel_projection_error_messages_are_stable() {
        let cases = [
            (PanelProjectionError::PanelMissing, "panel is missing"),
            (PanelProjectionError::CameraMissing, "camera is missing"),
            (
                PanelProjectionError::UnsupportedCameraTarget,
                "camera target is not a window",
            ),
            (PanelProjectionError::WindowMissing, "window is missing"),
            (
                PanelProjectionError::NoViewportSize,
                "camera viewport size is unavailable",
            ),
            (
                PanelProjectionError::TransformUnavailable,
                "transform is unavailable",
            ),
            (
                PanelProjectionError::InvalidPanelSize,
                "panel size is invalid",
            ),
            (
                PanelProjectionError::InvalidPanelPlane,
                "panel plane is invalid",
            ),
            (
                PanelProjectionError::InvalidWorldTarget,
                "world target is invalid",
            ),
            (
                PanelProjectionError::ScreenHandoffMissing,
                "screen handoff is missing",
            ),
            (
                PanelProjectionError::SavedWorldStateMissing,
                "saved world state is missing",
            ),
            (
                PanelProjectionError::ProjectionFailed,
                "panel projection failed",
            ),
            (
                PanelProjectionError::InvalidProjection,
                "panel projection is invalid",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn transform_error_conversion_is_lossy() {
        let error = PanelProjectionError::from(ComputeGlobalTransformError::MissingTransform(
            Entity::PLACEHOLDER,
        ));

        assert_eq!(error, PanelProjectionError::TransformUnavailable);
        assert!(error.source().is_none());
    }

    #[test]
    fn anchor_geometry_error_conversions_are_normalized() {
        let cases = [
            (
                PanelAnchorGeometryError::PanelMissing,
                PanelProjectionError::PanelMissing,
            ),
            (
                PanelAnchorGeometryError::WindowMissing,
                PanelProjectionError::WindowMissing,
            ),
            (
                PanelAnchorGeometryError::WindowZeroSized,
                PanelProjectionError::NoViewportSize,
            ),
            (
                PanelAnchorGeometryError::TransformUnavailable,
                PanelProjectionError::TransformUnavailable,
            ),
            (
                PanelAnchorGeometryError::InvalidPanelSize,
                PanelProjectionError::InvalidPanelSize,
            ),
            (
                PanelAnchorGeometryError::InvalidPanelPlane,
                PanelProjectionError::InvalidPanelPlane,
            ),
        ];

        for (source, expected) in cases {
            let error = PanelProjectionError::from(source);
            assert_eq!(error, expected);
            assert!(error.source().is_none());
        }
    }
}

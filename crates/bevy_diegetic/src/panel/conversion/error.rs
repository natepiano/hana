//! Shared projection and conversion errors.

use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use bevy::transform::helper::ComputeGlobalTransformError;

use crate::panel::PanelAnchorGeometryError;

/// Why a panel could not be projected or converted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PanelProjectionError {
    /// The panel entity has no [`DiegeticPanel`](crate::DiegeticPanel).
    PanelMissing,
    /// The camera entity has no [`Camera`](bevy::prelude::Camera).
    CameraMissing,
    /// The camera does not render to a window target.
    UnsupportedCameraTarget,
    /// The target window could not be resolved.
    WindowMissing,
    /// The camera has no usable viewport size yet.
    NoViewportSize,
    /// A transform needed for projection could not be computed.
    TransformUnavailable,
    /// The panel dimensions were non-finite or non-positive.
    InvalidPanelSize,
    /// The panel's world plane was degenerate.
    InvalidPanelPlane,
    /// The world target was missing a usable plane or size.
    InvalidWorldTarget,
    /// The panel has no saved screen handoff camera/depth.
    ScreenHandoffMissing,
    /// The panel has no saved world-authored state.
    SavedWorldStateMissing,
    /// The camera could not project or unproject the panel.
    ProjectionFailed,
    /// The resulting projection was non-finite or zero-sized.
    InvalidProjection,
}

impl Display for PanelProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PanelMissing => formatter.write_str("panel is missing"),
            Self::CameraMissing => formatter.write_str("camera is missing"),
            Self::UnsupportedCameraTarget => formatter.write_str("camera target is not a window"),
            Self::WindowMissing => formatter.write_str("window is missing"),
            Self::NoViewportSize => formatter.write_str("camera viewport size is unavailable"),
            Self::TransformUnavailable => formatter.write_str("transform is unavailable"),
            Self::InvalidPanelSize => formatter.write_str("panel size is invalid"),
            Self::InvalidPanelPlane => formatter.write_str("panel plane is invalid"),
            Self::InvalidWorldTarget => formatter.write_str("world target is invalid"),
            Self::ScreenHandoffMissing => formatter.write_str("screen handoff is missing"),
            Self::SavedWorldStateMissing => formatter.write_str("saved world state is missing"),
            Self::ProjectionFailed => formatter.write_str("panel projection failed"),
            Self::InvalidProjection => formatter.write_str("panel projection is invalid"),
        }
    }
}

impl Error for PanelProjectionError {}

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

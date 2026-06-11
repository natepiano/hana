#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on Reflect derive for CapStyle::Flat { normal }"
)]

use bevy::prelude::*;

use super::constants::DEFAULT_ARM_MULTIPLIER;
use super::constants::DEFAULT_ELBOW_ANGLE_THRESHOLD_DEG;
use super::constants::DEFAULT_ELBOW_BEND_RADIUS_MULTIPLIER;
use super::constants::DEFAULT_ELBOW_RINGS_PER_RIGHT_ANGLE;
use super::constants::DEFAULT_MIN_ELBOW_RADIUS_MULTIPLIER;
use super::constants::DEFAULT_TUBE_RADIUS;
use super::constants::DEFAULT_TUBE_SIDES;

/// How to cap each end of a tube mesh.
///
/// Surface normal is only relevant for [`CapStyle::Flat`] caps and is encoded directly
/// in the variant, so invalid states are unrepresentable.
#[derive(Clone, Debug, Default, Reflect)]
pub enum CapStyle {
    /// Open end, so no cap geometry is generated.
    None,
    /// Hemisphere cap with a smooth rounded end.
    #[default]
    Round,
    /// Flat disc cap. `normal` determines orientation.
    Flat {
        /// Cap orientation normal. `None` uses the cable tangent.
        normal: Option<Vec3>,
    },
}

impl CapStyle {
    /// Flat cap using the cable's tangent direction.
    #[must_use]
    pub const fn flat() -> Self { Self::Flat { normal: None } }

    /// Flat cap with an explicit orientation normal.
    #[must_use]
    pub const fn flat_with_normal(normal: Vec3) -> Self {
        Self::Flat {
            normal: Some(normal),
        }
    }
}

/// Which sides of the tube surface to render.
#[derive(Clone, Debug, Default, Reflect)]
pub enum Faces {
    /// Render only the outside.
    #[default]
    Outside,
    /// Render only the inside.
    Inside,
    /// Render both sides.
    Both,
}

/// Configuration for cable mesh generation.
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct CableMeshConfig {
    /// Tube cross-section: radius, side count, and rendered faces.
    pub tube_config:  TubeConfig,
    /// Cap styles at each end of the tube.
    pub cap_config:   CapConfig,
    /// Distance to trim the tube back from each end.
    pub trim_config:  TrimConfig,
    /// Elbow filleting between non-collinear tangents.
    pub elbow_config: ElbowConfig,
    /// Material to apply to the generated mesh. If `None`, no material is added.
    pub material:     Option<Handle<StandardMaterial>>,
}

/// Tube cross-section configuration.
#[derive(Clone, Debug, Reflect)]
pub struct TubeConfig {
    /// Radius of the tube cross-section.
    pub radius: f32,
    /// Number of vertices around the cross-section circle.
    pub sides:  u32,
    /// Which sides of the tube surface to render.
    pub faces:  Faces,
}

impl Default for TubeConfig {
    fn default() -> Self {
        Self {
            radius: DEFAULT_TUBE_RADIUS,
            sides:  DEFAULT_TUBE_SIDES,
            faces:  Faces::default(),
        }
    }
}

/// Cap style at each end of the tube.
#[derive(Clone, Debug, Reflect)]
pub struct CapConfig {
    /// Cap style for the start end of the tube.
    pub start: CapStyle,
    /// Cap style for the end of the tube.
    pub end:   CapStyle,
}

impl Default for CapConfig {
    fn default() -> Self {
        Self {
            start: CapStyle::Round,
            end:   CapStyle::Round,
        }
    }
}

/// Distance to trim from each end of the tube path.
#[derive(Clone, Debug, Default, Reflect)]
pub struct TrimConfig {
    /// Distance to trim from the start.
    pub start: f32,
    /// Distance to trim from the end.
    pub end:   f32,
}

/// Elbow filleting configuration.
#[derive(Clone, Debug, Reflect)]
pub struct ElbowConfig {
    /// Elbow bend radius multiplier relative to tube radius.
    pub bend_radius_multiplier: f32,
    /// Minimum elbow radius multiplier. Below this, elbows are skipped.
    pub min_radius_multiplier:  f32,
    /// Number of rings per 90 degrees of elbow bend.
    pub rings_per_right_angle:  u32,
    /// Minimum angle between consecutive tangents to trigger an elbow.
    pub angle_threshold_deg:    f32,
    /// Multiplier for Bezier arm length at elbows.
    pub arm_multiplier:         f32,
    /// Per-elbow arm overrides as `(control1_arm, control2_arm)` distances.
    pub arm_overrides:          Option<Vec<(f32, f32)>>,
}

impl Default for ElbowConfig {
    fn default() -> Self {
        Self {
            bend_radius_multiplier: DEFAULT_ELBOW_BEND_RADIUS_MULTIPLIER,
            min_radius_multiplier:  DEFAULT_MIN_ELBOW_RADIUS_MULTIPLIER,
            rings_per_right_angle:  DEFAULT_ELBOW_RINGS_PER_RIGHT_ANGLE,
            angle_threshold_deg:    DEFAULT_ELBOW_ANGLE_THRESHOLD_DEG,
            arm_multiplier:         DEFAULT_ARM_MULTIPLIER,
            arm_overrides:          None,
        }
    }
}

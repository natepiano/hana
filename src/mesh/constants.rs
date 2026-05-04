//! Tuning defaults and thresholds for tube mesh generation.

// Cap defaults
pub(super) const MIN_CAP_RINGS: u32 = 8;

// Elbow defaults
pub(super) const DEFAULT_ARM_MULTIPLIER: f32 = 1.0;
pub(super) const DEFAULT_ELBOW_ANGLE_THRESHOLD_DEG: f32 = 25.0;
/// Default Bezier arm length as a fraction of the fillet chord.
pub(super) const DEFAULT_ELBOW_ARM_FRACTION: f32 = 1.0 / 3.0;
pub(super) const DEFAULT_ELBOW_BEND_RADIUS_MULTIPLIER: f32 = 1.0;
pub(super) const DEFAULT_ELBOW_RINGS_PER_RIGHT_ANGLE: u32 = 32;
pub(super) const DEFAULT_MIN_ELBOW_RADIUS_MULTIPLIER: f32 = 0.5;
/// Maximum ratio of arm length to fillet reach, preventing self-intersecting loops.
pub(super) const MAX_ARM_RATIO: f32 = 0.95;
/// Minimum number of rings per elbow fillet.
pub(super) const MIN_ELBOW_RINGS: f32 = 3.0;

// Perpendicular detection
/// Dot-product threshold above which a vector is considered near-parallel to an axis.
pub(super) const PERPENDICULAR_AXIS_THRESHOLD: f32 = 0.9;

// Tube defaults
pub(super) const DEFAULT_TUBE_RADIUS: f32 = 0.06;
pub(super) const DEFAULT_TUBE_SIDES: u32 = 32;
/// Minimum polygon sides needed to form a closed tube cross-section.
pub(super) const MIN_TUBE_SIDES: u32 = 3;

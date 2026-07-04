//! Cross-module constants.

/// First numeric suffix appended to deduplicate a managed window name (e.g. `name-2`).
pub(crate) const FIRST_DUPLICATE_SUFFIX: u32 = 2;
pub(crate) const MANAGED_WINDOW_NAME_SEPARATOR: &str = "-";

pub(crate) const MONITOR_SOURCE_EXISTING: &str = "existing";
pub(crate) const MONITOR_SOURCE_FALLBACK: &str = "fallback";
pub(crate) const MONITOR_SOURCE_POSITION: &str = "position";
pub(crate) const MONITOR_SOURCE_WINIT: &str = "winit";

pub(crate) const PRIMARY_MONITOR_INDEX: usize = 0;

pub(crate) const CURRENT_STATE_VERSION: u8 = 2;
pub(crate) const PRIMARY_WINDOW_KEY: &str = "primary";
pub(crate) const STATE_FILE: &str = "windows.ron";

#[cfg(target_os = "linux")]
pub(crate) const WAYLAND_DISPLAY_ENVIRONMENT_VARIABLE: &str = "WAYLAND_DISPLAY";

pub(crate) const RESTORE_STRATEGY_APPLY_UNCHANGED: &str = "ApplyUnchanged";
pub(crate) const RESTORE_STRATEGY_LOWER_TO_HIGHER: &str = "LowerToHigher";

/// Fallback scale factor when the monitor cannot be determined.
pub(crate) const DEFAULT_SCALE_FACTOR: f64 = 1.0;
/// Threshold for considering two scale factors equal.
///
/// Accounts for floating-point imprecision when comparing scale factors.
/// A difference less than this epsilon is considered negligible.
pub(crate) const SCALE_FACTOR_EPSILON: f64 = 0.01;

/// Duration (in seconds) that all values must remain stable before declaring success.
pub(crate) const SETTLE_STABILITY_SECS: f32 = 0.2;
/// Maximum total duration (in seconds) to wait for values to stabilize.
pub(crate) const SETTLE_TIMEOUT_SECS: f32 = 2.0;

/// Header comment prepended to the RON file to document the coordinate contract.
pub(crate) const RON_HEADER: &str = "\
// All spatial values (position, size) are in logical pixels.
// monitor_scale: scale factor at save time (informational, not used during restore).
";

pub(crate) const MILLIS_PER_SECOND: f32 = 1000.0;

#[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
pub(crate) const DPI_CHANGE_HANDLED_RESULT: isize = 0;
/// Win32 subclass identifier for DPI-change interception.
#[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
pub(crate) const SUBCLASS_ID: usize = 1;
#[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
pub(crate) const SUBCLASS_REFERENCE_DATA: usize = 0;

/// Number of values in `_NET_FRAME_EXTENTS` (left, right, top, bottom).
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENT_COUNT: u32 = 4;
/// X11 property offset used when querying `_NET_FRAME_EXTENTS`.
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENT_PROPERTY_OFFSET: u32 = 0;
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENTS_ATOM_NAME: &[u8] = b"_NET_FRAME_EXTENTS";
/// Index of the "top" extent in `_NET_FRAME_EXTENTS`.
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENT_TOP_INDEX: usize = 2;

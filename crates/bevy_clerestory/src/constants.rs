//! Cross-module constants.

// managed window names
/// First numeric suffix appended to deduplicate a managed window name (e.g. `name-2`).
pub(crate) const FIRST_DUPLICATE_SUFFIX: u32 = 2;
pub(crate) const MANAGED_WINDOW_NAME_SEPARATOR: &str = "-";

// monitor identity
/// Length of a ColorSync display UUID, fixed by `CFUUIDBytes`.
#[cfg(target_os = "macos")]
pub(crate) const MACOS_DISPLAY_UUID_BYTES: usize = 16;
/// FNV-1a 64-bit offset basis, fixed by the algorithm's specification.
pub(crate) const FNV_1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime, fixed by the algorithm's specification.
pub(crate) const FNV_1A_PRIME: u64 = 0x0000_0100_0000_01b3;

// monitor lookup
pub(crate) const MONITOR_SOURCE_EXISTING: &str = "existing";
pub(crate) const MONITOR_SOURCE_FALLBACK: &str = "fallback";
pub(crate) const MONITOR_SOURCE_POSITION: &str = "position";
pub(crate) const MONITOR_SOURCE_WINIT: &str = "winit";
pub(crate) const PRIMARY_MONITOR_INDEX: usize = 0;

// monitor probe
#[cfg(feature = "monitor-probe")]
pub(crate) const MONITOR_PROBE_TARGET: &str = "bevy_clerestory::monitor_probe";
#[cfg(feature = "monitor-probe")]
pub(crate) const RECOVERY_ACCEPTANCE_PRODUCER: &str = "Update::accept_eligible_registrations";
#[cfg(feature = "monitor-probe")]
pub(crate) const RECOVERY_PROBE_TARGET: &str = "bevy_clerestory::recovery_probe";

// persisted state
pub(crate) const CURRENT_STATE_VERSION: u8 = 3;
pub(crate) const PRIMARY_WINDOW_KEY: &str = "primary";
/// Header comment prepended to the RON file to document the coordinate contract.
pub(crate) const RON_HEADER: &str = "\
// Sizes are in logical pixels.
// position: MonitorOffset is logical pixels from monitor_index's top-left corner.
// position: Unrebased is a pre-v3 absolute desktop coordinate awaiting a live monitor layout.
";
pub(crate) const STATE_FILE: &str = "windows.ron";

// platform detection
#[cfg(target_os = "linux")]
pub(crate) const WAYLAND_DISPLAY_ENVIRONMENT_VARIABLE: &str = "WAYLAND_DISPLAY";

// restore strategies
pub(crate) const RESTORE_STRATEGY_APPLY_UNCHANGED: &str = "ApplyUnchanged";
pub(crate) const RESTORE_STRATEGY_LOWER_TO_HIGHER: &str = "LowerToHigher";

// scale factors
/// Fallback scale factor when the monitor cannot be determined.
pub(crate) const DEFAULT_SCALE_FACTOR: f64 = 1.0;
/// Threshold for considering two scale factors equal.
///
/// Accounts for floating-point imprecision when comparing scale factors.
/// A difference less than this epsilon is considered negligible.
pub(crate) const SCALE_FACTOR_EPSILON: f64 = 0.01;

// settling
/// Maximum duration (in seconds) a cross-DPI restore waits for the scale change that moves it
/// out of `WindowRestoreState::WaitingForScaleChange`.
///
/// Neither signal that ends the wait is guaranteed. `WM_DPICHANGED` reaches a hidden window only
/// because `windows_dpi_fix` forwards it, and a target monitor that no longer matches any live
/// display is never arrived at. Without a deadline such a restore waits forever with the window
/// still hidden; on expiry it applies the final size and reveals the window instead, so settling
/// can report the mismatch.
pub(crate) const SCALE_CHANGE_WAIT_TIMEOUT_SECS: f32 = SETTLE_TIMEOUT_SECS;
/// Duration (in seconds) that all values must remain stable before declaring success.
pub(crate) const SETTLE_STABILITY_SECS: f32 = 0.2;
/// Maximum total duration (in seconds) to wait for values to stabilize.
pub(crate) const SETTLE_TIMEOUT_SECS: f32 = 2.0;
/// Maximum duration of a runtime restore, including shell creation and preparation.
pub(crate) const RUNTIME_RESTORE_TIMEOUT_SECS: f32 = SETTLE_TIMEOUT_SECS;

// time conversion
pub(crate) const MILLIS_PER_SECOND: f32 = 1000.0;

// windows display-config acquisition
#[cfg(target_os = "windows")]
pub(crate) const DISPLAY_CONFIG_ACQUISITION_ATTEMPTS: usize = 3;

// windows dpi interception
#[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
pub(crate) const DPI_CHANGE_HANDLED_RESULT: isize = 0;
/// Win32 subclass identifier for DPI-change interception.
#[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
pub(crate) const SUBCLASS_ID: usize = 1;
#[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
pub(crate) const SUBCLASS_REFERENCE_DATA: usize = 0;

// x11 frame extents
/// Number of values in `_NET_FRAME_EXTENTS` (left, right, top, bottom).
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENT_COUNT: u32 = 4;
/// X11 property offset used when querying `_NET_FRAME_EXTENTS`.
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENT_PROPERTY_OFFSET: u32 = 0;
/// Index of the "top" extent in `_NET_FRAME_EXTENTS`.
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENT_TOP_INDEX: usize = 2;
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) const FRAME_EXTENTS_ATOM_NAME: &[u8] = b"_NET_FRAME_EXTENTS";

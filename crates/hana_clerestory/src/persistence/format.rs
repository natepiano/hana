//! On-disk persistence format and version handling.
//!
//! # Versioning strategy
//!
//! Every RON state file carries a `version` field inside [`PersistedState`].
//! [`decode`] parses the file once, then dispatches to a version-specific
//! decoder based on that field. All previously shipped versions remain
//! supported so that users never lose their saved window positions.
//!
//! ## Adding a new version
//!
//! 1. Bump [`CURRENT_STATE_VERSION`].
//! 2. If the new version changes `PersistedEntry` or `PersistedWindowState` fields, add new structs
//!    (e.g. `PersistedEntryV2`) and a conversion from the old entry type. If only semantics change,
//!    the existing structs can be reused.
//! 3. Add a `decode_v<N>` function that accepts a [`PersistedState`] and returns
//!    `Option<HashMap<WindowKey, PersistedWindowState>>`.
//! 4. Add an arm to the `match persisted.version` block inside [`decode`].
//! 5. Update [`encode`] to write the new format (only the latest version is ever written).
//! 6. Add a test that round-trips through the new version **and** a test that an older version file
//!    still decodes correctly.
//!
//! ## Supported formats (oldest first)
//!
//! | Format | Description |
//! |--------|-------------|
//! | Legacy single-window | Bare window state (no version field, pre-multi-window) |
//! | v1 | `PersistedState { version: 1, entries }` with `width`/`height` (physical) |
//! | v2 | `PersistedState { version: 2, entries }` with `logical_width`/`logical_height` + `monitor_scale` |
//! | v3 | `PersistedState { version: 3, entries }` with `position: PersistedPosition` |
//!
//! ## v2 → v3: why the position representation changed
//!
//! v1 and v2 stored an **absolute logical desktop coordinate**. A monitor's logical origin is
//! its physical origin divided by its scale, so changing *any* monitor's scale renumbers the
//! desktop and silently relocates every saved window. v3 stores a scale-independent offset from
//! the window's own monitor instead.
//!
//! A v1/v2 coordinate cannot be converted without a live monitor layout to measure against, so
//! decode preserves it as [`PersistedPosition::Unrebased`] together with the scale that wrote it
//! and lets restore rebase it. The pair round-trips through v3 unchanged until then, so a window
//! that is never opened re-decides from the same two numbers on every launch rather than having
//! one arbitrary layout's approximation frozen into its file.

use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use bevy::prelude::*;
use ron::Error;
use ron::from_str;
use ron::ser::PrettyConfig;
use ron::ser::to_string_pretty;
use serde::Deserialize;
use serde::Serialize;

use super::constants::PERSISTED_STATE_VERSION_V1;
use super::constants::PERSISTED_STATE_VERSION_V2;
use super::window_state::PersistedPosition;
use super::window_state::PersistedWindowState;
#[cfg(test)]
use super::window_state::SavedVideoMode;
use super::window_state::SavedWindowMode;
use super::window_state::UnrebasedDesktopPosition;
use super::window_state::default_monitor_scale;
use crate::constants::CURRENT_STATE_VERSION;
#[cfg(test)]
use crate::constants::DEFAULT_SCALE_FACTOR;
use crate::constants::PRIMARY_WINDOW_KEY;
use crate::constants::RON_HEADER;
use crate::monitors::PanelIdentity;

/// Typed identifier for persisted window state.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
pub enum WindowKey {
    /// The application's primary window.
    Primary,
    /// A secondary managed window, identified by its name.
    Managed(String),
}

impl Display for WindowKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primary => write!(f, "{PRIMARY_WINDOW_KEY}"),
            Self::Managed(name) => write!(f, "{name}"),
        }
    }
}

/// One persisted key/state pair in the current format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedEntry {
    #[serde(rename = "key")]
    window_key:   WindowKey,
    #[serde(rename = "state")]
    window_state: PersistedWindowState,
}

/// Versioned persisted state format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedState {
    version: u8,
    entries: Vec<PersistedEntry>,
}

/// Minimal version probe — just extract the version number from any versioned format.
#[derive(Deserialize)]
struct VersionProbe {
    version: u8,
}

/// v1 window state layout (used `position` and `width`/`height` field names on the wire).
/// Used only for deserializing v1 and legacy files.
///
/// `deny_unknown_fields` for the same reason as [`WindowStateV2`]: a frozen format, where a
/// mismatched field name would otherwise decode to a silently empty value instead of an error.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowStateV1 {
    #[serde(rename = "position")]
    logical_position:  Option<(i32, i32)>,
    #[serde(rename = "width")]
    logical_width:     u32,
    #[serde(rename = "height")]
    logical_height:    u32,
    monitor_index:     usize,
    #[serde(rename = "mode")]
    saved_window_mode: SavedWindowMode,
    #[serde(default)]
    app_name:          String,
}

impl WindowStateV1 {
    /// Convert to the current `PersistedWindowState`, treating v1 values as logical at scale 1.0.
    fn into_current(self, window_key: &WindowKey) -> PersistedWindowState {
        PersistedWindowState {
            position:          legacy_position(
                self.logical_position,
                default_monitor_scale(),
                window_key,
            ),
            logical_width:     self.logical_width,
            logical_height:    self.logical_height,
            monitor:           self.monitor_index,
            // v1 and v2 carried no panel identity; the index is all these files ever had.
            monitor_panel:     PanelIdentity::Anonymous,
            saved_window_mode: self.saved_window_mode,
            app_name:          self.app_name,
        }
    }
}

/// v2 window state layout (absolute logical desktop position plus the scale that wrote it).
/// Used only for deserializing v2 files.
///
/// `deny_unknown_fields` is deliberate. v2 is a frozen format, so nothing new can legitimately
/// appear in one of its files, and without it a wire name that does not match here is not an
/// error: serde fills the missing `Option` with `None`, ignores the field that is actually
/// present, and the file decodes "successfully" with the user's saved position silently gone —
/// no warning, no backup, nothing to notice. Misnaming this field is exactly the mistake that
/// makes an upgrade quietly forget where every window was.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowStateV2 {
    logical_position:  Option<(i32, i32)>,
    logical_width:     u32,
    logical_height:    u32,
    #[serde(default = "default_monitor_scale", rename = "monitor_scale")]
    scale:             f64,
    #[serde(rename = "monitor_index")]
    monitor:           usize,
    #[serde(rename = "mode")]
    saved_window_mode: SavedWindowMode,
    #[serde(default)]
    app_name:          String,
}

impl WindowStateV2 {
    /// Convert to the current `PersistedWindowState`, preserving the saved coordinate and the
    /// scale that wrote it for restore to rebase against a live monitor layout.
    fn into_current(self, window_key: &WindowKey) -> PersistedWindowState {
        PersistedWindowState {
            position:          legacy_position(self.logical_position, self.scale, window_key),
            logical_width:     self.logical_width,
            logical_height:    self.logical_height,
            monitor:           self.monitor,
            // v1 and v2 carried no panel identity; the index is all these files ever had.
            monitor_panel:     PanelIdentity::Anonymous,
            saved_window_mode: self.saved_window_mode,
            app_name:          self.app_name,
        }
    }
}

/// Wrap a pre-v3 absolute coordinate for later rebasing, or report why it was discarded.
fn legacy_position(
    logical_position: Option<(i32, i32)>,
    captured_scale: f64,
    window_key: &WindowKey,
) -> PersistedPosition {
    let Some((x, y)) = logical_position else {
        return PersistedPosition::Unpositioned;
    };
    UnrebasedDesktopPosition::from_legacy(IVec2::new(x, y), captured_scale).map_or_else(
        || {
            warn!(
                "[legacy_position] [{window_key}] Discarding saved position: \
                 monitor_scale {captured_scale} is not a finite number greater than zero"
            );
            PersistedPosition::Unpositioned
        },
        PersistedPosition::Unrebased,
    )
}

/// v2 persisted entry (uses `WindowStateV2`).
#[derive(Debug, Clone, Deserialize)]
struct PersistedEntryV2 {
    #[serde(rename = "key")]
    window_key:   WindowKey,
    #[serde(rename = "state")]
    window_state: WindowStateV2,
}

/// v2 persisted state wrapper.
#[derive(Debug, Clone, Deserialize)]
struct PersistedStateV2 {
    entries: Vec<PersistedEntryV2>,
}

/// v1 persisted entry (uses `WindowStateV1`).
#[derive(Debug, Clone, Deserialize)]
struct PersistedEntryV1 {
    #[serde(rename = "key")]
    window_key:   WindowKey,
    #[serde(rename = "state")]
    window_state: WindowStateV1,
}

/// v1 persisted state wrapper.
#[derive(Debug, Clone, Deserialize)]
struct PersistedStateV1 {
    version: u8,
    entries: Vec<PersistedEntryV1>,
}

/// Decode persisted state text into typed runtime state.
///
/// Tries versioned formats first (dispatching by the `version` field),
/// then falls back to legacy unversioned formats. See the module-level
/// docs for the full list of supported formats.
pub(super) fn decode(contents: &str) -> Option<HashMap<WindowKey, PersistedWindowState>> {
    // Probe only `VersionProbe::version` before dispatching to `PersistedStateV1` or
    // `PersistedState`.
    if let Ok(probe) = from_str::<VersionProbe>(contents) {
        match probe.version {
            PERSISTED_STATE_VERSION_V1 => decode_v1(contents),
            PERSISTED_STATE_VERSION_V2 => decode_v2(contents),
            CURRENT_STATE_VERSION => decode_v3(contents),
            unsupported => {
                warn!(
                    "[decode] Unsupported persisted state version {unsupported} \
                     (latest supported: {CURRENT_STATE_VERSION})"
                );
                None
            },
        }
    } else {
        // Legacy unversioned format — bare `PersistedWindowState` from before multi-window
        // support. Cannot participate in the version match above because it has no
        // `version` field.
        decode_legacy_single_window(contents)
    }
}

/// Read just the version number out of a state file, for naming a backup of one that failed to
/// decode. Returns `None` for a legacy unversioned file or one too damaged to parse at all.
pub(super) fn probe_version(contents: &str) -> Option<u8> {
    from_str::<VersionProbe>(contents)
        .ok()
        .map(|probe| probe.version)
}

fn decode_legacy_single_window(contents: &str) -> Option<HashMap<WindowKey, PersistedWindowState>> {
    let window_state_v1 = from_str::<WindowStateV1>(contents).ok()?;
    debug!("[decode] Migrated legacy single-window format to v{CURRENT_STATE_VERSION}");
    Some(HashMap::from([(
        WindowKey::Primary,
        window_state_v1.into_current(&WindowKey::Primary),
    )]))
}

fn decode_v1(contents: &str) -> Option<HashMap<WindowKey, PersistedWindowState>> {
    let persisted_state_v1 = from_str::<PersistedStateV1>(contents).ok()?;
    if persisted_state_v1.version != PERSISTED_STATE_VERSION_V1 {
        warn!(
            "[decode] Invalid v1 persisted state version {}",
            persisted_state_v1.version
        );
        return None;
    }

    let mut states = HashMap::with_capacity(persisted_state_v1.entries.len());
    for persisted_entry_v1 in persisted_state_v1.entries {
        let window_state = persisted_entry_v1
            .window_state
            .into_current(&persisted_entry_v1.window_key);
        if states
            .insert(persisted_entry_v1.window_key.clone(), window_state)
            .is_some()
        {
            warn!(
                "[decode] Invalid persisted state: duplicate key \"{}\"",
                persisted_entry_v1.window_key
            );
            return None;
        }
    }

    debug!("[decode] Migrated v1 state to v{CURRENT_STATE_VERSION}");
    Some(states)
}

fn decode_v2(contents: &str) -> Option<HashMap<WindowKey, PersistedWindowState>> {
    let persisted_state_v2 = from_str::<PersistedStateV2>(contents).ok()?;
    let mut states = HashMap::with_capacity(persisted_state_v2.entries.len());
    for persisted_entry_v2 in persisted_state_v2.entries {
        let window_state = persisted_entry_v2
            .window_state
            .into_current(&persisted_entry_v2.window_key);
        if states
            .insert(persisted_entry_v2.window_key.clone(), window_state)
            .is_some()
        {
            warn!(
                "[decode] Invalid persisted state: duplicate key \"{}\"",
                persisted_entry_v2.window_key
            );
            return None;
        }
    }

    debug!("[decode] Migrated v2 state to v{CURRENT_STATE_VERSION}");
    Some(states)
}

fn decode_v3(contents: &str) -> Option<HashMap<WindowKey, PersistedWindowState>> {
    let persisted_state = from_str::<PersistedState>(contents).ok()?;
    let mut states = HashMap::with_capacity(persisted_state.entries.len());
    for persisted_entry in persisted_state.entries {
        if states
            .insert(
                persisted_entry.window_key.clone(),
                persisted_entry.window_state,
            )
            .is_some()
        {
            warn!(
                "[decode] Invalid persisted state: duplicate key \"{}\"",
                persisted_entry.window_key
            );
            return None;
        }
    }

    Some(states)
}

/// Encode typed runtime state into the current persisted format.
pub(super) fn encode(states: &HashMap<WindowKey, PersistedWindowState>) -> Result<String, Error> {
    let mut entries: Vec<PersistedEntry> = states
        .iter()
        .map(|(key, window_state)| PersistedEntry {
            window_key:   key.clone(),
            window_state: window_state.clone(),
        })
        .collect();
    entries.sort_by(|a, b| a.window_key.cmp(&b.window_key));

    let persisted_state = PersistedState {
        version: CURRENT_STATE_VERSION,
        entries,
    };
    let ron_body = to_string_pretty(&persisted_state, PrettyConfig::default())?;
    Ok(format!("{RON_HEADER}{ron_body}"))
}

#[cfg(test)]
#[allow(clippy::panic, reason = "tests should panic on unexpected values")]
mod tests {
    use std::collections::HashMap;

    use bevy::prelude::*;
    use ron::from_str;
    use ron::ser::PrettyConfig;
    use ron::ser::to_string_pretty;

    use super::CURRENT_STATE_VERSION;
    use super::DEFAULT_SCALE_FACTOR;
    use super::PERSISTED_STATE_VERSION_V1;
    use super::PersistedEntry;
    use super::PersistedPosition;
    use super::PersistedState;
    use super::PersistedWindowState;
    use super::SavedVideoMode;
    use super::SavedWindowMode;
    use super::UnrebasedDesktopPosition;
    use super::WindowKey;
    use crate::monitors::PanelIdentity;
    use crate::persistence::format;

    /// Unwrap a migrated legacy coordinate, or fail naming what was found instead.
    fn unrebased(window_state: &PersistedWindowState) -> UnrebasedDesktopPosition {
        match window_state.position {
            PersistedPosition::Unrebased(unrebased) => unrebased,
            other => panic!("expected a migrated legacy coordinate, found {other:?}"),
        }
    }

    fn sample_state() -> PersistedWindowState {
        PersistedWindowState {
            monitor_panel:     PanelIdentity::Anonymous,
            position:          PersistedPosition::MonitorOffset(IVec2::new(10, 20)),
            logical_width:     800,
            logical_height:    600,
            monitor:           1,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name:          "test-app".to_string(),
        }
    }

    #[test]
    fn decode_v3_distinguishes_primary_and_managed_primary() {
        let persisted_state = PersistedState {
            version: CURRENT_STATE_VERSION,
            entries: vec![
                PersistedEntry {
                    window_key:   WindowKey::Primary,
                    window_state: sample_state(),
                },
                PersistedEntry {
                    window_key:   WindowKey::Managed("primary".to_string()),
                    window_state: PersistedWindowState {
                        position: PersistedPosition::MonitorOffset(IVec2::new(30, 40)),
                        ..sample_state()
                    },
                },
            ],
        };
        let contents = match to_string_pretty(&persisted_state, PrettyConfig::default()) {
            Ok(contents) => contents,
            Err(error) => panic!("failed to serialize test state: {error}"),
        };

        let decoded = format::decode(&contents);
        assert!(decoded.is_some(), "expected v3 decode to succeed");
        let decoded = decoded.unwrap_or_default();
        assert!(decoded.contains_key(&WindowKey::Primary));
        assert!(decoded.contains_key(&WindowKey::Managed("primary".to_string())));
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn decode_legacy_single_window_migrates_to_current() {
        // Legacy format uses `width`/`height` field names (pre-multi-window era)
        let legacy_ron = "\
(
    position: Some((10, 20)),
    width: 800,
    height: 600,
    monitor_index: 1,
    mode: Windowed,
    app_name: \"test-app\",
)";

        let decoded = format::decode(legacy_ron);
        assert!(
            decoded.is_some(),
            "expected legacy single-window decode to succeed"
        );
        let decoded = decoded.unwrap_or_default();
        assert!(decoded.contains_key(&WindowKey::Primary));
        assert_eq!(decoded.len(), 1);
        let window_state = &decoded[&WindowKey::Primary];
        // Preserved for rebasing rather than converted: without a live monitor layout there is
        // nothing to measure an offset against.
        let position = unrebased(window_state);
        assert_eq!(position.logical(), IVec2::new(10, 20));
        assert!((position.captured_scale() - DEFAULT_SCALE_FACTOR).abs() < f64::EPSILON);
        assert_eq!(window_state.logical_width, 800);
        assert_eq!(window_state.logical_height, 600);
    }

    #[test]
    fn decode_v1_migrates_to_current() {
        let v1_ron = format!(
            "\
(
    version: {PERSISTED_STATE_VERSION_V1},
    entries: [
        (
            key: Primary,
            state: (
                position: Some((10, 20)),
                width: 800,
                height: 600,
                monitor_index: 1,
                mode: Windowed,
                app_name: \"test-app\",
            ),
        ),
    ],
)",
        );

        let decoded = format::decode(&v1_ron);
        assert!(decoded.is_some(), "expected v1 decode to succeed");
        let decoded = decoded.unwrap_or_default();
        let window_state = &decoded[&WindowKey::Primary];
        assert_eq!(window_state.logical_width, 800);
        assert_eq!(window_state.logical_height, 600);
        let position = unrebased(window_state);
        assert_eq!(position.logical(), IVec2::new(10, 20));
        assert!((position.captured_scale() - DEFAULT_SCALE_FACTOR).abs() < f64::EPSILON);
    }

    /// A v2 file written by a shipped build must still decode after the version bump.
    ///
    /// The version is spelled out rather than read from a constant on purpose: this test exists
    /// to fail if a future bump lands without its decode arm. Reaching `decode` -> `None` makes
    /// `load` seed an empty state, and the first dirty frame then writes that empty state over
    /// whatever the user had saved.
    #[test]
    fn decode_v2_preserves_the_saved_coordinate_and_its_scale() {
        let v2_ron = "\
(
    version: 2,
    entries: [
        (
            key: Primary,
            state: (
                logical_position: Some((-6880, 0)),
                logical_width: 800,
                logical_height: 600,
                monitor_scale: 2.0,
                monitor_index: 1,
                mode: Windowed,
                app_name: \"test-app\",
            ),
        ),
    ],
)";
        let decoded = format::decode(v2_ron);
        assert!(decoded.is_some(), "expected v2 decode to succeed");
        let decoded = decoded.unwrap_or_default();
        let window_state = &decoded[&WindowKey::Primary];
        let position = unrebased(window_state);
        assert_eq!(position.logical(), IVec2::new(-6880, 0));
        assert!((position.captured_scale() - 2.0).abs() < f64::EPSILON);
        assert_eq!(window_state.monitor, 1);
        assert_eq!(window_state.logical_width, 800);
        assert_eq!(window_state.logical_height, 600);
    }

    /// A `monitor_scale` that would make the rebase divide by zero is dropped, not carried.
    #[test]
    fn decode_v2_drops_a_coordinate_whose_scale_is_unusable() {
        let v2_ron = "\
(
    version: 2,
    entries: [
        (
            key: Primary,
            state: (
                logical_position: Some((-6880, 0)),
                logical_width: 800,
                logical_height: 600,
                monitor_scale: 0.0,
                monitor_index: 1,
                mode: Windowed,
                app_name: \"test-app\",
            ),
        ),
    ],
)";
        let decoded = format::decode(v2_ron);
        assert!(
            decoded.is_some(),
            "a bad scale must not fail the whole file"
        );
        let decoded = decoded.unwrap_or_default();
        assert_eq!(
            decoded[&WindowKey::Primary].position,
            PersistedPosition::Unpositioned
        );
    }

    /// Serde ignores field privacy, so a derived `Deserialize` would otherwise be a second,
    /// unvalidated constructor for `UnrebasedDesktopPosition` — the one reading untrusted files.
    #[test]
    fn decode_v3_rejects_an_unrebased_entry_with_an_unusable_scale() {
        let v3_ron = format!(
            "\
(
    version: {CURRENT_STATE_VERSION},
    entries: [
        (
            key: Primary,
            state: (
                position: Unrebased((logical: (-6880, 0), captured_scale: 0.0)),
                logical_width: 800,
                logical_height: 600,
                monitor_index: 1,
                mode: Windowed,
                app_name: \"test-app\",
            ),
        ),
    ],
)"
        );

        assert!(
            format::decode(&v3_ron).is_none(),
            "an unusable captured_scale must not survive v3 deserialization"
        );
    }

    #[test]
    fn decode_v3_rejects_duplicate_keys() {
        let persisted_state = PersistedState {
            version: CURRENT_STATE_VERSION,
            entries: vec![
                PersistedEntry {
                    window_key:   WindowKey::Primary,
                    window_state: sample_state(),
                },
                PersistedEntry {
                    window_key:   WindowKey::Primary,
                    window_state: sample_state(),
                },
            ],
        };
        let contents = match to_string_pretty(&persisted_state, PrettyConfig::default()) {
            Ok(contents) => contents,
            Err(error) => panic!("failed to serialize duplicate-key test state: {error}"),
        };

        assert!(
            format::decode(&contents).is_none(),
            "duplicate keys should fail decode"
        );
    }

    /// Golden-file tests using exact RON strings from the pre-multi-window era
    /// (commit 516f5930, used through v0.18.2). These are byte-for-byte copies of
    /// files that the published crate wrote via `ron::ser::to_string_pretty` with
    /// `PrettyConfig::default()`. If a dependency bump or struct change silently
    /// breaks deserialization, these tests catch it.
    mod golden_legacy {
        use super::*;

        /// Bare `PersistedWindowState` — windowed mode, from `macos_0/same_monitor_restore.ron`.
        const WINDOWED: &str = "\
(
    position: Some((200, 200)),
    width: 1600,
    height: 1200,
    monitor_index: 0,
    mode: Windowed,
    app_name: \"restore_window\",
)";

        /// Bare `PersistedWindowState` — borderless fullscreen, from
        /// `macos_0/fullscreen_borderless_programmatic.ron`.
        const BORDERLESS_FULLSCREEN: &str = "\
(
    position: Some((0, 0)),
    width: 3456,
    height: 2234,
    monitor_index: 0,
    mode: BorderlessFullscreen,
    app_name: \"restore_window\",
)";

        /// Bare `PersistedWindowState` — exclusive fullscreen with explicit video mode,
        /// from `macos_0/fullscreen_exclusive.ron`.
        const EXCLUSIVE_FULLSCREEN: &str = "\
(
    position: Some((0, 0)),
    width: 1920,
    height: 1200,
    monitor_index: 0,
    mode: Fullscreen(
        video_mode: Some((
            physical_size: (1920, 1200),
            bit_depth: 32,
            refresh_rate_millihertz: 60000,
        )),
    ),
    app_name: \"restore_window\",
)";

        #[test]
        fn decode_golden_legacy_windowed() {
            let decoded = format::decode(WINDOWED);
            assert!(decoded.is_some(), "golden legacy windowed file must decode");
            let decoded = decoded.unwrap_or_default();
            assert_eq!(decoded.len(), 1);
            let window_state = &decoded[&WindowKey::Primary];
            let position = unrebased(window_state);
            assert_eq!(position.logical(), IVec2::new(200, 200));
            assert!((position.captured_scale() - DEFAULT_SCALE_FACTOR).abs() < f64::EPSILON);
            assert_eq!(window_state.logical_width, 1600);
            assert_eq!(window_state.logical_height, 1200);
            assert_eq!(window_state.monitor, 0);
            assert_eq!(window_state.saved_window_mode, SavedWindowMode::Windowed);
            assert_eq!(window_state.app_name, "restore_window");
        }

        #[test]
        fn decode_golden_legacy_borderless_fullscreen() {
            let decoded = format::decode(BORDERLESS_FULLSCREEN);
            assert!(
                decoded.is_some(),
                "golden legacy borderless fullscreen file must decode"
            );
            let decoded = decoded.unwrap_or_default();
            let window_state = &decoded[&WindowKey::Primary];
            assert_eq!(unrebased(window_state).logical(), IVec2::ZERO);
            assert_eq!(window_state.logical_width, 3456);
            assert_eq!(window_state.logical_height, 2234);
            assert_eq!(
                window_state.saved_window_mode,
                SavedWindowMode::BorderlessFullscreen
            );
        }

        #[test]
        fn decode_golden_legacy_exclusive_fullscreen() {
            let decoded = format::decode(EXCLUSIVE_FULLSCREEN);
            assert!(
                decoded.is_some(),
                "golden legacy exclusive fullscreen file must decode"
            );
            let decoded = decoded.unwrap_or_default();
            let window_state = &decoded[&WindowKey::Primary];
            assert_eq!(unrebased(window_state).logical(), IVec2::ZERO);
            assert_eq!(window_state.logical_width, 1920);
            assert_eq!(window_state.logical_height, 1200);
            assert_eq!(
                window_state.saved_window_mode,
                SavedWindowMode::Fullscreen {
                    video_mode: Some(SavedVideoMode {
                        physical_size:           UVec2::new(1920, 1200),
                        bit_depth:               32,
                        refresh_rate_millihertz: 60000,
                    }),
                }
            );
        }
    }

    #[test]
    fn encode_sets_the_current_version() {
        let states = HashMap::from([
            (WindowKey::Primary, sample_state()),
            (WindowKey::Managed("inspector".to_string()), sample_state()),
        ]);

        let encoded = match format::encode(&states) {
            Ok(encoded) => encoded,
            Err(error) => panic!("failed to encode state: {error}"),
        };
        let decoded = from_str::<PersistedState>(&encoded);
        assert!(
            decoded.is_ok(),
            "encoded text should parse as the current version"
        );
        let decoded = decoded.unwrap_or(PersistedState {
            version: 0,
            entries: Vec::new(),
        });
        assert_eq!(decoded.version, CURRENT_STATE_VERSION);
        assert_eq!(decoded.entries.len(), 2);
    }

    #[test]
    fn encode_then_decode_roundtrip() {
        let states = HashMap::from([
            (WindowKey::Primary, sample_state()),
            (
                WindowKey::Managed("inspector".to_string()),
                PersistedWindowState {
                    monitor_panel:     PanelIdentity::Anonymous,
                    position:          PersistedPosition::MonitorOffset(IVec2::new(100, 200)),
                    logical_width:     1024,
                    logical_height:    768,
                    monitor:           0,
                    saved_window_mode: SavedWindowMode::Windowed,
                    app_name:          "test-app".to_string(),
                },
            ),
        ]);

        let encoded = match format::encode(&states) {
            Ok(encoded) => encoded,
            Err(error) => panic!("failed to encode state: {error}"),
        };
        let decoded = format::decode(&encoded);
        assert!(decoded.is_some(), "roundtrip decode should succeed");
        let decoded = decoded.unwrap_or_default();
        assert_eq!(decoded.len(), 2);
        let primary_window_state = &decoded[&WindowKey::Primary];
        assert_eq!(primary_window_state.logical_width, 800);
        assert_eq!(primary_window_state.logical_height, 600);
        // Offsets survive the round trip as offsets — encode never rewrites them as absolute
        // coordinates, so a saved position cannot regain a dependence on the desktop layout.
        assert_eq!(
            primary_window_state.position,
            PersistedPosition::MonitorOffset(IVec2::new(10, 20))
        );
        let inspector_window_state = &decoded[&WindowKey::Managed("inspector".to_string())];
        assert_eq!(inspector_window_state.logical_width, 1024);
        assert_eq!(inspector_window_state.logical_height, 768);
        assert_eq!(
            inspector_window_state.position,
            PersistedPosition::MonitorOffset(IVec2::new(100, 200))
        );
    }

    /// A migrated legacy entry stays migratable across a save: it round-trips through v3 with
    /// its scale intact, so a window that is never opened re-decides from the same pair on every
    /// launch instead of having one arbitrary layout's approximation frozen into its file.
    #[test]
    fn encode_then_decode_roundtrips_an_unrebased_entry() {
        let v2_ron = "\
(
    version: 2,
    entries: [
        (
            key: Primary,
            state: (
                logical_position: Some((-6880, 0)),
                logical_width: 800,
                logical_height: 600,
                monitor_scale: 2.0,
                monitor_index: 1,
                mode: Windowed,
                app_name: \"test-app\",
            ),
        ),
    ],
)";
        let migrated = format::decode(v2_ron).unwrap_or_default();
        let encoded = match format::encode(&migrated) {
            Ok(encoded) => encoded,
            Err(error) => panic!("failed to encode migrated state: {error}"),
        };

        let decoded = format::decode(&encoded);
        assert!(decoded.is_some(), "v3 must round-trip an Unrebased entry");
        let decoded = decoded.unwrap_or_default();
        let position = unrebased(&decoded[&WindowKey::Primary]);
        assert_eq!(position.logical(), IVec2::new(-6880, 0));
        assert!((position.captured_scale() - 2.0).abs() < f64::EPSILON);
    }
}

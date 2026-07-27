//! Saved window state types for persistence serialization.

#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]

use bevy::prelude::*;
use bevy::window::MonitorSelection;
use bevy::window::VideoMode;
use bevy::window::VideoModeSelection;
use bevy::window::WindowMode;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::constants::DEFAULT_SCALE_FACTOR;
use crate::monitors::PanelIdentity;

/// Saved video mode for exclusive fullscreen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub(crate) struct SavedVideoMode {
    pub(super) physical_size:           UVec2,
    pub(super) bit_depth:               u16,
    pub(super) refresh_rate_millihertz: u32,
}

impl SavedVideoMode {
    /// Convert to Bevy's `VideoMode`.
    #[must_use]
    const fn to_video_mode(&self) -> VideoMode {
        VideoMode {
            physical_size:           self.physical_size,
            bit_depth:               self.bit_depth,
            refresh_rate_millihertz: self.refresh_rate_millihertz,
        }
    }
}

/// Serializable window mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub(crate) enum SavedWindowMode {
    Windowed,
    BorderlessFullscreen,
    /// Exclusive fullscreen with optional specific video mode.
    Fullscreen {
        /// Video mode if explicitly set (`None` = use current display mode).
        video_mode: Option<SavedVideoMode>,
    },
}

impl SavedWindowMode {
    /// Convert to Bevy's `WindowMode` with the given monitor index.
    #[must_use]
    pub(crate) const fn to_window_mode(&self, monitor_index: usize) -> WindowMode {
        let monitor_selection = MonitorSelection::Index(monitor_index);
        match self {
            Self::Windowed => WindowMode::Windowed,
            Self::BorderlessFullscreen => WindowMode::BorderlessFullscreen(monitor_selection),
            Self::Fullscreen { video_mode: None } => {
                WindowMode::Fullscreen(monitor_selection, VideoModeSelection::Current)
            },
            Self::Fullscreen {
                video_mode: Some(saved),
            } => WindowMode::Fullscreen(
                monitor_selection,
                VideoModeSelection::Specific(saved.to_video_mode()),
            ),
        }
    }

    /// Check if this is a fullscreen mode (borderless or exclusive).
    #[must_use]
    pub(crate) const fn is_fullscreen(&self) -> bool { !matches!(self, Self::Windowed) }
}

impl From<&WindowMode> for SavedWindowMode {
    fn from(mode: &WindowMode) -> Self {
        match mode {
            WindowMode::Windowed => Self::Windowed,
            WindowMode::BorderlessFullscreen(_) => Self::BorderlessFullscreen,
            WindowMode::Fullscreen(_, video_mode_selection) => Self::Fullscreen {
                video_mode: match video_mode_selection {
                    VideoModeSelection::Current => None,
                    VideoModeSelection::Specific(mode) => Some(SavedVideoMode {
                        physical_size:           mode.physical_size,
                        bit_depth:               mode.bit_depth,
                        refresh_rate_millihertz: mode.refresh_rate_millihertz,
                    }),
                },
            },
        }
    }
}

/// Where a persisted window should be placed.
///
/// A monitor's logical origin is a function of the scale in force when a coordinate was
/// written, so an absolute logical desktop coordinate silently relocates the window when any
/// monitor's scale changes between save and restore. `MonitorOffset` is measured from the
/// monitor's own corner and is therefore scale-independent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) enum PersistedPosition {
    /// Logical-pixel offset of the window's top-left corner from its monitor's top-left corner.
    MonitorOffset(IVec2),
    /// A pre-v3 absolute desktop coordinate that has not been rebased onto a monitor yet.
    Unrebased(UnrebasedDesktopPosition),
    /// Nothing usable was saved: the platform withholds window position (Wayland), the window
    /// was compositor-placed, or a saved coordinate was rejected as no longer plausible.
    Unpositioned,
}

/// A pre-v3 absolute logical desktop coordinate paired with the monitor scale that wrote it.
///
/// The two are meaningless apart — reconstructing where the window actually sat requires the
/// scale in force at save time, not the live one. Kept in this form until a live monitor layout
/// is available to rebase against, and re-serialized unchanged until then so every launch
/// re-decides from the same pair instead of freezing one layout's approximation into the file.
///
/// Construction is confined to [`UnrebasedDesktopPosition::from_legacy`]: the fields are private,
/// so no other module can write the struct literal, and `Deserialize` is routed through
/// `UnrebasedWire` so a hand-edited or corrupt file cannot bypass validation either.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "UnrebasedWire", into = "UnrebasedWire")]
pub(crate) struct UnrebasedDesktopPosition {
    logical:        IVec2,
    captured_scale: f64,
}

impl UnrebasedDesktopPosition {
    /// Sole constructor. Rejects a `captured_scale` that is not finite and greater than zero.
    ///
    /// Every consumer divides or multiplies by this value, and `ToI32` saturates rather than
    /// failing: `0.0` yields `±inf`/`NaN` and lands the window at `i32::MIN` or the origin, and
    /// a negative scale mirrors the coordinate across the monitor corner.
    pub(super) fn from_legacy(logical: IVec2, captured_scale: f64) -> Option<Self> {
        (captured_scale.is_finite() && captured_scale > 0.0).then_some(Self {
            logical,
            captured_scale,
        })
    }

    /// Build a legacy coordinate from outside `persistence`, for tests that exercise the rebase.
    /// Routes through `from_legacy`, so it validates exactly as decode does.
    #[cfg(test)]
    pub(crate) fn from_test_legacy(logical: IVec2, captured_scale: f64) -> Option<Self> {
        Self::from_legacy(logical, captured_scale)
    }

    /// The absolute logical desktop coordinate as written.
    #[must_use]
    pub(crate) const fn logical(self) -> IVec2 { self.logical }

    /// The monitor scale in force when the coordinate was written.
    #[must_use]
    pub(crate) const fn captured_scale(self) -> f64 { self.captured_scale }
}

/// Wire form of [`UnrebasedDesktopPosition`]. Exists so the derived `Deserialize` cannot act as a
/// second, unvalidated constructor — serde ignores field privacy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct UnrebasedWire {
    logical:        IVec2,
    #[serde(default = "default_monitor_scale")]
    captured_scale: f64,
}

/// Rejected `captured_scale` from a persisted `Unrebased` entry.
#[derive(Debug, Error)]
#[error("persisted captured_scale {0} is not a finite number greater than zero")]
pub(crate) struct InvalidCapturedScale(f64);

impl TryFrom<UnrebasedWire> for UnrebasedDesktopPosition {
    type Error = InvalidCapturedScale;

    fn try_from(wire: UnrebasedWire) -> Result<Self, Self::Error> {
        Self::from_legacy(wire.logical, wire.captured_scale)
            .ok_or(InvalidCapturedScale(wire.captured_scale))
    }
}

impl From<UnrebasedDesktopPosition> for UnrebasedWire {
    fn from(unrebased: UnrebasedDesktopPosition) -> Self {
        Self {
            logical:        unrebased.logical,
            captured_scale: unrebased.captured_scale,
        }
    }
}

/// Saved window state persisted to the RON file.
///
/// Sizes are in **logical pixels** — they represent the user's visual intent and are independent
/// of scale factor. Restore converts them to physical pixels using the target monitor's live
/// scale. Position is scale-independent by construction; see [`PersistedPosition`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PersistedWindowState {
    /// Placement of the window relative to `monitor`.
    pub(crate) position:          PersistedPosition,
    /// Content area width in logical pixels (excludes window decoration).
    pub(crate) logical_width:     u32,
    /// Content area height in logical pixels (excludes window decoration).
    pub(crate) logical_height:    u32,
    /// Index of the monitor the position is measured from, and the fullscreen target.
    ///
    /// An index is only meaningful within one enumeration of the displays. It is the fallback,
    /// used when `monitor_fingerprint` is absent or matches nothing live.
    #[serde(rename = "monitor_index")]
    pub(crate) monitor:           usize,
    /// Whether the panel `monitor` referred to when this entry was written can be recognised.
    ///
    /// Under v1 and v2 a wrong monitor index still put the window at the saved *absolute*
    /// desktop position, so a renumbering cost nothing. A monitor-relative offset has no such
    /// safety net: anchored to the wrong panel it lands the window some distance into the wrong
    /// screen, and the next save records that as if it were correct. The fingerprint is what
    /// makes the index recoverable after a replug, a dock, or a driver update reorders displays.
    ///
    /// `Anonymous` for entries migrated from v1/v2, and for any display whose panel cannot be
    /// identified — Wayland, or a virtual display with synthetic or absent EDID. Those fall back
    /// to the index, which is what the format did before panel identities existed.
    #[serde(default, rename = "monitor_panel")]
    pub(crate) monitor_panel:     PanelIdentity,
    #[serde(rename = "mode")]
    pub(crate) saved_window_mode: SavedWindowMode,
    #[serde(default)]
    pub(crate) app_name:          String,
}

/// Default monitor scale for deserialization of legacy files missing the field.
pub(super) const fn default_monitor_scale() -> f64 { DEFAULT_SCALE_FACTOR }

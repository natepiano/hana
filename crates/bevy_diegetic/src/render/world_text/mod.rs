//! Shared text-entity components.
//!
//! Holds [`TextContent`] (the text-source component carried by every panel-text
//! label and by the one-element [`WorldText`](crate::WorldText) /
//! [`ScreenText`](crate::ScreenText) panels) and the [`PanelChild`] marker that
//! distinguishes a panel's text-child
//! labels from the panel root. The standalone world-text render path that once
//! lived here was removed when [`WorldText`](crate::WorldText) /
//! [`ScreenText`](crate::ScreenText) became one-element panels — all text now
//! routes through the panel-text pipeline.
//!
//! The readiness signal ([`WorldTextReady`] + the `AwaitingReady` gate) is shared
//! infrastructure the panel-text path drives, kept here.

mod readiness;

use bevy::prelude::*;
pub(crate) use readiness::AwaitingReady;
pub use readiness::WorldTextReady;
pub(crate) use readiness::emit_world_text_ready;

use crate::layout::WorldTextStyle;

/// Computed layout data for a [`TextContent`] entity, read by the typography
/// debug overlay to draw glyph bounding boxes and metric lines aligned with the
/// rendered text.
///
/// Only available when the `typography_overlay` feature is enabled.
///
/// TODO(overlay-retarget): nothing populates this since the standalone render
/// path was deleted (see `docs/bevy_diegetic/unify_text.md`, R8 SUPERSEDED). The
/// overlay reads it and is dark until a follow-up emits the computed run from the
/// panel-text path.
#[cfg(feature = "typography_overlay")]
#[derive(Component, Clone, Debug)]
pub struct ComputedWorldText {
    /// `Anchor` offset Y in layout units (matches the renderer's anchor).
    pub anchor_y: f32,
    /// Per-visible-glyph metrics aligned with the rendered text.
    pub glyphs:   Vec<ComputedGlyphMetrics>,
}

/// Overlay-only metrics for one visible glyph in a [`TextContent`] run.
#[cfg(feature = "typography_overlay")]
#[derive(Clone, Debug)]
pub struct ComputedGlyphMetrics {
    /// Ink bounding box `[x, y, width, height]` in world units.
    pub rect:      [f32; 4],
    /// Glyph origin X for overlay callouts, in world units.
    ///
    /// Usually the laid-out glyph origin. When a substituted glyph draws before
    /// that origin, as `JetBrains` Mono coding alternates do, this shifts left to
    /// the visible overhang so the origin/advance callout tracks the displayed
    /// glyph cell.
    pub origin_x:  f32,
    /// Glyph baseline origin Y in world units.
    pub origin_y:  f32,
    /// Laid-out horizontal advance in world units.
    pub advance_x: f32,
}

/// The text-source string for a text entity.
///
/// Carried by panel-text child labels and by the one-element panels that
/// [`WorldText`](crate::WorldText) / [`ScreenText`](crate::ScreenText) spawn —
/// querying such a panel's `TextContent` is how callers change the string at
/// runtime. Style is controlled by the required [`TextStyle`](crate::TextStyle)
/// component (added with defaults if not specified).
#[derive(Component, Clone, Debug, Reflect)]
#[require(WorldTextStyle, Transform, Visibility)]
pub struct TextContent {
    text: String,
}

impl TextContent {
    /// Creates new text content with the given string.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self { Self { text: text.into() } }

    /// Text contents.
    #[must_use]
    pub fn text(&self) -> &str { &self.text }

    /// Mutates the text contents.
    pub fn set_text(&mut self, text: impl Into<String>) { self.text = text.into(); }
}

/// Marker on a [`TextContent`] entity spawned as a child label of a
/// [`DiegeticPanel`](crate::DiegeticPanel).
///
/// Panel-text systems filter `With<PanelTextChild>` to act on label entities. A
/// panel root is `Without<PanelTextChild>` and is not rendered as text — including
/// the one-element [`WorldText`](crate::WorldText) / [`ScreenText`](crate::ScreenText)
/// panels, whose root carries [`TextContent`] only so callers can change the string
/// at runtime. The layout payload lives in
/// [`PanelTextLayout`](crate::render::panel_text::PanelTextLayout).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PanelTextChild;

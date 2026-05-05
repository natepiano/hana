//! Standalone world-space text component and rendering system.

mod mesh_spawning;
mod panel_text_child;
mod readiness;
mod rendering;
mod shaping;

use bevy::prelude::*;
pub use panel_text_child::PanelTextChild;
pub(super) use readiness::AwaitingReady;
pub use readiness::PendingGlyphs;
pub use readiness::WorldTextReady;
pub(super) use readiness::emit_world_text_ready;
pub(super) use rendering::render_world_text;

use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeTarget;
use crate::layout::Unit;
use crate::layout::WorldTextStyle;

/// Computed layout data for a [`WorldText`] entity, populated by the
/// renderer. Used by the typography debug overlay to draw glyph bounding
/// boxes and metric lines aligned with the rendered text.
///
/// Only available when the `typography_overlay` feature is enabled.
#[cfg(feature = "typography_overlay")]
#[derive(Component, Clone, Debug)]
pub struct ComputedWorldText {
    /// Anchor offset X in layout units (matches the renderer's anchor).
    pub anchor_x:      f32,
    /// Anchor offset Y in layout units (matches the renderer's anchor).
    pub anchor_y:      f32,
    /// Per-glyph ink bounding boxes `[x, y, width, height]` in world
    /// units. Derived from the font's glyph bbox, positioned using the
    /// same coordinate system as the renderer.
    pub glyph_rects:   Vec<[f32; 4]>,
    /// Advance width of the first glyph in world units.
    pub first_advance: f32,
}

/// Standalone MSDF text rendered in world space.
///
/// Attach to any entity with a [`Transform`] to place text in the 3D scene.
/// Style is controlled by the required [`TextStyle`] component (added
/// automatically with defaults if not specified).
///
/// ```ignore
/// commands.spawn((
///     WorldText::new("Hello, world!"),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
///
/// // With custom style:
/// commands.spawn((
///     WorldText::new("Styled"),
///     WorldTextStyle::new(24.0).with_color(Color::RED),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
/// ```
#[derive(Component, Clone, Debug, Reflect)]
#[require(WorldTextStyle, Transform, Visibility)]
pub struct WorldText(pub String);

impl WorldText {
    /// Creates a new world text with the given string.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self { Self(text.into()) }
}

/// Cascading attribute for standalone-world-text alpha mode.
///
/// 2-tier cascade: [`WorldTextStyle::alpha_mode`] (entity) →
/// [`CascadeDefaults::text_alpha`] (global). The resolved value is cached in
/// [`Resolved<WorldTextAlpha>`] on each standalone [`WorldText`] entity;
/// [`render_world_text`] reads it when spawning meshes.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(super) struct WorldTextAlpha(pub AlphaMode);

impl CascadeTarget for WorldTextAlpha {
    type Override = WorldTextStyle;

    fn override_value(entity_override: &WorldTextStyle) -> Option<Self> {
        entity_override.alpha_mode().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}

/// Cascading attribute for standalone-world-text font unit.
///
/// 2-tier cascade: [`WorldTextStyle::unit`] (entity) →
/// [`CascadeDefaults::world_font_unit`] (global). Resolved on each
/// standalone [`WorldText`] entity; readers multiply by
/// `meters_per_unit()` to convert font sizes into world-space scale.
/// A non-`None` [`WorldTextStyle::world_scale`] short-circuits this
/// cascade (it is a raw meters-per-unit override that bypasses the
/// [`Unit`] abstraction).
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct WorldFontUnit(pub Unit);

impl CascadeTarget for WorldFontUnit {
    type Override = WorldTextStyle;

    fn override_value(entity_override: &WorldTextStyle) -> Option<Self> {
        entity_override.unit().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.world_font_unit) }
}

//! Standalone world-space text component and rendering system.

mod mesh_spawning;
mod panel_text_child;
mod readiness;
mod rendering;
mod shaping;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use mesh_spawning::WorldTextMesh;
use mesh_spawning::WorldTextShadowProxy;
pub use panel_text_child::PanelTextChild;
pub(super) use readiness::AwaitingReady;
pub use readiness::PendingGlyphs;
pub use readiness::WorldTextReady;
pub(super) use readiness::emit_world_text_ready;

use super::text_shaping::TextShapingContext;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeTarget;
use crate::cascade::Resolved;
use crate::layout::ShapedTextCache;
use crate::layout::Unit;
use crate::layout::WorldTextStyle;
use crate::text::FontRegistry;
use crate::text::SlugBackend;
use crate::text::SlugTextMaterial;

pub(super) fn render_world_text(
    changed_texts: Query<
        Entity,
        (
            With<WorldText>,
            Without<PanelTextChild>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<Resolved<WorldTextAlpha>>,
                Changed<Resolved<WorldFontUnit>>,
            )>,
        ),
    >,
    pending_texts: Query<
        Entity,
        (
            With<WorldText>,
            With<PendingGlyphs>,
            Without<PanelTextChild>,
        ),
    >,
    texts: Query<(&WorldText, &WorldTextStyle), Without<PanelTextChild>>,
    resolved_alphas: Query<&Resolved<WorldTextAlpha>, Without<PanelTextChild>>,
    resolved_units: Query<&Resolved<WorldFontUnit>, Without<PanelTextChild>>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    cache: ResMut<ShapedTextCache>,
    meshes: ResMut<Assets<Mesh>>,
    backend_services: BackendRenderServices,
    defaults: Res<CascadeDefaults>,
    commands: Commands,
) {
    rendering::render_world_text(
        changed_texts,
        pending_texts,
        texts,
        resolved_alphas,
        resolved_units,
        old_meshes,
        font_registry,
        shaping_cx,
        cache,
        meshes,
        backend_services,
        defaults,
        commands,
    );
}

#[derive(SystemParam)]
pub(super) struct BackendRenderServices<'w> {
    slug_backend:    ResMut<'w, SlugBackend>,
    slug_materials:  ResMut<'w, Assets<SlugTextMaterial>>,
    storage_buffers: ResMut<'w, Assets<ShaderStorageBuffer>>,
}

/// Computed layout data for a [`WorldText`] entity, populated by the
/// renderer. Used by the typography debug overlay to draw glyph bounding
/// boxes and metric lines aligned with the rendered text.
///
/// Only available when the `typography_overlay` feature is enabled.
#[cfg(feature = "typography_overlay")]
#[derive(Component, Clone, Debug)]
pub struct ComputedWorldText {
    /// `Anchor` offset Y in layout units (matches the renderer's anchor).
    pub anchor_y: f32,
    /// Per-visible-glyph metrics aligned with the rendered text.
    pub glyphs:   Vec<ComputedGlyphMetrics>,
}

/// Overlay-only metrics for one visible glyph in a shaped [`WorldText`] run.
#[cfg(feature = "typography_overlay")]
#[derive(Clone, Debug)]
pub struct ComputedGlyphMetrics {
    /// Ink bounding box `[x, y, width, height]` in world units.
    pub rect:      [f32; 4],
    /// Glyph origin X for overlay callouts, in world units.
    ///
    /// Usually this is the shaped glyph origin. When a substituted glyph draws
    /// before that origin, as `JetBrains` Mono coding alternates do, this shifts
    /// left to the visible overhang so the origin/advance callout tracks the
    /// displayed glyph cell.
    pub origin_x:  f32,
    /// Glyph baseline origin Y in world units.
    pub origin_y:  f32,
    /// Shaped horizontal advance in world units.
    pub advance_x: f32,
}

/// Standalone text rendered in world space.
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
pub struct WorldText {
    text: String,
}

impl WorldText {
    /// Creates a new world text with the given string.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self { Self { text: text.into() } }

    /// Text contents.
    #[must_use]
    pub fn text(&self) -> &str { &self.text }

    /// Mutates the text contents.
    pub fn set_text(&mut self, text: impl Into<String>) { self.text = text.into(); }
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
    type Exclude = PanelTextChild;
    type Override = WorldTextStyle;

    fn override_value(entity_override: &WorldTextStyle) -> Option<Self> {
        entity_override.alpha_mode().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}

/// Cascading attribute for standalone-world-text font unit.
///
/// 2-tier cascade: [`WorldTextStyle::unit`] (entity) →
/// [`CascadeDefaults::world_font_unit`] (global). The resolved value is cached
/// in [`Resolved<WorldFontUnit>`] on each standalone [`WorldText`] entity;
/// readers multiply by `meters_per_unit()` to convert font sizes into
/// world-space scale.
/// A non-`None` [`WorldTextStyle::world_scale`] short-circuits this
/// cascade (it is a raw meters-per-unit override that bypasses the
/// [`Unit`] abstraction).
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(crate) struct WorldFontUnit(pub Unit);

impl CascadeTarget for WorldFontUnit {
    type Exclude = PanelTextChild;
    type Override = WorldTextStyle;

    fn override_value(entity_override: &WorldTextStyle) -> Option<Self> {
        entity_override.unit().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.world_font_unit) }
}

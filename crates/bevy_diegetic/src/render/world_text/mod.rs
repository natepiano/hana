//! Standalone world-space text component and rendering system.

mod mesh_spawning;
mod readiness;
mod rendering;
mod shaping;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use mesh_spawning::WorldTextMesh;
pub(super) use readiness::AwaitingReady;
pub use readiness::PendingGlyphs;
pub use readiness::WorldTextReady;
pub(super) use readiness::emit_world_text_ready;

use super::text_shaping::TextShapingContext;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeTarget;
use crate::cascade::FontUnit;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
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
            Without<PanelChild>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<Override<TextAlpha>>,
                Changed<Override<FontUnit>>,
                Changed<Resolved<WorldTextAlpha>>,
                Changed<Resolved<WorldFontUnit>>,
            )>,
        ),
    >,
    pending_texts: Query<Entity, (With<WorldText>, With<PendingGlyphs>, Without<PanelChild>)>,
    texts: Query<(&WorldText, &WorldTextStyle), Without<PanelChild>>,
    text_alpha_overrides: Query<&Override<TextAlpha>, Without<PanelChild>>,
    font_unit_overrides: Query<&Override<FontUnit>, Without<PanelChild>>,
    resolved_alphas: Query<&Resolved<WorldTextAlpha>, Without<PanelChild>>,
    resolved_units: Query<&Resolved<WorldFontUnit>, Without<PanelChild>>,
    old_meshes: Query<(Entity, &ChildOf), With<WorldTextMesh>>,
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
        text_alpha_overrides,
        font_unit_overrides,
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
    backend:         ResMut<'w, SlugBackend>,
    materials:       ResMut<'w, Assets<SlugTextMaterial>>,
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

/// Marker on a [`WorldText`] entity spawned as a child of a
/// [`DiegeticPanel`](crate::DiegeticPanel).
///
/// Standalone-text systems filter `Without<PanelChild>` to skip panel labels
/// (the panel-text systems render those); panel-text systems filter
/// `With<PanelChild>`. The layout payload lives in
/// [`PanelTextLayout`](crate::render::panel_text::PanelTextLayout).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PanelChild;

/// Cascading attribute for standalone-world-text alpha mode.
///
/// 2-tier cascade: [`Override<TextAlpha>`] (entity) →
/// [`CascadeDefaults::text_alpha`] (global). The override is seeded at spawn by
/// [`seed_world_text_overrides`] from [`WorldTextStyle::alpha_mode`]. The
/// resolved value is cached in [`Resolved<WorldTextAlpha>`] on each standalone
/// [`WorldText`] entity; [`render_world_text`] reads it when spawning meshes.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(super) struct WorldTextAlpha(pub AlphaMode);

impl CascadeTarget for WorldTextAlpha {
    type Exclude = PanelChild;
    type Override = Override<TextAlpha>;

    fn override_value(entity_override: &Override<TextAlpha>) -> Option<Self> {
        Some(Self(entity_override.0.0))
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}

/// Cascading attribute for standalone-world-text font unit.
///
/// 2-tier cascade: [`Override<FontUnit>`] (entity) →
/// [`CascadeDefaults::world_font_unit`] (global). The override is seeded at
/// spawn by [`seed_world_text_overrides`] from [`WorldTextStyle::unit`]. The
/// resolved value is cached in [`Resolved<WorldFontUnit>`] on each standalone
/// [`WorldText`] entity; readers multiply by `meters_per_unit()` to convert
/// font sizes into world-space scale.
/// A non-`None` [`WorldTextStyle::world_scale`] short-circuits this
/// cascade (it is a raw meters-per-unit override that bypasses the
/// [`Unit`] abstraction).
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(crate) struct WorldFontUnit(pub Unit);

impl CascadeTarget for WorldFontUnit {
    type Exclude = PanelChild;
    type Override = Override<FontUnit>;

    fn override_value(entity_override: &Override<FontUnit>) -> Option<Self> {
        Some(Self(entity_override.0.0))
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.world_font_unit) }
}

/// Spawn-time authoring bridge for standalone world-text cascade overrides.
///
/// Reads a newly-added [`WorldTextStyle`]'s `unit` / `alpha_mode` authoring
/// fields and inserts the matching `Override<A>` cascade component, but only
/// when the field is set. An absent field leaves the component absent, which
/// the cascade reads as "inherit." `WorldTextStyle` is no longer a cascade
/// source — the cascade reads `Override<A>`; the fields are authoring inputs
/// the bridge consumes once at spawn. The standalone twin of the panel's
/// `seed_panel_overrides` bridge.
///
/// Filters `Without<PanelChild>`: panel labels share the [`WorldTextStyle`]
/// component but inherit their cascade values from the panel subtree, so they
/// must not carry their own standalone-seeded `Override<A>`.
pub(super) fn seed_world_text_overrides(
    trigger: On<Add, WorldTextStyle>,
    styles: Query<&WorldTextStyle, Without<PanelChild>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(style) = styles.get(entity) else {
        return;
    };
    let mut entity_commands = commands.entity(entity);
    if let Some(unit) = style.unit() {
        entity_commands.insert(Override(FontUnit(unit)));
    }
    if let Some(alpha_mode) = style.alpha_mode() {
        entity_commands.insert(Override(TextAlpha(alpha_mode)));
    }
}

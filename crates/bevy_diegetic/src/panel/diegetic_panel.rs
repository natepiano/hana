//! [`DiegeticPanel`] — the main panel component and its computed companion.

use std::collections::HashMap;

use bevy::prelude::*;

use super::builder::DiegeticPanelBuilder;
use super::builder::NeedsSize;
use super::builder::Screen;
use super::builder::World;
use super::coordinate_space::CoordinateSpace;
use super::coordinate_space::ScreenPosition;
use super::coordinate_space::SurfaceShadow;
use super::events::LastPanelDimensions;
use super::field::PanelFieldRecord;
use crate::cascade;
use crate::cascade::CascadeDefaults;
use crate::cascade::FontUnit;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::GlyphLighting;
use crate::layout::GlyphSidedness;
use crate::layout::InvalidSize;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::LayoutTreeChange;
use crate::layout::PanelSize;
use crate::layout::TextStyle;
use crate::layout::Unit;

/// A diegetic UI panel attached to a 3D entity.
///
/// Defines a layout tree and the panel's dimensions in layout units.
/// World-space size is computed automatically from the panel's
/// `layout_unit`. Font sizes in the tree are interpreted in `font_unit`
/// (defaults through [`CascadeDefaults::panel_font_unit`]).
///
/// Construct via [`DiegeticPanel::world`] or [`DiegeticPanel::screen`]:
///
/// ```ignore
/// commands.spawn((
///     DiegeticPanel::world()
///         .size(Mm(210.0), Mm(297.0))
///         .world_height(0.5)
///         .layout(|b| {
///             b.text("Hello", TextStyle::new(48.0));
///         })
///         .build()?,
///     Transform::from_xyz(0.0, 0.0, 0.0),
/// ));
/// ```
///
/// The layout engine runs automatically when this component changes,
/// storing results in [`ComputedDiegeticPanel`].
///
/// Requires a [`Transform`] for world-space positioning.
#[derive(Clone, Component, Reflect)]
#[reflect(Component)]
#[require(
    ComputedDiegeticPanel,
    DiegeticPanelChangeClassification,
    LastPanelDimensions,
    ScaledLayoutTreeCache,
    Transform,
    Visibility
)]
pub struct DiegeticPanel {
    /// The layout tree defining this panel's UI structure.
    #[reflect(ignore)]
    pub(super) tree:             LayoutTree,
    /// Monotonic revision for cache invalidation when [`Self::tree`] is replaced.
    #[reflect(ignore)]
    pub(super) tree_revision:    u64,
    /// Panel width in layout `Unit`s. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    pub(super) width:            f32,
    /// Panel height in layout `Unit`s. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    pub(super) height:           f32,
    /// Unit for `width`/`height`. Set automatically by
    /// [`DiegeticPanelBuilder::size`] or [`set_size`](Self::set_size).
    pub(super) layout_unit:      Unit,
    /// Unit for font sizes in the layout tree. `None` inherits from
    /// [`CascadeDefaults::panel_font_unit`](crate::CascadeDefaults) via the
    /// cascade framework.
    pub(super) font_unit:        Option<Unit>,
    /// Which point on the panel sits at the entity's [`Transform`] position.
    /// Defaults to [`Anchor::TopLeft`].
    pub(super) anchor:           Anchor,
    /// Target world width in meters. When set, the panel is uniformly scaled
    /// so its width matches this value (height follows aspect ratio).
    /// If both `world_width` and `world_height` are set, non-uniform scaling
    /// is applied.
    pub(super) world_width:      Option<f32>,
    /// Target world height in meters. When set, the panel is uniformly scaled
    /// so its height matches this value (width follows aspect ratio).
    pub(super) world_height:     Option<f32>,
    /// Whether the panel surface casts 3D shadows. Defaults to [`SurfaceShadow::Off`].
    /// Text shadow casting is controlled per-element via `GlyphShadowMode`.
    pub(super) surface_shadow:   SurfaceShadow,
    /// Default PBR material for backgrounds and borders. When `None`, the
    /// library uses a matte default (roughness 0.95, reflectance 0.02).
    /// Individual elements can override via `El::material`.
    /// `base_color` is overridden by the layout color when both are set.
    #[reflect(ignore)]
    pub(super) material:         Option<StandardMaterial>,
    /// Default PBR material for text. When `None`, uses the same default as
    /// `material`. Individual text elements can override.
    /// `base_color` is overridden by `TextStyle::color` when set.
    #[reflect(ignore)]
    pub(super) text_material:    Option<StandardMaterial>,
    /// Panel-level override for text [`AlphaMode`]. When `None`, the resolution
    /// falls through to the per-style setting and then to
    /// `CascadeDefault<TextAlpha>`.
    pub(super) text_alpha_mode:  Option<AlphaMode>,
    /// Whether the panel is world-space or screen-space.
    pub(super) coordinate_space: CoordinateSpace,
    /// Maps each text run's [`PanelFieldId`](crate::PanelFieldId) to the entity
    /// reconcile materialized for it, so
    /// [`text_child`](Self::text_child) resolves a named run in O(1).
    ///
    /// `reconcile_panel_text_children` rebuilds this from scratch every pass and
    /// writes it without tripping change detection, so it never re-triggers
    /// layout; [`set_tree`](DiegeticPanelCommands::set_tree) clears it so a stale
    /// id stops resolving immediately.
    #[reflect(ignore)]
    pub(crate) text_index:       HashMap<crate::PanelFieldId, Entity>,
}

impl Default for DiegeticPanel {
    fn default() -> Self {
        Self {
            tree:             LayoutTree::default(),
            tree_revision:    0,
            width:            0.0,
            height:           0.0,
            layout_unit:      Unit::Meters,
            font_unit:        None,
            anchor:           Anchor::TopLeft,
            world_width:      None,
            world_height:     None,
            surface_shadow:   SurfaceShadow::Off,
            material:         None,
            text_material:    None,
            text_alpha_mode:  None,
            coordinate_space: CoordinateSpace::default(),
            text_index:       HashMap::new(),
        }
    }
}

// ── Public read-only accessors ──────────────────────────────────────────────

impl DiegeticPanel {
    /// Returns a reference to the layout tree.
    #[must_use]
    pub const fn tree(&self) -> &LayoutTree { &self.tree }

    /// Revision of the current layout tree.
    #[must_use]
    pub(crate) const fn tree_revision(&self) -> u64 { self.tree_revision }

    /// Panel width in layout units.
    #[must_use]
    pub const fn width(&self) -> f32 { self.width }

    /// Panel height in layout units.
    #[must_use]
    pub const fn height(&self) -> f32 { self.height }

    /// The layout unit for this panel's dimensions.
    #[must_use]
    pub const fn layout_unit(&self) -> Unit { self.layout_unit }

    /// The font unit override, or `None` if inheriting from
    /// [`CascadeDefaults::panel_font_unit`](crate::CascadeDefaults).
    #[must_use]
    pub const fn font_unit(&self) -> Option<Unit> { self.font_unit }

    /// The panel's anchor point.
    #[must_use]
    pub const fn anchor(&self) -> Anchor { self.anchor }

    /// Whether the panel surface casts shadows.
    #[must_use]
    pub const fn surface_shadow(&self) -> SurfaceShadow { self.surface_shadow }

    /// The default panel material, if set.
    #[must_use]
    pub const fn material(&self) -> Option<&StandardMaterial> { self.material.as_ref() }

    /// Mutable access to the default panel material.
    pub const fn material_mut(&mut self) -> &mut Option<StandardMaterial> { &mut self.material }

    /// The default text material, if set.
    #[must_use]
    pub const fn text_material(&self) -> Option<&StandardMaterial> { self.text_material.as_ref() }

    /// Mutable access to the default text material.
    pub const fn text_material_mut(&mut self) -> &mut Option<StandardMaterial> {
        &mut self.text_material
    }

    /// The panel-level text [`AlphaMode`] override, if set.
    #[must_use]
    pub const fn text_alpha_mode(&self) -> Option<AlphaMode> { self.text_alpha_mode }

    /// The panel's coordinate space (world or screen).
    #[must_use]
    pub const fn coordinate_space(&self) -> &CoordinateSpace { &self.coordinate_space }

    /// Resolves a text run's [`PanelFieldId`](crate::PanelFieldId) to the entity
    /// reconcile materialized for it (its `line_index == 0` child), or `None` if
    /// no run carries that id.
    ///
    /// This is an **unchecked** index read: it returns the stored `Entity` as-is.
    /// The method takes `&self` with no `World`/`Entities` access, so it cannot
    /// confirm the entity is still alive — an out-of-flow `despawn` would leave a
    /// stale mapping until the next reconcile rebuilds the index. Liveness is
    /// validated one layer up by the [`PanelText`](crate::PanelText) `SystemParam`,
    /// whose `Query::get` on the returned entity yields `None` for a dead child.
    ///
    /// A `set_tree` in the same frame clears the index immediately, so a lookup
    /// before the next reconcile pass returns `None`.
    #[must_use]
    pub fn text_child(&self, id: &crate::PanelFieldId) -> Option<Entity> {
        self.text_index.get(id).copied()
    }
}

// ── Public mutators ─────────────────────────────────────────────────────────

impl DiegeticPanel {
    /// Atomically updates the panel's width, height, and layout unit.
    ///
    /// This is the preferred way to resize a panel at runtime (e.g. for
    /// animation) because it keeps dimensions and unit in sync.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSize`] if either dimension is zero or negative.
    pub fn set_size(&mut self, size: impl PanelSize) -> Result<(), InvalidSize> {
        let (w, h, unit) = size.dimensions();
        if w <= 0.0 || h <= 0.0 {
            return Err(InvalidSize {
                width:  w,
                height: h,
            });
        }
        self.width = w;
        self.height = h;
        self.layout_unit = unit;
        Ok(())
    }

    /// Replaces the layout tree and takes the conservative full-layout path.
    ///
    /// For optimized visual-only updates, prefer
    /// [`DiegeticPanelCommands::set_tree`]. A direct component
    /// method cannot update the sibling change-classification component.
    #[cfg(feature = "bench_support")]
    #[doc(hidden)]
    pub fn set_tree_full_rebuild(&mut self, tree: LayoutTree) {
        self.tree = tree;
        self.tree_revision = self.tree_revision.wrapping_add(1);
    }

    /// Writes a run-text edit into the authoritative `El.text` tree and bumps the
    /// tree revision so [`ScaledLayoutTreeCache`] rebuilds with the new string.
    /// Returns whether the cache changed.
    ///
    /// `El.text` is the single source for run text; the child
    /// [`TextContent`](crate::TextContent) is derived output reconcile rewrites
    /// from the tree. The public edit path (`PanelText` / `DiegeticTextMut` via
    /// `TextEdit`) calls this to change a run's string. Skips the revision bump
    /// (and so the layout) when the string already matches, which also keeps the
    /// measurer off an unchanged cached string.
    pub(crate) fn sync_run_text_cache(&mut self, index: usize, text: &str) -> bool {
        if self.tree.set_element_text(index, text) {
            self.tree_revision = self.tree_revision.wrapping_add(1);
            true
        } else {
            false
        }
    }

    /// Writes a restyle into the authored `El.config` and bumps the tree
    /// revision so [`ScaledLayoutTreeCache`] rebuilds and the layout engine
    /// re-measures with the new style. Returns whether the style changed.
    ///
    /// Like run text (see [`sync_run_text_cache`](Self::sync_run_text_cache)),
    /// the tree is the single authoritative source: `El.config` for style,
    /// `El.text` for the string, while the run child is derived output reconcile
    /// rewrites. A label restyle (font, size) mutates `El.config` through this
    /// method; the relayout it triggers flows the new config to the run via
    /// reconcile, so measurement and rendering stay on the same source. Skips the
    /// revision bump (and so the layout) when the style already matches.
    pub(crate) fn restyle_run(&mut self, index: usize, style: TextStyle) -> bool {
        if self.tree.set_element_style(index, style) {
            self.tree_revision = self.tree_revision.wrapping_add(1);
            true
        } else {
            false
        }
    }

    /// Sets the panel width directly (in layout units).
    ///
    /// Used by the screen-space positioning system to resize panels
    /// whose dimensions are derived from the window.
    pub const fn set_width(&mut self, width: f32) { self.width = width; }

    /// Sets the panel height directly (in layout units).
    pub const fn set_height(&mut self, height: f32) { self.height = height; }
}

// ── Builder entry points ────────────────────────────────────────────────────

impl DiegeticPanel {
    /// Returns a builder for a world-space panel.
    ///
    /// Bare floats in `.size()` default to [`Unit::Meters`].
    #[must_use]
    pub fn world() -> DiegeticPanelBuilder<World, NeedsSize> { DiegeticPanelBuilder::new_world() }

    /// Returns a builder for a screen-space panel.
    ///
    /// Bare floats in `.size()` default to [`Unit::Pixels`].
    #[must_use]
    pub fn screen() -> DiegeticPanelBuilder<Screen, NeedsSize> {
        DiegeticPanelBuilder::new_screen()
    }
}

// ── Computation methods ─────────────────────────────────────────────────────

impl DiegeticPanel {
    /// Physical width in meters before world scaling.
    fn physical_width(&self) -> f32 { self.width * self.layout_unit.meters_per_unit() }

    /// Physical height in meters before world scaling.
    fn physical_height(&self) -> f32 { self.height * self.layout_unit.meters_per_unit() }

    /// Panel width in meters (world units), incorporating `world_width`
    /// and `world_height` scaling.
    ///
    /// - `world_width` set: returns `world_width`.
    /// - `world_height` only: uniform scale from height, width follows aspect ratio.
    /// - Neither: physical size from layout units.
    #[must_use]
    pub fn world_width(&self) -> f32 {
        let physical_width = self.physical_width();
        let physical_height = self.physical_height();
        match (self.world_width, self.world_height) {
            (Some(target_width), _) => target_width,
            (None, Some(target_height)) => {
                if physical_height > 0.0 {
                    physical_width * (target_height / physical_height)
                } else {
                    physical_width
                }
            },
            (None, None) => physical_width,
        }
    }

    /// Panel height in meters (world units), incorporating `world_width`
    /// and `world_height` scaling.
    ///
    /// - `world_height` set: returns `world_height`.
    /// - `world_width` only: uniform scale from width, height follows aspect ratio.
    /// - Neither: physical size from layout units.
    #[must_use]
    pub fn world_height(&self) -> f32 {
        let physical_width = self.physical_width();
        let physical_height = self.physical_height();
        match (self.world_width, self.world_height) {
            (_, Some(target_height)) => target_height,
            (Some(target_width), None) => {
                if physical_width > 0.0 {
                    physical_height * (target_width / physical_width)
                } else {
                    physical_height
                }
            },
            (None, None) => physical_height,
        }
    }

    /// Returns the (`x_offset`, `y_offset`) in world meters for converting
    /// layout coordinates (top-left origin, Y-down) to panel-local
    /// coordinates relative to the anchor point.
    ///
    /// Layout local position = `(layout_x * scale - x_offset, -layout_y * scale + y_offset)`
    /// where `scale` is `points_to_world()`.
    ///
    /// Screen panels render under an orthographic camera where 1 world
    /// unit = 1 logical pixel, so anchor offsets are computed directly
    /// from `panel.width`/`panel.height` (in layout units = pixels).
    /// This keeps anchor positioning correct for dynamic-sized screen
    /// panels (`Sizing::Fit` / `Sizing::Grow` / `Sizing::Percent`) whose
    /// `world_width` / `world_height` fields are left unset at build
    /// time.
    #[must_use]
    pub fn anchor_offsets(&self) -> (f32, f32) {
        let (fx, fy) = self.anchor.offset_fraction();
        if self.coordinate_space.is_screen() {
            return (self.width * fx, self.height * fy);
        }
        (self.world_width() * fx, self.world_height() * fy)
    }

    /// Conversion factor from layout points to world meters.
    ///
    /// The layout engine works in points internally. Multiply a layout-space
    /// value (in points) by this factor to get world meters. Incorporates
    /// `world_width`/`world_height` scaling.
    #[must_use]
    pub fn points_to_world(&self) -> f32 {
        // Screen panels render under an orthographic camera where 1 world
        // unit = 1 logical pixel. The layout engine scales dimensions by
        // `layout_unit.to_points()`; reversing that factor returns values
        // to the layout unit (which equals world units for screen panels),
        // independent of panel height. This keeps points_to_world stable
        // across dynamic `Sizing::Fit` / `Sizing::Grow` sizing where
        // `panel.height` is recomputed each frame.
        if self.coordinate_space.is_screen() {
            let to_pts = self.layout_unit.to_points();
            if to_pts > 0.0 {
                return 1.0 / to_pts;
            }
        }
        let viewport_points_height = self.height * self.layout_unit.to_points();
        let world_height = self.world_height();
        if viewport_points_height > 0.0 {
            world_height / viewport_points_height
        } else {
            Unit::Points.meters_per_unit()
        }
    }

    /// Font-to-layout conversion factor for this panel.
    ///
    /// Multiply a font size by this value to convert from font units
    /// to layout units. Callers pass the panel's resolved font unit —
    /// read from `Resolved<FontUnit>` on the panel
    /// entity (every panel carries one, seeded from `font_unit` or
    /// [`CascadeDefaults::panel_font_unit`]).
    #[must_use]
    pub fn font_scale(&self, panel_font_unit: Unit) -> f32 {
        let font_meters_per_unit = panel_font_unit.meters_per_unit();
        let layout_meters_per_unit = self.layout_unit.meters_per_unit();
        font_meters_per_unit / layout_meters_per_unit
    }
}

/// Extension methods for mutating diegetic panels through [`Commands`].
///
/// This trait is an ergonomic wrapper around Bevy's
/// [`Commands::run_system_cached_with`]. Replacing a tree has two effects:
/// it stores the new tree on [`DiegeticPanel`], and it records a pending
/// `Identical` / `VisualOnly` / `LayoutAffecting` decision for the later layout
/// system to consume. That pending decision is stored on an internal sibling
/// component, not on [`DiegeticPanel`] itself, so a plain `&mut DiegeticPanel`
/// method would update the tree but lose the information needed to skip layout.
/// Keeping the wrapper here lets callers use a focused panel API instead of
/// exposing the cached-system function and its tuple input structure as public
/// surface.
pub trait DiegeticPanelCommands {
    /// Queues a layout-tree replacement that records whether the change is
    /// visual-only or layout-affecting.
    ///
    /// The queued setter is deferred. Schedule systems that call this before
    /// panel layout systems when the update must be visible in the same frame.
    fn set_tree(&mut self, entity: Entity, tree: LayoutTree);
}

impl DiegeticPanelCommands for Commands<'_, '_> {
    fn set_tree(&mut self, entity: Entity, tree: LayoutTree) {
        self.run_system_cached_with(set_tree_command, (entity, tree));
    }
}

fn set_tree_command(
    In((entity, next_tree)): In<(Entity, LayoutTree)>,
    mut panels: Query<(&mut DiegeticPanel, &mut DiegeticPanelChangeClassification)>,
) {
    let Ok((mut panel, mut classification)) = panels.get_mut(entity) else {
        return;
    };
    let change = panel.tree.classify_change(&next_tree);
    classification.record(change);
    panel.tree = next_tree;
    panel.tree_revision = panel.tree_revision.wrapping_add(1);
    // Drop stale id → entity mappings so a lookup for a run the new tree no
    // longer contains returns `None` before reconcile rebuilds the index.
    panel.text_index.clear();
}

/// Spawn-time authoring bridge for panel cascade overrides.
///
/// Reads a newly-added [`DiegeticPanel`]'s `text_alpha_mode` / `font_unit`
/// authoring fields, inserts the matching `Override<A>` cascade components, and
/// seeds the panel's `Resolved<FontUnit>` (which `compute_panel_layouts` reads).
///
/// A panel is depth-1 with no cascade ancestor, so it cannot inherit `FontUnit`
/// from a parent — it therefore **always** carries `Override<FontUnit>`, from
/// `panel.font_unit()` when set else [`CascadeDefaults::panel_font_unit`]. That
/// is why `panel_font_unit` is a construction-time seed, not a cascade global:
/// no panel ever reads the `FontUnit` global, and a runtime `font_unit` change
/// does not reach existing panels. `text_alpha_mode` is the panel's optional
/// override for the alpha its labels inherit; absent → labels resolve to
/// `CascadeDefault<TextAlpha>` through the parent-walk. The panel needs no
/// `Resolved<TextAlpha>` of its own — only its labels render text, and they
/// walk up to this `Override<TextAlpha>` directly. The panel twin of the
/// standalone `TextStyle` authoring bridge.
pub(super) fn seed_panel_overrides(
    trigger: On<Add, DiegeticPanel>,
    panels: Query<&DiegeticPanel>,
    defaults: Res<CascadeDefaults>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(panel) = panels.get(entity) else {
        return;
    };
    let font_unit = panel.font_unit().unwrap_or(defaults.panel_font_unit);
    // Per-context glyph defaults: screen panels are unlit, front-facing HUD
    // surfaces; world panels stay lit and double-sided (the global cascade
    // default). A world panel whose text material is explicitly unlit also seeds
    // an unlit override so its labels inherit the look its material implied
    // before lighting moved onto the cascade. Labels still override per-label
    // via `TextStyle::with_lighting` / `with_sidedness` (captured in
    // `reconcile_panel_text_children`).
    let is_screen = panel.coordinate_space().is_screen();
    let material_unlit = panel.text_material().is_some_and(|material| material.unlit);
    let mut entity_commands = commands.entity(entity);
    entity_commands.insert(Resolved(FontUnit(font_unit)));
    cascade::apply_cascade_override(&mut entity_commands, FontUnit(font_unit));
    if let Some(alpha_mode) = panel.text_alpha_mode() {
        cascade::apply_cascade_override(&mut entity_commands, TextAlpha(alpha_mode));
    }
    if is_screen {
        cascade::apply_cascade_override(&mut entity_commands, TextLighting(GlyphLighting::Unlit));
        cascade::apply_cascade_override(
            &mut entity_commands,
            TextSidedness(GlyphSidedness::OneSided),
        );
    } else if material_unlit {
        cascade::apply_cascade_override(&mut entity_commands, TextLighting(GlyphLighting::Unlit));
    }
}

/// Per-frame tree-change classification consumed by the panel layout system.
#[derive(Component, Default)]
pub(crate) struct DiegeticPanelChangeClassification {
    pending: Option<LayoutTreeChange>,
}

impl DiegeticPanelChangeClassification {
    pub(super) fn record(&mut self, change: LayoutTreeChange) {
        self.pending = Some(match self.pending.take() {
            None => change,
            Some(prior) => prior.combine(change),
        });
    }

    /// Records a run-text edit as `VisualOnly`: a text change moves no other
    /// element, so `compute_panel_layouts` re-measures only the edited leaf and
    /// takes the geometry-stable skip when its box is unchanged. Combining with a
    /// same-frame `LayoutAffecting` change still resolves to a full solve.
    pub(crate) fn note_text_edit(&mut self) { self.record(LayoutTreeChange::VisualOnly); }

    pub(super) const fn take(&mut self) -> Option<LayoutTreeChange> { self.pending.take() }

    pub(super) const fn pending(&self) -> Option<LayoutTreeChange> { self.pending }
}

/// Cached point-scaled layout tree for one [`DiegeticPanel`].
///
/// The source [`LayoutTree`] remains owned by [`DiegeticPanel`]. This component
/// stores the derived tree used by the layout engine after layout and font units
/// are converted to points.
#[derive(Component, Default)]
pub(super) struct ScaledLayoutTreeCache {
    tree_revision:         TreeRevision,
    layout_to_points_bits: F32Bits,
    font_to_points_bits:   F32Bits,
    tree:                  Option<LayoutTree>,
    #[cfg(test)]
    hits:                  usize,
    #[cfg(test)]
    misses:                usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TreeRevision(u64);

impl From<u64> for TreeRevision {
    fn from(value: u64) -> Self { Self(value) }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct F32Bits(u32);

impl F32Bits {
    const fn new(value: f32) -> Self { Self(value.to_bits()) }
}

impl ScaledLayoutTreeCache {
    /// Returns a point-scaled tree, rebuilding the cache when the source tree
    /// or scale factors change.
    pub(super) fn get_or_update(
        &mut self,
        source: &LayoutTree,
        tree_revision: u64,
        layout_to_points: f32,
        font_to_points: f32,
    ) -> &LayoutTree {
        let tree_revision = TreeRevision::from(tree_revision);
        let layout_to_points_bits = F32Bits::new(layout_to_points);
        let font_to_points_bits = F32Bits::new(font_to_points);
        let cache_hit = self.tree.is_some()
            && self.tree_revision == tree_revision
            && self.layout_to_points_bits == layout_to_points_bits
            && self.font_to_points_bits == font_to_points_bits;

        if cache_hit {
            #[cfg(test)]
            {
                self.hits += 1;
            }
        } else {
            self.tree_revision = tree_revision;
            self.layout_to_points_bits = layout_to_points_bits;
            self.font_to_points_bits = font_to_points_bits;
            self.tree = None;
            #[cfg(test)]
            {
                self.misses += 1;
            }
        }

        self.tree
            .get_or_insert_with(|| source.scaled(layout_to_points, font_to_points))
    }

    #[cfg(test)]
    pub(super) const fn hits(&self) -> usize { self.hits }

    #[cfg(test)]
    pub(super) const fn misses(&self) -> usize { self.misses }
}

/// Computed layout result for a [`DiegeticPanel`].
///
/// Automatically added via required components when a [`DiegeticPanel`] is inserted.
/// Updated by the layout system whenever the panel changes.
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct ComputedDiegeticPanel {
    #[reflect(ignore)]
    result:             Option<LayoutResult>,
    #[reflect(ignore)]
    field_records:      Vec<PanelFieldRecord>,
    #[reflect(ignore)]
    field_id_conflicts: Vec<crate::PanelFieldId>,
    content_width:      f32,
    content_height:     f32,
}

impl ComputedDiegeticPanel {
    /// Actual computed content width in world units.
    ///
    /// This is the width of the first user-defined element (the content),
    /// not the full viewport. With `Sizing::FIT`, this shrinks to fit.
    #[must_use]
    pub const fn content_width(&self) -> f32 { self.content_width }

    /// Actual computed content height in world units.
    #[must_use]
    pub const fn content_height(&self) -> f32 { self.content_height }

    /// Returns the bounding box of the panel's content in layout units,
    /// or `None` if layout has not yet been computed.
    #[must_use]
    pub fn content_bounds(&self) -> Option<BoundingBox> {
        self.result.as_ref().and_then(LayoutResult::content_bounds)
    }

    /// Returns computed editable fields in draw-independent element order.
    #[must_use]
    pub fn field_records(&self) -> &[PanelFieldRecord] { &self.field_records }

    /// Returns duplicated editable field ids found during the latest layout.
    #[must_use]
    pub fn field_id_conflicts(&self) -> &[crate::PanelFieldId] { &self.field_id_conflicts }

    /// Resolves an editable field at a panel-local layout point.
    ///
    /// Records with duplicated ids are ignored because their semantic target
    /// is ambiguous.
    #[must_use]
    pub fn field_at_local_position(&self, panel_local: Vec2) -> Option<&PanelFieldRecord> {
        self.field_records
            .iter()
            .rev()
            .find(|record| !record.duplicate_id && record.contains(panel_local))
    }

    /// Returns the computed layout result, or `None` if not yet computed.
    #[must_use]
    pub const fn result(&self) -> Option<&LayoutResult> { self.result.as_ref() }

    /// Returns the computed layout result mutably, or `None` if not yet computed.
    pub(super) const fn result_mut(&mut self) -> Option<&mut LayoutResult> { self.result.as_mut() }

    /// Stores the computed layout result.
    pub fn set_result(&mut self, result: LayoutResult) {
        self.result = Some(result);
        self.field_records.clear();
        self.field_id_conflicts.clear();
    }

    pub(super) fn set_result_with_fields(
        &mut self,
        result: LayoutResult,
        field_records: Vec<PanelFieldRecord>,
        field_id_conflicts: Vec<crate::PanelFieldId>,
    ) {
        self.result = Some(result);
        self.field_records = field_records;
        self.field_id_conflicts = field_id_conflicts;
    }

    /// Sets the content dimensions in world units.
    pub const fn set_content_size(&mut self, width: f32, height: f32) {
        self.content_width = width;
        self.content_height = height;
    }
}

impl DiegeticPanel {
    /// Places this screen-space panel at an explicit pixel position.
    ///
    /// The origin is the window's top-left corner and y grows downward. The
    /// panel's [`Anchor`] determines which point of the panel is placed at this
    /// position. Returns `false` for world-space panels.
    #[must_use]
    pub const fn set_screen_position(&mut self, screen_position: Vec2) -> bool {
        let CoordinateSpace::Screen { position, .. } = &mut self.coordinate_space else {
            return false;
        };
        *position = ScreenPosition::At(screen_position);
        true
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "tests compare exact revision and cache counter values"
)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic if fixture panel construction fails"
)]
mod tests {
    use super::DiegeticPanel;
    use super::ScaledLayoutTreeCache;
    use crate::LayoutBuilder;
    use crate::TextStyle;

    fn test_tree(text: &str) -> crate::LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(text, TextStyle::new(10.0));
        builder.build()
    }

    #[test]
    fn scaled_tree_cache_hits_until_revision_or_scale_changes() {
        let tree = test_tree("cache");
        let mut cache = ScaledLayoutTreeCache::default();

        let _ = cache.get_or_update(&tree, 0, 2.0, 3.0);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 1);

        let _ = cache.get_or_update(&tree, 0, 2.0, 3.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);

        let _ = cache.get_or_update(&tree, 1, 2.0, 3.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 2);

        let _ = cache.get_or_update(&tree, 1, 4.0, 3.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 3);

        let _ = cache.get_or_update(&tree, 1, 4.0, 5.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 4);
    }

    #[cfg(feature = "bench_support")]
    #[test]
    fn tree_revision_changes_only_when_tree_is_replaced() {
        let mut panel = DiegeticPanel::default();
        assert_eq!(panel.tree_revision(), 0);

        panel.set_width(120.0);
        panel.set_height(80.0);
        assert_eq!(panel.tree_revision(), 0);

        let resize_result = panel.set_size((2.0, 1.0));
        assert!(resize_result.is_ok());
        assert_eq!(panel.tree_revision(), 0);

        panel.set_tree_full_rebuild(test_tree("next"));
        assert_eq!(panel.tree_revision(), 1);
    }

    #[test]
    fn builder_panels_start_at_tree_revision_zero() {
        let panel = DiegeticPanel::world()
            .size(1.0, 0.5)
            .with_tree(test_tree("builder"))
            .build()
            .expect("test panel should build");

        assert_eq!(panel.tree_revision(), 0);
    }
}

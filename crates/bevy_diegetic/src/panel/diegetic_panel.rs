//! [`DiegeticPanel`] — the main panel component and its computed companion.

use bevy::prelude::*;

use super::builder::DiegeticPanelBuilder;
use super::builder::NeedsSize;
use super::builder::Screen;
use super::builder::World;
use super::coordinate_space::CoordinateSpace;
use super::coordinate_space::RenderMode;
use super::coordinate_space::SurfaceShadow;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeTarget;
use crate::cascade::ExcludeNone;
use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::InvalidSize;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::LayoutTreeChange;
use crate::layout::PanelSize;
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
///             b.text("Hello", LayoutTextStyle::new(48.0));
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
#[derive(Component, Reflect)]
#[reflect(Component)]
#[require(
    ComputedDiegeticPanel,
    DiegeticPanelChangeClassification,
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
    /// How the panel renders its content. Defaults to [`RenderMode::Geometry`].
    pub(super) render_mode:      RenderMode,
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
    /// `base_color` is overridden by `LayoutTextStyle::color` when set.
    #[reflect(ignore)]
    pub(super) text_material:    Option<StandardMaterial>,
    /// Panel-level override for text [`AlphaMode`]. When `None`, the resolution
    /// falls through to the per-style setting and then to
    /// [`CascadeDefaults::text_alpha`](crate::CascadeDefaults).
    pub(super) text_alpha_mode:  Option<AlphaMode>,
    /// Whether the panel is world-space or screen-space.
    pub(super) coordinate_space: CoordinateSpace,
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
            render_mode:      RenderMode::Geometry,
            surface_shadow:   SurfaceShadow::Off,
            material:         None,
            text_material:    None,
            text_alpha_mode:  None,
            coordinate_space: CoordinateSpace::default(),
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
    pub(super) const fn tree_revision(&self) -> u64 { self.tree_revision }

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

    /// The rendering mode.
    #[must_use]
    pub const fn render_mode(&self) -> RenderMode { self.render_mode }

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
    /// usually read from [`Resolved<PanelFontUnit>`](crate::cascade::Resolved)
    /// on the panel entity, falling back to
    /// [`CascadeDefaults::panel_font_unit`].
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
/// system to consume. That pending decision lives on an internal sibling
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
}

/// Cascading attribute for panel-text font unit.
///
/// 2-tier cascade: [`DiegeticPanel::font_unit`] (panel override) →
/// [`CascadeDefaults::panel_font_unit`] (global). The resolved value is
/// cached in [`Resolved<PanelFontUnit>`](crate::cascade::Resolved) on the
/// panel entity; [`compute_panel_layouts`](crate::panel::compute_layout)
/// reads it to scale the layout tree's font sizes.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(crate) struct PanelFontUnit(pub Unit);

impl CascadeTarget for PanelFontUnit {
    type Exclude = ExcludeNone;
    type Override = DiegeticPanel;

    fn override_value(entity_override: &DiegeticPanel) -> Option<Self> {
        entity_override.font_unit().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.panel_font_unit) }
}

/// Per-frame tree-change classification consumed by the panel layout system.
#[derive(Component, Default)]
pub(super) struct DiegeticPanelChangeClassification {
    pending: Option<LayoutTreeChange>,
}

impl DiegeticPanelChangeClassification {
    pub(super) fn record(&mut self, change: LayoutTreeChange) {
        self.pending = Some(match self.pending.take() {
            None => change,
            Some(prior) => prior.combine(change),
        });
    }

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
    result:         Option<LayoutResult>,
    content_width:  f32,
    content_height: f32,
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

    /// Returns the computed layout result, or `None` if not yet computed.
    #[must_use]
    pub const fn result(&self) -> Option<&LayoutResult> { self.result.as_ref() }

    /// Returns the computed layout result mutably, or `None` if not yet computed.
    pub(super) const fn result_mut(&mut self) -> Option<&mut LayoutResult> { self.result.as_mut() }

    /// Stores the computed layout result.
    pub fn set_result(&mut self, result: LayoutResult) { self.result = Some(result); }

    /// Sets the content dimensions in world units.
    pub const fn set_content_size(&mut self, width: f32, height: f32) {
        self.content_width = width;
        self.content_height = height;
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
    use crate::LayoutTextStyle;

    fn test_tree(text: &str) -> crate::LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(text, LayoutTextStyle::new(10.0));
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

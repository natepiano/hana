//! [`DiegeticPanel`] — the main panel component and its computed companion.

use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;

use super::PanelScreenConversion;
use super::PanelScreenHandoff;
use super::PanelWorldConversion;
use super::ResolvedScreenPanelPosition;
use super::SavedPanelWorldState;
use super::apply_screen_conversion;
use super::apply_screen_root_sizing;
use super::apply_world_conversion;
use super::builder::DiegeticPanelBuilder;
use super::builder::NeedsSize;
use super::builder::Screen;
use super::builder::World;
use super::constants::PANEL_RESIZE_EPSILON;
use super::conversion;
use super::coordinate_space::CoordinateSpace;
use super::coordinate_space::PanelSpace;
use super::coordinate_space::ScreenPosition;
use super::coordinate_space::SurfaceShadow;
use super::events::LastPanelDimensions;
use super::field::PanelFieldRecord;
use super::precompose::PanelPrecomposeCache;
use super::validate_screen_conversion;
use super::validate_world_conversion;
use crate::cascade;
use crate::cascade::Cascade;
use crate::cascade::CascadeDefault;
use crate::cascade::FontUnit;
use crate::cascade::HdrTextCoverageBias;
use crate::cascade::Override;
use crate::cascade::PanelDefaults;
use crate::cascade::Resolved;
use crate::cascade::SdfMaterial;
use crate::cascade::ShapeMaterial;
use crate::cascade::TextAlpha;
use crate::cascade::TextMaterial;
use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::Dimension;
use crate::layout::InvalidSize;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::LayoutTreeChange;
use crate::layout::Lighting;
use crate::layout::PanelSize;
use crate::layout::ShadowCasting;
use crate::layout::Sidedness;
use crate::layout::Sizing;
use crate::layout::TextStyle;
use crate::layout::Unit;
use crate::render::AntiAlias;
use crate::render::DrawOrder;
use crate::render::HairlineFade;

/// Source tree plus the revision token used by derived tree caches.
#[derive(Clone, Default)]
pub(super) struct PanelTree {
    tree:     LayoutTree,
    revision: TreeRevision,
}

impl PanelTree {
    pub(super) const fn tree(&self) -> &LayoutTree { &self.tree }

    pub(super) const fn revision(&self) -> TreeRevision { self.revision }

    pub(super) const fn next_revision(&self) -> TreeRevision { self.revision.next() }

    fn replace(&mut self, tree: LayoutTree) {
        self.tree = tree;
        self.revision.bump();
    }

    fn set_element_text(&mut self, index: usize, text: &str) -> bool {
        if self.tree.set_element_text(index, text) {
            self.revision.bump();
            true
        } else {
            false
        }
    }

    fn set_element_style(&mut self, index: usize, style: TextStyle) -> bool {
        if self.tree.set_element_style(index, style) {
            self.revision.bump();
            true
        } else {
            false
        }
    }

    const fn use_next_revision_after_replacement(&mut self, previous: &Self) {
        self.revision = previous.next_revision();
    }
}

impl From<LayoutTree> for PanelTree {
    fn from(tree: LayoutTree) -> Self {
        Self {
            tree,
            revision: TreeRevision::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TreeRevision(u64);

impl TreeRevision {
    const fn next(self) -> Self { Self(self.0.wrapping_add(1)) }

    const fn bump(&mut self) { *self = self.next(); }
}

impl From<TreeRevision> for u64 {
    fn from(value: TreeRevision) -> Self { value.0 }
}

/// A diegetic UI panel attached to a 3D entity.
///
/// Defines a layout tree and the panel's dimensions in layout units.
/// World-space size is computed automatically from the panel's
/// `layout_unit`. Font sizes in the tree are interpreted in `font_unit`
/// (defaults through [`PanelDefaults::panel_font_unit`]).
///
/// Construct via [`DiegeticPanel::world`] or [`DiegeticPanel::screen`]:
///
/// ```ignore
/// commands.spawn((
///     DiegeticPanel::world()
///         .size(Mm(210.0), Mm(297.0))
///         .world_height(0.5)
///         .layout(|b| {
///             b.text(("Hello", TextStyle::new(48.0)));
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
    PanelPrecomposeCache,
    ResolvedScreenPanelPosition,
    ScaledLayoutTreeCache,
    Transform,
    Visibility
)]
pub struct DiegeticPanel {
    /// The layout tree defining this panel's UI structure.
    #[reflect(ignore)]
    tree:                              PanelTree,
    /// Panel width in layout `Unit`s. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    pub(super) width:                  f32,
    /// Panel height in layout `Unit`s. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    pub(super) height:                 f32,
    /// Unit for `width`/`height`. Set automatically by
    /// [`DiegeticPanelBuilder::size`] or [`set_size`](Self::set_size).
    pub(super) layout_unit:            Unit,
    /// Unit for font sizes in the layout tree.
    pub(super) font_unit:              Cascade<Unit>,
    /// Which point on the panel sits at the entity's [`Transform`] position.
    /// Defaults to [`Anchor::TopLeft`].
    pub(super) anchor:                 Anchor,
    /// Target world width in meters. When set, the panel is uniformly scaled
    /// so its width matches this value (height follows aspect ratio).
    /// If both `world_width` and `world_height` are set, non-uniform scaling
    /// is applied.
    pub(super) world_width:            Option<f32>,
    /// Target world height in meters. When set, the panel is uniformly scaled
    /// so its height matches this value (width follows aspect ratio).
    pub(super) world_height:           Option<f32>,
    /// Panel-level shadow casting authoring.
    pub(super) shadow_casting:         Cascade<ShadowCasting>,
    /// Default source-material handle for backgrounds and borders.
    /// Individual elements can override via `El::material`; `base_color` is
    /// overridden by the layout color when both are set.
    #[reflect(ignore)]
    pub(super) material:               Cascade<Handle<StandardMaterial>>,
    /// Default source-material handle for text.
    ///
    /// `base_color` is overridden by `TextStyle::color` when set.
    #[reflect(ignore)]
    pub(super) text_material:          Cascade<Handle<StandardMaterial>>,
    /// Default source-material handle for panel-shape primitives.
    ///
    /// Shape-local colors override `base_color` before projection.
    #[reflect(ignore)]
    pub(super) shape_material:         Cascade<Handle<StandardMaterial>>,
    /// Panel-level authoring for text [`AlphaMode`].
    pub(super) text_alpha_mode:        Cascade<AlphaMode>,
    /// Panel-level authoring for HDR text coverage compensation.
    pub(super) hdr_text_coverage_bias: Cascade<f32>,
    /// Whether the panel is world-space or screen-space.
    pub(super) coordinate_space:       CoordinateSpace,
    /// Maps each text run's [`PanelElementId`](crate::PanelElementId) to the entity
    /// reconcile materialized for it, so
    /// [`text_child`](Self::text_child) resolves a named run in O(1).
    ///
    /// `reconcile_panel_text_children` rebuilds this from scratch every pass and
    /// writes it without tripping change detection, so it never re-triggers
    /// layout; [`set_tree`](DiegeticPanelCommands::set_tree) clears it so a stale
    /// id stops resolving immediately.
    #[reflect(ignore)]
    pub(crate) text_index:             HashMap<crate::PanelElementId, Entity>,
}

impl Default for DiegeticPanel {
    fn default() -> Self {
        Self {
            tree:                   PanelTree::default(),
            width:                  0.0,
            height:                 0.0,
            layout_unit:            Unit::Meters,
            font_unit:              Cascade::Inherit,
            anchor:                 Anchor::TopLeft,
            world_width:            None,
            world_height:           None,
            shadow_casting:         Cascade::Inherit,
            material:               Cascade::Inherit,
            text_material:          Cascade::Inherit,
            shape_material:         Cascade::Inherit,
            text_alpha_mode:        Cascade::Inherit,
            hdr_text_coverage_bias: Cascade::Inherit,
            coordinate_space:       CoordinateSpace::default(),
            text_index:             HashMap::new(),
        }
    }
}

impl DiegeticPanel {
    pub(super) fn with_initial_tree(tree: LayoutTree) -> Self {
        Self {
            tree: PanelTree::from(tree),
            ..Self::default()
        }
    }
}

// ── Public read-only accessors ──────────────────────────────────────────────

impl DiegeticPanel {
    /// Returns a reference to the layout tree.
    #[must_use]
    pub const fn tree(&self) -> &LayoutTree { self.tree.tree() }

    /// Revision of the current layout tree.
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn tree_revision(&self) -> TreeRevision { self.tree.revision() }

    /// Returns the revision-owned tree source used by derived caches.
    #[must_use]
    pub(super) const fn tree_source(&self) -> &PanelTree { &self.tree }

    /// Returns the revision a successful tree replacement will produce.
    #[must_use]
    pub(crate) const fn next_tree_revision(&self) -> TreeRevision { self.tree.next_revision() }

    /// Panel width in layout units.
    #[must_use]
    pub const fn width(&self) -> f32 { self.width }

    /// Panel height in layout units.
    #[must_use]
    pub const fn height(&self) -> f32 { self.height }

    /// The layout unit for this panel's dimensions.
    #[must_use]
    pub const fn layout_unit(&self) -> Unit { self.layout_unit }

    /// The font unit authoring for this panel.
    #[must_use]
    pub const fn font_unit(&self) -> Cascade<Unit> { self.font_unit }

    /// The panel's anchor point.
    #[must_use]
    pub const fn anchor(&self) -> Anchor { self.anchor }

    /// Whether the panel surface casts shadows under the compatibility API.
    #[must_use]
    pub const fn surface_shadow(&self) -> SurfaceShadow {
        match self.shadow_casting.resolve_or(ShadowCasting::On) {
            ShadowCasting::Off => SurfaceShadow::Off,
            ShadowCasting::On => SurfaceShadow::On,
        }
    }

    /// The panel-level shadow casting authoring.
    #[must_use]
    pub const fn shadow_casting(&self) -> Cascade<ShadowCasting> { self.shadow_casting }

    /// The default panel material authoring.
    #[must_use]
    pub const fn material(&self) -> Cascade<&Handle<StandardMaterial>> { self.material.as_ref() }

    /// Mutable access to the default panel material.
    pub const fn material_mut(&mut self) -> &mut Cascade<Handle<StandardMaterial>> {
        &mut self.material
    }

    /// The default text material authoring.
    #[must_use]
    pub const fn text_material(&self) -> Cascade<&Handle<StandardMaterial>> {
        self.text_material.as_ref()
    }

    /// Mutable access to the default text material.
    pub const fn text_material_mut(&mut self) -> &mut Cascade<Handle<StandardMaterial>> {
        &mut self.text_material
    }

    /// The default panel-shape material authoring.
    #[must_use]
    pub const fn shape_material(&self) -> Cascade<&Handle<StandardMaterial>> {
        self.shape_material.as_ref()
    }

    /// Mutable access to the default panel-shape material handle.
    pub const fn shape_material_mut(&mut self) -> &mut Cascade<Handle<StandardMaterial>> {
        &mut self.shape_material
    }

    /// The panel-level text [`AlphaMode`] authoring.
    #[must_use]
    pub const fn text_alpha_mode(&self) -> Cascade<AlphaMode> { self.text_alpha_mode }

    /// The panel-level HDR text coverage-bias authoring.
    #[must_use]
    pub const fn hdr_text_coverage_bias(&self) -> Cascade<f32> { self.hdr_text_coverage_bias }

    /// The panel's coordinate space (world or screen).
    #[must_use]
    pub const fn coordinate_space(&self) -> &CoordinateSpace { &self.coordinate_space }

    pub(crate) const fn authored_world_width(&self) -> Option<f32> { self.world_width }

    pub(crate) const fn authored_world_height(&self) -> Option<f32> { self.world_height }

    /// Resolves a text run's [`PanelElementId`](crate::PanelElementId) to the entity
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
    pub fn text_child(&self, id: &crate::PanelElementId) -> Option<Entity> {
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
    pub(crate) fn replace_tree_full_rebuild(&mut self, tree: LayoutTree) {
        self.tree.replace(tree);
        self.text_index.clear();
    }

    fn replace_from_precompose_helper(&mut self, panel: Self) {
        let mut panel = panel;
        panel.tree.use_next_revision_after_replacement(&self.tree);
        panel.text_index.clear();
        *self = panel;
    }

    /// Bench-support wrapper for [`Self::replace_tree_full_rebuild`].
    #[cfg(feature = "bench_support")]
    #[doc(hidden)]
    pub fn set_tree_full_rebuild(&mut self, tree: LayoutTree) {
        self.replace_tree_full_rebuild(tree);
    }

    /// Writes a run-text edit into the authoritative `El.text` tree and bumps the
    /// edit-session revision. Returns whether the cache changed.
    ///
    /// `El.text` is the single source for run text; the child
    /// [`TextContent`](crate::TextContent) is derived output — reconcile overwrites
    /// it from the tree each frame. The public edit path (`PanelText` / `DiegeticTextMut`
    /// via `TextEdit`) calls this to change a run's string. Skips the revision bump
    /// when the string is unchanged, avoiding a layout pass and a cache lookup.
    pub(crate) fn sync_run_text_cache(&mut self, index: usize, text: &str) -> bool {
        self.tree.set_element_text(index, text)
    }

    /// Writes a restyle into the authored `El.config` and bumps the edit-session
    /// revision. Returns whether the style changed.
    ///
    /// Like run text (see [`sync_run_text_cache`](Self::sync_run_text_cache)),
    /// the tree is the single authoritative source: `El.config` for style,
    /// `El.text` for the string, while the run child is derived output reconcile
    /// rewrites. A label restyle (font, size) mutates `El.config` through this
    /// method; the relayout it triggers flows the new config to the run via
    /// reconcile, so measurement and rendering stay on the same source. Skips the
    /// revision bump (and so the layout) when the style already matches.
    pub(crate) fn restyle_run(&mut self, index: usize, style: TextStyle) -> bool {
        self.tree.set_element_style(index, style)
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
    /// [`PanelDefaults::panel_font_unit`]).
    #[must_use]
    pub fn font_scale(&self, panel_font_unit: Unit) -> f32 {
        let font_meters_per_unit = panel_font_unit.meters_per_unit();
        let layout_meters_per_unit = self.layout_unit.meters_per_unit();
        font_meters_per_unit / layout_meters_per_unit
    }
}

/// Extension methods for mutating diegetic panels through [`Commands`].
///
/// This trait is an ergonomic wrapper around panel mutations that need more
/// schedule coordination than a plain `&mut DiegeticPanel` method can provide.
/// Tree replacement records a pending layout-change classification, and
/// coordinate-space conversions are applied by the panel pipeline before layout
/// and screen placement run. Keeping the wrapper here lets callers use a focused
/// panel API without learning those internal components or schedule fences.
pub trait DiegeticPanelCommands {
    /// Queues a layout-tree replacement that records whether the change is
    /// visual-only or layout-affecting.
    ///
    /// The queued setter is deferred. Schedule systems that call this before
    /// panel layout systems when the update must be visible in the same frame.
    fn set_tree(&mut self, entity: Entity, tree: LayoutTree);

    /// Starts an animated conversion of an existing world panel to screen space.
    ///
    /// The panel remains world-space, but its visual source is normalized to the
    /// screen conversion's pixel sizing so the final handoff can switch cameras
    /// without rebuilding the visual tree. The final handoff should use
    /// [`Self::finish_panel_to_screen`].
    fn begin_panel_to_screen<C>(&mut self, entity: Entity, camera: Entity, conversion: C)
    where
        C: Into<PanelScreenConversion>;

    /// Queues a resolved conversion of an existing panel to screen space.
    ///
    /// `conversion` can be a [`PanelScreenConversion`] or any type that converts
    /// into one, including [`PanelScreenProjection`](super::PanelScreenProjection).
    /// This is the advanced resolved-data path; higher-level callers should
    /// prefer [`PanelScreenConversionParam::to_screen_at`](super::PanelScreenConversionParam::to_screen_at).
    fn finish_panel_to_screen<C>(&mut self, entity: Entity, camera: Entity, conversion: C)
    where
        C: Into<PanelScreenConversion>;

    /// Queues a resolved conversion of an existing panel to screen space without
    /// saving a screen handoff camera.
    ///
    /// This is primarily for advanced callers applying their own saved-state
    /// policy.
    fn apply_panel_screen_conversion<C>(&mut self, entity: Entity, conversion: C)
    where
        C: Into<PanelScreenConversion>;

    /// Queues a resolved conversion of an existing panel to world space.
    ///
    /// This is the advanced resolved-data path; higher-level callers should
    /// prefer [`PanelWorldConversionParam::to_world`](super::PanelWorldConversionParam::to_world)
    /// or [`PanelWorldConversionParam::to_world_at`](super::PanelWorldConversionParam::to_world_at).
    fn apply_panel_world_conversion<C>(&mut self, entity: Entity, conversion: C)
    where
        C: Into<PanelWorldConversion>;
}

#[derive(Component, Clone)]
pub(super) enum PendingPanelConversion {
    BeginScreen {
        camera:     Entity,
        conversion: PanelScreenConversion,
    },
    Screen {
        camera:     Option<Entity>,
        conversion: PanelScreenConversion,
    },
    World(PanelWorldConversion),
}

#[derive(Component, Clone, Copy)]
pub(super) struct PreparedPanelScreenConversion {
    size: Vec2,
}

impl PreparedPanelScreenConversion {
    fn matches(self, size: Vec2) -> bool {
        (self.size.x - size.x).abs() <= PANEL_RESIZE_EPSILON
            && (self.size.y - size.y).abs() <= PANEL_RESIZE_EPSILON
    }
}

#[derive(Clone, Copy)]
struct ScreenConversionSource<'a> {
    defaults:           &'a PanelDefaults,
    font_resolved:      Option<&'a Resolved<FontUnit>>,
    lighting_resolved:  Option<&'a Resolved<Lighting>>,
    sidedness_resolved: Option<&'a Resolved<Sidedness>>,
    render_layers:      Option<&'a RenderLayers>,
}

impl ScreenConversionSource<'_> {
    fn font_unit(self) -> Unit {
        self.font_resolved
            .map_or(self.defaults.panel_font_unit, |resolved| resolved.0.0)
    }

    fn saved_world_state(
        self,
        panel: &DiegeticPanel,
        transform: &Transform,
    ) -> SavedPanelWorldState {
        conversion::saved_world_state_from_panel(
            panel,
            transform,
            self.font_unit(),
            self.lighting_resolved
                .map_or(Lighting::Lit, |resolved| resolved.0),
            self.sidedness_resolved
                .map_or(Sidedness::BothSides, |resolved| resolved.0),
            self.render_layers,
        )
    }
}

impl DiegeticPanelCommands for Commands<'_, '_> {
    fn set_tree(&mut self, entity: Entity, tree: LayoutTree) {
        self.run_system_cached_with(set_tree_command, (entity, tree));
    }

    fn begin_panel_to_screen<C>(&mut self, entity: Entity, camera: Entity, conversion: C)
    where
        C: Into<PanelScreenConversion>,
    {
        self.entity(entity)
            .insert(PendingPanelConversion::BeginScreen {
                camera,
                conversion: conversion.into(),
            });
    }

    fn finish_panel_to_screen<C>(&mut self, entity: Entity, camera: Entity, conversion: C)
    where
        C: Into<PanelScreenConversion>,
    {
        self.entity(entity).insert(PendingPanelConversion::Screen {
            camera:     Some(camera),
            conversion: conversion.into(),
        });
    }

    fn apply_panel_screen_conversion<C>(&mut self, entity: Entity, conversion: C)
    where
        C: Into<PanelScreenConversion>,
    {
        self.entity(entity).insert(PendingPanelConversion::Screen {
            camera:     None,
            conversion: conversion.into(),
        });
    }

    fn apply_panel_world_conversion<C>(&mut self, entity: Entity, conversion: C)
    where
        C: Into<PanelWorldConversion>,
    {
        self.entity(entity)
            .insert(PendingPanelConversion::World(conversion.into()));
    }
}

fn set_tree_command(
    In((entity, next_tree)): In<(Entity, LayoutTree)>,
    mut panels: Query<(&mut DiegeticPanel, &mut DiegeticPanelChangeClassification)>,
) {
    let Ok((mut panel, mut classification)) = panels.get_mut(entity) else {
        return;
    };
    let change = panel.tree().classify_change(&next_tree);
    classification.record_tree_change(change);
    panel.replace_tree_full_rebuild(next_tree);
}

pub(crate) fn apply_precompose_helper_panel(
    In((entity, next_panel)): In<(Entity, DiegeticPanel)>,
    mut panels: Query<(&mut DiegeticPanel, &mut DiegeticPanelChangeClassification)>,
) {
    let Ok((mut panel, mut classification)) = panels.get_mut(entity) else {
        return;
    };
    classification.record_tree_change(LayoutTreeChange::LayoutAffecting);
    panel.replace_from_precompose_helper(next_panel);
}

pub(super) fn apply_pending_panel_conversions(
    defaults: Res<PanelDefaults>,
    mut commands: Commands,
    primary: Query<Entity, With<PrimaryWindow>>,
    windows: Query<&Window>,
    cameras: Query<&GlobalTransform, With<Camera>>,
    mut panels: Query<(
        Entity,
        &PendingPanelConversion,
        &mut DiegeticPanel,
        &mut Transform,
        &mut DiegeticPanelChangeClassification,
        &mut ResolvedScreenPanelPosition,
        Option<&Resolved<FontUnit>>,
        Option<&Resolved<Lighting>>,
        Option<&Resolved<Sidedness>>,
        Option<&SavedPanelWorldState>,
        Option<&PreparedPanelScreenConversion>,
        Option<&RenderLayers>,
    )>,
) {
    for (
        entity,
        pending,
        mut panel,
        mut transform,
        mut classification,
        mut resolved_position,
        font_resolved,
        lighting_resolved,
        sidedness_resolved,
        saved,
        prepared_screen,
        source_render_layers,
    ) in &mut panels
    {
        let source = ScreenConversionSource {
            defaults: &defaults,
            font_resolved,
            lighting_resolved,
            sidedness_resolved,
            render_layers: source_render_layers,
        };
        match pending.clone() {
            PendingPanelConversion::BeginScreen { camera, conversion } => {
                begin_panel_to_screen_now(
                    entity,
                    camera,
                    conversion,
                    &mut commands,
                    &cameras,
                    &mut panel,
                    &transform,
                    &mut classification,
                    saved,
                    source,
                );
            },
            PendingPanelConversion::Screen { camera, conversion } => {
                apply_panel_screen_conversion_now(
                    entity,
                    camera,
                    conversion,
                    &mut commands,
                    &primary,
                    &windows,
                    &cameras,
                    &mut panel,
                    &mut transform,
                    &mut resolved_position,
                    source,
                    prepared_screen,
                );
            },
            PendingPanelConversion::World(conversion) => apply_panel_world_conversion_now(
                entity,
                conversion,
                &mut commands,
                &mut panel,
                &mut transform,
                &mut classification,
                saved,
                &mut resolved_position,
            ),
        }
        commands.entity(entity).remove::<PendingPanelConversion>();
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "screen conversion applies one coordinated ECS handoff"
)]
fn apply_panel_screen_conversion_now(
    entity: Entity,
    camera: Option<Entity>,
    conversion: PanelScreenConversion,
    commands: &mut Commands<'_, '_>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    windows: &Query<&Window>,
    cameras: &Query<&GlobalTransform, With<Camera>>,
    panel: &mut DiegeticPanel,
    transform: &mut Transform,
    resolved_position: &mut ResolvedScreenPanelPosition,
    source: ScreenConversionSource<'_>,
    prepared_screen: Option<&PreparedPanelScreenConversion>,
) {
    if let Err(error) = validate_screen_conversion(&conversion) {
        warn!("failed to convert panel {entity:?} to screen space: {error}");
        return;
    }
    let was_world_panel = !panel.coordinate_space().is_screen();
    let rotation = conversion.rotation;
    let anchor_position = conversion.anchor_position;
    let render_layers = conversion.render_layers.clone();
    let handoff_conversion = conversion.clone();
    let window_size = screen_conversion_window_size(&conversion, primary, windows);
    let prepared = prepared_screen.is_some_and(|prepared| prepared.matches(conversion.size));
    if was_world_panel
        && !prepared
        && !prepare_world_panel_for_screen_conversion(
            entity,
            &conversion,
            commands,
            panel,
            transform,
            None,
            source,
        )
    {
        return;
    }
    let handoff = camera
        .and_then(|camera| {
            cameras
                .get(camera)
                .ok()
                .map(|transform| (camera, transform))
        })
        .and_then(|(camera, camera_transform)| {
            screen_handoff(camera, handoff_conversion, camera_transform, transform)
        });
    if let Err(error) = apply_screen_conversion(panel, conversion) {
        warn!("failed to convert panel {entity:?} to screen space: {error}");
        return;
    }
    transform.translation.z = 0.0;
    if let Some(window_size) = window_size {
        let half_size = window_size * 0.5;
        transform.translation.x = anchor_position.x - half_size.x;
        transform.translation.y = half_size.y - anchor_position.y;
    }
    transform.rotation = Quat::from_rotation_z(rotation);
    transform.scale = Vec3::ONE;
    *resolved_position = ResolvedScreenPanelPosition::default();
    let mut entity_commands = commands.entity(entity);
    entity_commands.insert((render_layers, PanelSpace::Screen));
    if let Some(handoff) = handoff.filter(|_| was_world_panel) {
        entity_commands.insert(handoff);
    }
    if was_world_panel {
        entity_commands.remove::<PreparedPanelScreenConversion>();
    }
}

fn begin_panel_to_screen_now(
    entity: Entity,
    camera: Entity,
    conversion: PanelScreenConversion,
    commands: &mut Commands<'_, '_>,
    cameras: &Query<&GlobalTransform, With<Camera>>,
    panel: &mut DiegeticPanel,
    transform: &Transform,
    classification: &mut DiegeticPanelChangeClassification,
    saved: Option<&SavedPanelWorldState>,
    source: ScreenConversionSource<'_>,
) {
    if let Err(error) = validate_screen_conversion(&conversion) {
        warn!("failed to prepare panel {entity:?} for screen conversion: {error}");
        return;
    }
    if panel.coordinate_space().is_screen() {
        return;
    }
    if prepare_world_panel_for_screen_conversion(
        entity,
        &conversion,
        commands,
        panel,
        transform,
        saved,
        source,
    ) {
        if let Ok(camera_transform) = cameras.get(camera)
            && let Some(handoff) = screen_handoff(camera, conversion, camera_transform, transform)
        {
            commands.entity(entity).insert(handoff);
        }
        classification.record_tree_change(LayoutTreeChange::LayoutAffecting);
    }
}

fn prepare_world_panel_for_screen_conversion(
    entity: Entity,
    conversion: &PanelScreenConversion,
    commands: &mut Commands<'_, '_>,
    panel: &mut DiegeticPanel,
    transform: &Transform,
    saved: Option<&SavedPanelWorldState>,
    source: ScreenConversionSource<'_>,
) -> bool {
    let old_world_width = panel.world_width();
    let old_world_height = panel.world_height();
    let old_points_to_world = panel.points_to_world();
    let points_to_pixels = old_points_to_world * (conversion.size.y / old_world_height);
    if !points_to_pixels.is_finite() || points_to_pixels <= 0.0 {
        warn!("failed to scale panel {entity:?} tree for screen conversion: invalid source scale");
        return false;
    }

    if saved.is_none() {
        commands
            .entity(entity)
            .insert(source.saved_world_state(panel, transform));
    }

    let mut tree = panel.tree().screen_source_scaled(
        panel.layout_unit().to_points(),
        source.font_unit().to_points(),
        points_to_pixels,
    );
    apply_screen_root_sizing(&mut tree, conversion.width, conversion.height);
    panel.replace_tree_full_rebuild(tree);
    panel.width = conversion.size.x;
    panel.height = conversion.size.y;
    panel.layout_unit = Unit::Pixels;
    panel.font_unit = Cascade::Override(Unit::Pixels);
    panel.world_width = Some(old_world_width);
    panel.world_height = Some(old_world_height);
    panel.coordinate_space = CoordinateSpace::World {
        width:  fixed_pixel_sizing(conversion.size.x),
        height: fixed_pixel_sizing(conversion.size.y),
    };
    commands.entity(entity).insert((
        PreparedPanelScreenConversion {
            size: conversion.size,
        },
        Override(FontUnit(Unit::Pixels)),
        Resolved(FontUnit(Unit::Pixels)),
        Override(Lighting::Unlit),
        Resolved(Lighting::Unlit),
        Override(Sidedness::FrontOnly),
        Resolved(Sidedness::FrontOnly),
    ));
    true
}

fn screen_handoff(
    camera: Entity,
    conversion: PanelScreenConversion,
    camera_transform: &GlobalTransform,
    panel_transform: &Transform,
) -> Option<PanelScreenHandoff> {
    let camera_transform = camera_transform.compute_transform();
    let forward = camera_transform.rotation * Vec3::NEG_Z;
    let distance = (panel_transform.translation - camera_transform.translation).dot(forward);
    if !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    Some(conversion::panel_screen_handoff(
        camera, conversion, distance,
    ))
}

const fn fixed_pixel_sizing(value: f32) -> Sizing {
    Sizing::Fixed(Dimension {
        value,
        unit: Some(Unit::Pixels),
    })
}

fn screen_conversion_window_size(
    conversion: &PanelScreenConversion,
    primary: &Query<Entity, With<PrimaryWindow>>,
    windows: &Query<&Window>,
) -> Option<Vec2> {
    let entity = match conversion.window {
        WindowRef::Primary => primary.single().ok()?,
        WindowRef::Entity(entity) => entity,
    };
    let window = windows.get(entity).ok()?;
    let size = Vec2::new(window.width(), window.height());
    (size.is_finite() && size.x > 0.0 && size.y > 0.0).then_some(size)
}

fn apply_panel_world_conversion_now(
    entity: Entity,
    conversion: PanelWorldConversion,
    commands: &mut Commands<'_, '_>,
    panel: &mut DiegeticPanel,
    transform: &mut Transform,
    classification: &mut DiegeticPanelChangeClassification,
    saved: Option<&SavedPanelWorldState>,
    resolved_position: &mut ResolvedScreenPanelPosition,
) {
    if let Err(error) = validate_world_conversion(&conversion) {
        warn!("failed to convert panel {entity:?} to world space: {error}");
        return;
    }
    let saved = if matches!(
        conversion.restore_saved_world,
        conversion::SavedWorldRestoreMode::Restore
    ) {
        let Some(saved) = saved.cloned() else {
            warn!("failed to convert panel {entity:?} to saved world space: no saved world state");
            return;
        };
        Some(saved)
    } else {
        None
    };
    let next_transform = conversion.transform;
    if let Some(saved) = saved.as_ref() {
        if let Err(error) = saved.apply_world_conversion(panel, &conversion) {
            warn!("failed to convert panel {entity:?} to saved world space: {error}");
            return;
        }
        classification.record_tree_change(LayoutTreeChange::LayoutAffecting);
    } else if let Err(error) = apply_world_conversion(panel, conversion) {
        warn!("failed to convert panel {entity:?} to world space: {error}");
        return;
    }
    *transform = next_transform;
    *resolved_position = ResolvedScreenPanelPosition::default();
    let mut entity_commands = commands.entity(entity);
    entity_commands.insert(PanelSpace::World);
    if let Some(saved) = saved {
        entity_commands.insert((
            Override(FontUnit(saved.resolved_font_unit)),
            Resolved(FontUnit(saved.resolved_font_unit)),
            Override(saved.resolved_lighting),
            Resolved(saved.resolved_lighting),
            Override(saved.resolved_sidedness),
            Resolved(saved.resolved_sidedness),
        ));
        if let Some(render_layers) = saved.render_layers {
            entity_commands.insert(render_layers);
        } else {
            entity_commands.remove::<RenderLayers>();
        }
    } else {
        entity_commands.remove::<RenderLayers>();
    }
    entity_commands.remove::<PreparedPanelScreenConversion>();
}

/// Spawn-time authoring bridge for panel cascade overrides.
///
/// Reads a newly-added [`DiegeticPanel`]'s `text_alpha_mode`,
/// `hdr_text_coverage_bias`, and `font_unit` authoring fields, inserts the
/// matching `Override<A>` cascade components, and seeds the panel's
/// `Resolved<FontUnit>` (which `compute_panel_layouts` reads).
///
/// A panel is depth-1 with no cascade ancestor, so it cannot inherit `FontUnit`
/// from a parent — it therefore **always** carries `Override<FontUnit>`, from
/// `panel.font_unit()` when set else [`PanelDefaults::panel_font_unit`]. That
/// is why `panel_font_unit` is a construction-time seed, not a cascade global:
/// no panel ever reads the `FontUnit` global, and a runtime `font_unit` change
/// does not reach existing panels. `text_alpha_mode` is the panel's optional
/// override for the alpha its labels inherit; absent → labels resolve to
/// `CascadeDefault<TextAlpha>` through the parent-walk. The panel needs no
/// `Resolved<TextAlpha>` of its own — only its labels render text, and they
/// walk up to this `Override<TextAlpha>` directly. The panel twin of the
/// standalone `TextStyle` authoring bridge.
#[derive(SystemParam)]
pub(super) struct PanelCascadeSeedParams<'w, 's> {
    anti_alias_overrides:             Query<'w, 's, &'static Override<AntiAlias>>,
    hairline_fade_overrides:          Query<'w, 's, &'static Override<HairlineFade>>,
    sdf_material_overrides:           Query<'w, 's, &'static Override<SdfMaterial>>,
    text_material_overrides:          Query<'w, 's, &'static Override<TextMaterial>>,
    shape_material_overrides:         Query<'w, 's, &'static Override<ShapeMaterial>>,
    hdr_text_coverage_bias_overrides: Query<'w, 's, &'static Override<HdrTextCoverageBias>>,
    shadow_casting_overrides:         Query<'w, 's, &'static Override<ShadowCasting>>,
    parents:                          Query<'w, 's, &'static ChildOf>,
    anti_alias_default:               Res<'w, CascadeDefault<AntiAlias>>,
    hairline_fade_default:            Res<'w, CascadeDefault<HairlineFade>>,
    sdf_material_default:             Option<Res<'w, CascadeDefault<SdfMaterial>>>,
    text_material_default:            Option<Res<'w, CascadeDefault<TextMaterial>>>,
    shape_material_default:           Option<Res<'w, CascadeDefault<ShapeMaterial>>>,
    hdr_text_coverage_bias_default:   Res<'w, CascadeDefault<HdrTextCoverageBias>>,
    shadow_casting_default:           Res<'w, CascadeDefault<ShadowCasting>>,
}

pub(super) fn seed_panel_overrides(
    trigger: On<Add, DiegeticPanel>,
    panels: Query<&DiegeticPanel>,
    defaults: Res<PanelDefaults>,
    cascade_params: PanelCascadeSeedParams,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(panel) = panels.get(entity) else {
        return;
    };
    let font_unit = panel.font_unit().resolve_or(defaults.panel_font_unit);
    // Per-context glyph defaults: screen panels are unlit, front-facing HUD
    // surfaces; world panels stay lit and double-sided (the global cascade
    // default). Labels still override per-label via
    // `TextStyle::with_lighting` / `with_sidedness` (captured in
    // `reconcile_panel_text_children`). Material handles are resolved by
    // render systems so this headless bridge stays independent of
    // `Assets<StandardMaterial>`.
    let is_screen = panel.coordinate_space().is_screen();
    // The panel renders its elements' analytic line marks, so it carries its
    // own resolved anti-alias mode and hairline fade for the line batcher to
    // read (and to filter on `Changed<Resolved<A>>`). A fresh panel resolves
    // to the global defaults; later `override_*` verbs self-heal these.
    let anti_alias = cascade::resolve_walk::<AntiAlias>(
        entity,
        &cascade_params.anti_alias_overrides,
        &cascade_params.parents,
        cascade_params.anti_alias_default.0,
    );
    let hairline_fade = cascade::resolve_walk::<HairlineFade>(
        entity,
        &cascade_params.hairline_fade_overrides,
        &cascade_params.parents,
        cascade_params.hairline_fade_default.0,
    );
    let sdf_material = resolve_panel_sdf_material(
        entity,
        panel,
        &cascade_params.sdf_material_overrides,
        &cascade_params.parents,
        cascade_params.sdf_material_default.as_deref(),
    );
    let text_material = resolve_panel_text_material(
        entity,
        panel,
        &cascade_params.text_material_overrides,
        &cascade_params.parents,
        cascade_params.text_material_default.as_deref(),
    );
    let shape_material = resolve_panel_shape_material(
        entity,
        panel,
        &cascade_params.shape_material_overrides,
        &cascade_params.parents,
        cascade_params.shape_material_default.as_deref(),
    );
    let hdr_text_coverage_bias = panel.hdr_text_coverage_bias().map_or_else(
        || {
            cascade::resolve_walk::<HdrTextCoverageBias>(
                entity,
                &cascade_params.hdr_text_coverage_bias_overrides,
                &cascade_params.parents,
                cascade_params.hdr_text_coverage_bias_default.0,
            )
        },
        HdrTextCoverageBias,
    );
    let shadow_casting = panel.shadow_casting().map_or_else(
        || {
            cascade::resolve_walk::<ShadowCasting>(
                entity,
                &cascade_params.shadow_casting_overrides,
                &cascade_params.parents,
                cascade_params.shadow_casting_default.0,
            )
        },
        core::convert::identity,
    );
    let mut entity_commands = commands.entity(entity);
    entity_commands.insert((
        Resolved(FontUnit(font_unit)),
        Resolved(anti_alias),
        Resolved(hairline_fade),
        Resolved(sdf_material.clone()),
        Resolved(text_material.clone()),
        Resolved(shape_material.clone()),
        Resolved(hdr_text_coverage_bias),
        Resolved(shadow_casting),
    ));
    cascade::apply_cascade_override(&mut entity_commands, FontUnit(font_unit));
    if let Cascade::Override(alpha_mode) = panel.text_alpha_mode() {
        cascade::apply_cascade_override(&mut entity_commands, TextAlpha(alpha_mode));
    }
    if let Cascade::Override(bias) = panel.hdr_text_coverage_bias() {
        cascade::apply_cascade_override(&mut entity_commands, HdrTextCoverageBias(bias));
    }
    if panel.material().is_override() {
        cascade::apply_cascade_override(&mut entity_commands, sdf_material);
    }
    if panel.text_material().is_override() {
        cascade::apply_cascade_override(&mut entity_commands, text_material);
    }
    if panel.shape_material().is_override() {
        cascade::apply_cascade_override(&mut entity_commands, shape_material);
    }
    if panel.shadow_casting().is_override() {
        cascade::apply_cascade_override(&mut entity_commands, shadow_casting);
    }
    if is_screen {
        cascade::apply_cascade_override(&mut entity_commands, Lighting::Unlit);
        cascade::apply_cascade_override(&mut entity_commands, Sidedness::FrontOnly);
    }
}

fn resolve_panel_sdf_material(
    entity: Entity,
    panel: &DiegeticPanel,
    overrides: &Query<&Override<SdfMaterial>>,
    parents: &Query<&ChildOf>,
    default: Option<&CascadeDefault<SdfMaterial>>,
) -> SdfMaterial {
    panel.material().cloned().map_or_else(
        || {
            cascade::resolve_walk::<SdfMaterial>(
                entity,
                overrides,
                parents,
                default.cloned().unwrap_or_default().0,
            )
        },
        SdfMaterial,
    )
}

fn resolve_panel_text_material(
    entity: Entity,
    panel: &DiegeticPanel,
    overrides: &Query<&Override<TextMaterial>>,
    parents: &Query<&ChildOf>,
    default: Option<&CascadeDefault<TextMaterial>>,
) -> TextMaterial {
    panel.text_material().cloned().map_or_else(
        || {
            cascade::resolve_walk::<TextMaterial>(
                entity,
                overrides,
                parents,
                default.cloned().unwrap_or_default().0,
            )
        },
        TextMaterial,
    )
}

fn resolve_panel_shape_material(
    entity: Entity,
    panel: &DiegeticPanel,
    overrides: &Query<&Override<ShapeMaterial>>,
    parents: &Query<&ChildOf>,
    default: Option<&CascadeDefault<ShapeMaterial>>,
) -> ShapeMaterial {
    panel.shape_material().cloned().map_or_else(
        || {
            cascade::resolve_walk::<ShapeMaterial>(
                entity,
                overrides,
                parents,
                default.cloned().unwrap_or_default().0,
            )
        },
        ShapeMaterial,
    )
}

pub(super) fn sync_panel_cascade_overrides(
    panels: Query<(Entity, Ref<DiegeticPanel>), Changed<DiegeticPanel>>,
    mut commands: Commands,
) {
    for (entity, panel) in &panels {
        if panel.is_added() {
            continue;
        }
        let mut entity_commands = commands.entity(entity);
        match panel.material().cloned() {
            Cascade::Override(material) => {
                cascade::apply_cascade_override(&mut entity_commands, SdfMaterial(material));
            },
            Cascade::Inherit => {
                cascade::remove_cascade_override::<SdfMaterial>(&mut entity_commands);
            },
        }
        match panel.text_material().cloned() {
            Cascade::Override(material) => {
                cascade::apply_cascade_override(&mut entity_commands, TextMaterial(material));
            },
            Cascade::Inherit => {
                cascade::remove_cascade_override::<TextMaterial>(&mut entity_commands);
            },
        }
        match panel.shape_material().cloned() {
            Cascade::Override(material) => {
                cascade::apply_cascade_override(&mut entity_commands, ShapeMaterial(material));
            },
            Cascade::Inherit => {
                cascade::remove_cascade_override::<ShapeMaterial>(&mut entity_commands);
            },
        }
        match panel.hdr_text_coverage_bias() {
            Cascade::Override(bias) => {
                cascade::apply_cascade_override(&mut entity_commands, HdrTextCoverageBias(bias));
            },
            Cascade::Inherit => {
                cascade::remove_cascade_override::<HdrTextCoverageBias>(&mut entity_commands);
            },
        }
        match panel.text_alpha_mode() {
            Cascade::Override(alpha_mode) => {
                cascade::apply_cascade_override(&mut entity_commands, TextAlpha(alpha_mode));
            },
            Cascade::Inherit => {
                cascade::remove_cascade_override::<TextAlpha>(&mut entity_commands);
            },
        }
        match panel.shadow_casting() {
            Cascade::Override(shadow_casting) => {
                cascade::apply_cascade_override(&mut entity_commands, shadow_casting);
            },
            Cascade::Inherit => {
                cascade::remove_cascade_override::<ShadowCasting>(&mut entity_commands);
            },
        }
    }
}

/// Per-frame tree-change classification consumed by the panel layout system.
#[derive(Component, Default)]
pub(crate) struct DiegeticPanelChangeClassification {
    pending:                     Option<LayoutTreeChange>,
    tree_visual_geometry_stable: bool,
}

impl DiegeticPanelChangeClassification {
    fn record(&mut self, change: LayoutTreeChange) {
        self.pending = Some(match self.pending {
            None => change,
            Some(prior) => prior.combine(change),
        });
    }

    pub(super) fn record_tree_change(&mut self, change: LayoutTreeChange) {
        let prior = self.pending;
        let prior_stable = self.tree_visual_geometry_stable;
        self.record(change);

        self.tree_visual_geometry_stable = match self.pending {
            Some(LayoutTreeChange::VisualOnly) => match change {
                LayoutTreeChange::VisualOnly => match prior {
                    None | Some(LayoutTreeChange::Identical) => true,
                    Some(LayoutTreeChange::VisualOnly) => prior_stable,
                    Some(LayoutTreeChange::LayoutAffecting) => false,
                },
                LayoutTreeChange::Identical => prior_stable,
                LayoutTreeChange::LayoutAffecting => false,
            },
            Some(LayoutTreeChange::Identical | LayoutTreeChange::LayoutAffecting) | None => false,
        };
    }

    /// Records a run-text edit as `VisualOnly`: a text change moves no other
    /// element, so `compute_panel_layouts` re-measures only the edited leaf and
    /// takes the geometry-stable skip when its box is unchanged. Combining with a
    /// same-frame `LayoutAffecting` change still resolves to a full solve.
    pub(crate) fn note_text_edit(&mut self) {
        self.record(LayoutTreeChange::VisualOnly);
        self.tree_visual_geometry_stable = false;
    }

    pub(super) fn take_with_tree_visual_geometry_stable(
        &mut self,
    ) -> (Option<LayoutTreeChange>, bool) {
        let pending = self.pending.take();
        let tree_visual_geometry_stable =
            pending == Some(LayoutTreeChange::VisualOnly) && self.tree_visual_geometry_stable;
        self.tree_visual_geometry_stable = false;
        (pending, tree_visual_geometry_stable)
    }

    pub(super) const fn pending(&self) -> Option<LayoutTreeChange> { self.pending }
}

/// Cached point-scaled layout tree for one [`DiegeticPanel`].
///
/// The source [`LayoutTree`] remains owned by [`DiegeticPanel`]. This component
/// stores the derived tree used by the layout engine after layout and font units
/// are converted to points.
#[derive(Component, Default)]
pub(super) struct ScaledLayoutTreeCache {
    source_revision:       TreeRevision,
    layout_to_points_bits: F32Bits,
    font_to_points_bits:   F32Bits,
    tree:                  Option<LayoutTree>,
    #[cfg(test)]
    hits:                  usize,
    #[cfg(test)]
    misses:                usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct F32Bits(u32);

impl F32Bits {
    const fn new(value: f32) -> Self { Self(value.to_bits()) }
}

impl ScaledLayoutTreeCache {
    /// Test helper that drops the source-tree-derived value while keeping the
    /// last source revision and scale keys.
    #[cfg(test)]
    pub(super) fn invalidate_source(&mut self) { self.tree = None; }

    /// Returns a point-scaled tree, rebuilding the cache when the source tree
    /// or scale factors change.
    pub(super) fn get_or_update(
        &mut self,
        source: &PanelTree,
        layout_to_points: f32,
        font_to_points: f32,
    ) -> &LayoutTree {
        let source_revision = source.revision();
        let layout_to_points_bits = F32Bits::new(layout_to_points);
        let font_to_points_bits = F32Bits::new(font_to_points);
        let cache_hit = self.tree.is_some()
            && self.source_revision == source_revision
            && self.layout_to_points_bits == layout_to_points_bits
            && self.font_to_points_bits == font_to_points_bits;

        if cache_hit {
            #[cfg(test)]
            {
                self.hits += 1;
            }
        } else {
            self.source_revision = source_revision;
            self.layout_to_points_bits = layout_to_points_bits;
            self.font_to_points_bits = font_to_points_bits;
            self.tree = None;
            #[cfg(test)]
            {
                self.misses += 1;
            }
        }

        self.tree
            .get_or_insert_with(|| source.tree().scaled(layout_to_points, font_to_points))
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
    draw_order:         DrawOrder,
    #[reflect(ignore)]
    field_records:      Vec<PanelFieldRecord>,
    #[reflect(ignore)]
    field_id_conflicts: Vec<crate::PanelElementId>,
    content_width:      f32,
    content_height:     f32,
}

impl ComputedDiegeticPanel {
    /// Actual computed panel-surface width in world units.
    ///
    /// This is the solved root element width, including root padding and
    /// border. With `Sizing::FIT`, this shrinks the panel viewport to the
    /// visible surface instead of clipping root chrome.
    #[must_use]
    pub const fn content_width(&self) -> f32 { self.content_width }

    /// Actual computed panel-surface height in world units.
    #[must_use]
    pub const fn content_height(&self) -> f32 { self.content_height }

    /// Returns the bounding box of the first child content in layout units, or
    /// `None` if layout has not yet been computed.
    #[must_use]
    pub fn content_bounds(&self) -> Option<BoundingBox> {
        self.result.as_ref().and_then(LayoutResult::content_bounds)
    }

    /// Returns computed editable fields in draw-independent element order.
    #[must_use]
    pub fn field_records(&self) -> &[PanelFieldRecord] { &self.field_records }

    /// Returns duplicated editable field ids found during the latest layout.
    #[must_use]
    pub fn field_id_conflicts(&self) -> &[crate::PanelElementId] { &self.field_id_conflicts }

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

    /// Returns the command-indexed `DrawOrder` for the latest result.
    #[must_use]
    pub(crate) const fn draw_order(&self) -> &DrawOrder { &self.draw_order }

    /// Regenerates `LayoutResult::commands` and keeps `DrawOrder` synchronized.
    ///
    /// Returns `false` when the panel has no computed `LayoutResult` yet.
    pub(super) fn regenerate_commands(&mut self, tree: &LayoutTree) -> bool {
        let Some(result) = self.result.as_mut() else {
            return false;
        };

        result.regenerate_commands(tree);
        self.draw_order = DrawOrder::from_commands(&result.commands);
        true
    }

    /// Stores the computed layout result.
    pub fn set_result(&mut self, result: LayoutResult) {
        self.draw_order = DrawOrder::from_commands(&result.commands);
        self.result = Some(result);
        self.field_records.clear();
        self.field_id_conflicts.clear();
    }

    pub(super) fn set_result_with_fields(
        &mut self,
        result: LayoutResult,
        field_records: Vec<PanelFieldRecord>,
        field_id_conflicts: Vec<crate::PanelElementId>,
    ) {
        self.draw_order = DrawOrder::from_commands(&result.commands);
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
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;
    use bevy::window::WindowRef;

    use super::CoordinateSpace;
    use super::DiegeticPanel;
    use super::DiegeticPanelChangeClassification;
    use super::DiegeticPanelCommands;
    use super::PanelScreenHandoff;
    use super::PanelTree;
    use super::PendingPanelConversion;
    use super::PreparedPanelScreenConversion;
    use super::SavedPanelWorldState;
    use super::ScaledLayoutTreeCache;
    use crate::Cascade;
    use crate::DiegeticTextMeasurer;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::Mm;
    use crate::PanelScreenConversion;
    use crate::TextStyle;
    use crate::Unit;
    use crate::layout::LayoutTreeChange;

    fn test_tree(text: &str) -> crate::LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text((text, TextStyle::new(10.0)));
        builder.build()
    }

    #[test]
    fn scaled_tree_cache_hits_until_source_invalidated_or_scale_changes() {
        let mut source = PanelTree::from(test_tree("cache"));
        let mut cache = ScaledLayoutTreeCache::default();

        let _ = cache.get_or_update(&source, 2.0, 3.0);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 1);

        let _ = cache.get_or_update(&source, 2.0, 3.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);

        cache.invalidate_source();
        let _ = cache.get_or_update(&source, 2.0, 3.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 2);

        source.replace(test_tree("cache"));
        let _ = cache.get_or_update(&source, 2.0, 3.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 3);

        let _ = cache.get_or_update(&source, 4.0, 3.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 4);

        let _ = cache.get_or_update(&source, 4.0, 5.0);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 5);
    }

    #[test]
    fn scaled_tree_cache_hits_across_unrelated_panel_component_changes() {
        let source = PanelTree::from(test_tree("cache"));
        let mut cache = ScaledLayoutTreeCache::default();

        let _ = cache.get_or_update(&source, 2.0, 3.0);
        let _ = cache.get_or_update(&source, 2.0, 3.0);

        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn precompose_helper_replace_invalidates_scaled_tree_cache() {
        let mut helper = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Millimeters)
            .world_height(0.5)
            .with_tree(test_tree("Blend"))
            .build()
            .expect("helper panel should build");
        let mut cache = ScaledLayoutTreeCache::default();

        let scaled = cache.get_or_update(helper.tree_source(), 1.0, 1.0);
        assert_eq!(scaled.element_text(1), Some("Blend"));
        assert_eq!(cache.misses(), 1);

        let next = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Millimeters)
            .world_height(0.5)
            .with_tree(test_tree("Add"))
            .build()
            .expect("replacement helper panel should build");

        helper.replace_from_precompose_helper(next);
        let scaled = cache.get_or_update(helper.tree_source(), 1.0, 1.0);

        assert_eq!(scaled.element_text(1), Some("Add"));
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 2);
    }

    #[test]
    fn same_frame_text_edit_clears_tree_visual_geometry_stability() {
        let mut classification = DiegeticPanelChangeClassification::default();

        classification.record_tree_change(LayoutTreeChange::VisualOnly);
        assert_eq!(
            classification.take_with_tree_visual_geometry_stable(),
            (Some(LayoutTreeChange::VisualOnly), true)
        );

        classification.record_tree_change(LayoutTreeChange::VisualOnly);
        classification.note_text_edit();

        assert_eq!(
            classification.take_with_tree_visual_geometry_stable(),
            (Some(LayoutTreeChange::VisualOnly), false)
        );
    }

    #[test]
    fn begin_screen_conversion_preserves_world_state_until_handoff() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(DiegeticTextMeasurer::default());
        app.add_plugins(HeadlessLayoutPlugin);
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Millimeters)
            .world_height(0.5)
            .with_tree(test_tree("prepare"))
            .build()
            .expect("world panel should build");
        let camera = app
            .world_mut()
            .spawn((
                Camera::default(),
                GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 5.0)),
            ))
            .id();
        let entity = app
            .world_mut()
            .spawn((panel, Transform::from_scale(Vec3::new(2.0, 2.0, 1.0))))
            .id();
        let conversion =
            PanelScreenConversion::at_pixels(Vec2::new(400.0, 300.0), Vec2::new(200.0, 100.0));

        app.world_mut()
            .commands()
            .begin_panel_to_screen(entity, camera, conversion.clone());
        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel should still exist");
        assert!(matches!(
            panel.coordinate_space(),
            CoordinateSpace::World { .. }
        ));
        assert_eq!(panel.width(), 200.0);
        assert_eq!(panel.height(), 100.0);
        assert_eq!(panel.layout_unit(), Unit::Pixels);
        assert_eq!(panel.font_unit(), Cascade::Override(Unit::Pixels));
        assert_eq!(panel.world_width(), 1.0);
        assert_eq!(panel.world_height(), 0.5);
        assert!(app.world().get::<SavedPanelWorldState>(entity).is_some());
        assert!(app.world().get::<PanelScreenHandoff>(entity).is_some());
        assert!(
            app.world()
                .get::<PreparedPanelScreenConversion>(entity)
                .is_some()
        );

        app.world_mut()
            .commands()
            .finish_panel_to_screen(entity, camera, conversion);
        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel should still exist");
        assert!(panel.coordinate_space().is_screen());
        assert!(
            app.world()
                .get::<PreparedPanelScreenConversion>(entity)
                .is_none()
        );
        assert!(app.world().get::<SavedPanelWorldState>(entity).is_some());
    }

    #[test]
    fn queued_screen_conversion_applies_in_panel_pipeline_and_resets_scale() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(DiegeticTextMeasurer::default());
        app.add_plugins(HeadlessLayoutPlugin);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (800_u32, 600_u32).into(),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Points)
            .world_height(0.5)
            .with_tree(test_tree("convert"))
            .build()
            .expect("world panel should build");
        let entity = app
            .world_mut()
            .spawn((panel, Transform::from_scale(Vec3::new(2.0, 3.0, 4.0))))
            .id();
        let conversion =
            PanelScreenConversion::at_pixels(Vec2::new(400.0, 300.0), Vec2::new(200.0, 100.0))
                .window(WindowRef::Entity(window));

        app.world_mut()
            .commands()
            .apply_panel_screen_conversion(entity, conversion);
        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel should still exist");
        assert!(panel.coordinate_space().is_screen());
        assert!(app.world().get::<PendingPanelConversion>(entity).is_none());
        let transform = app
            .world()
            .get::<Transform>(entity)
            .expect("panel transform should still exist");
        assert_eq!(transform.translation, Vec3::ZERO);
        assert_eq!(transform.rotation, Quat::IDENTITY);
        assert_eq!(transform.scale, Vec3::ONE);
    }

    #[cfg(feature = "bench_support")]
    #[test]
    fn tree_revision_changes_only_when_tree_is_replaced() {
        let mut panel = DiegeticPanel::default();
        assert_eq!(u64::from(panel.tree_revision()), 0);

        panel.set_width(120.0);
        panel.set_height(80.0);
        assert_eq!(u64::from(panel.tree_revision()), 0);

        let resize_result = panel.set_size((2.0, 1.0));
        assert!(resize_result.is_ok());
        assert_eq!(u64::from(panel.tree_revision()), 0);

        panel.set_tree_full_rebuild(test_tree("next"));
        assert_eq!(u64::from(panel.tree_revision()), 1);
    }

    #[test]
    fn builder_panels_start_at_tree_revision_zero() {
        let panel = DiegeticPanel::world()
            .size(1.0, 0.5)
            .with_tree(test_tree("builder"))
            .build()
            .expect("test panel should build");

        assert_eq!(u64::from(panel.tree_revision()), 0);
    }
}

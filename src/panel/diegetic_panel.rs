//! [`DiegeticPanel`] — the main panel component and its computed companion.

use bevy::prelude::*;

use super::builder::DiegeticPanelBuilder;
use super::builder::NeedsSize;
use super::builder::Screen;
use super::builder::World;
use super::modes::PanelMode;
use super::modes::RenderMode;
use super::modes::SurfaceShadow;
use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::InvalidSize;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::PanelSize;
use crate::layout::Unit;
use crate::layout::UnitConfig;

/// A diegetic UI panel attached to a 3D entity.
///
/// Defines a layout tree and the panel's dimensions in layout units.
/// World-space size is computed automatically from the `layout_unit`
/// (or the global [`UnitConfig`] default). Font sizes in the tree are
/// interpreted in `font_unit` (defaults to [`Unit::Points`]).
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
#[require(ComputedDiegeticPanel, Transform, Visibility)]
pub struct DiegeticPanel {
    /// The layout tree defining this panel's UI structure.
    #[reflect(ignore)]
    pub(super) tree:            LayoutTree,
    /// Panel width in layout units. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    pub(super) width:           f32,
    /// Panel height in layout units. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    pub(super) height:          f32,
    /// Unit for `width`/`height`. Set automatically by
    /// [`DiegeticPanelBuilder::size`] or [`set_size`](Self::set_size).
    pub(super) layout_unit:     Unit,
    /// Unit for font sizes in the layout tree. `None` inherits from [`UnitConfig::font`].
    pub(super) font_unit:       Option<Unit>,
    /// Which point on the panel sits at the entity's [`Transform`] position.
    /// Defaults to [`Anchor::TopLeft`].
    pub(super) anchor:          Anchor,
    /// Target world width in meters. When set, the panel is uniformly scaled
    /// so its width matches this value (height follows aspect ratio).
    /// If both `world_width` and `world_height` are set, non-uniform scaling
    /// is applied.
    pub(super) world_width:     Option<f32>,
    /// Target world height in meters. When set, the panel is uniformly scaled
    /// so its height matches this value (width follows aspect ratio).
    pub(super) world_height:    Option<f32>,
    /// How the panel renders its content. Defaults to [`RenderMode::Geometry`].
    pub(super) render_mode:     RenderMode,
    /// Whether the panel surface casts 3D shadows. Defaults to [`SurfaceShadow::Off`].
    /// Text shadow casting is controlled per-element via `GlyphShadowMode`.
    pub(super) surface_shadow:  SurfaceShadow,
    /// Default PBR material for backgrounds and borders. When `None`, the
    /// library uses a matte default (roughness 0.95, reflectance 0.02).
    /// Individual elements can override via `El::material`.
    /// `base_color` is overridden by the layout color when both are set.
    #[reflect(ignore)]
    pub(super) material:        Option<StandardMaterial>,
    /// Default PBR material for text. When `None`, uses the same default as
    /// `material`. Individual text elements can override.
    /// `base_color` is overridden by `LayoutTextStyle::color` when set.
    #[reflect(ignore)]
    pub(super) text_material:   Option<StandardMaterial>,
    /// Panel-level override for text [`AlphaMode`]. When `None`, the resolution
    /// falls through to the per-style setting and then to `TextAlphaModeDefault`.
    pub(super) text_alpha_mode: Option<AlphaMode>,
    /// Whether the panel is world-space or screen-space.
    pub(super) mode:            PanelMode,
}

impl Default for DiegeticPanel {
    fn default() -> Self {
        Self {
            tree:            LayoutTree::default(),
            width:           0.0,
            height:          0.0,
            layout_unit:     Unit::Meters,
            font_unit:       None,
            anchor:          Anchor::TopLeft,
            world_width:     None,
            world_height:    None,
            render_mode:     RenderMode::Geometry,
            surface_shadow:  SurfaceShadow::Off,
            material:        None,
            text_material:   None,
            text_alpha_mode: None,
            mode:            PanelMode::World,
        }
    }
}

// ── Public read-only accessors ──────────────────────────────────────────────

impl DiegeticPanel {
    /// Returns a reference to the layout tree.
    #[must_use]
    pub const fn tree(&self) -> &LayoutTree { &self.tree }

    /// Panel width in layout units.
    #[must_use]
    pub const fn width(&self) -> f32 { self.width }

    /// Panel height in layout units.
    #[must_use]
    pub const fn height(&self) -> f32 { self.height }

    /// The layout unit for this panel's dimensions.
    #[must_use]
    pub const fn layout_unit(&self) -> Unit { self.layout_unit }

    /// The font unit override, or `None` if inheriting from [`UnitConfig`].
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

    /// The panel's mode (world or screen).
    #[must_use]
    pub const fn mode(&self) -> &PanelMode { &self.mode }
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

    /// Replaces the layout tree.
    pub fn set_tree(&mut self, tree: LayoutTree) { self.tree = tree; }

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
    /// Returns the layout unit.
    pub(crate) const fn resolved_layout_unit(&self, _config: &UnitConfig) -> Unit {
        self.layout_unit
    }

    /// Resolves the font unit, falling back to the global [`UnitConfig`].
    pub(crate) fn resolved_font_unit(&self, config: &UnitConfig) -> Unit {
        self.font_unit.unwrap_or(config.font)
    }

    /// Physical width in meters before world scaling.
    fn physical_width(&self, config: &UnitConfig) -> f32 {
        self.width * self.resolved_layout_unit(config).meters_per_unit()
    }

    /// Physical height in meters before world scaling.
    fn physical_height(&self, config: &UnitConfig) -> f32 {
        self.height * self.resolved_layout_unit(config).meters_per_unit()
    }

    /// Panel width in meters (world units), incorporating `world_width`
    /// and `world_height` scaling.
    ///
    /// - `world_width` set: returns `world_width`.
    /// - `world_height` only: uniform scale from height, width follows aspect ratio.
    /// - Neither: physical size from layout units.
    #[must_use]
    pub fn world_width(&self, config: &UnitConfig) -> f32 {
        let physical_width = self.physical_width(config);
        let physical_height = self.physical_height(config);
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
    pub fn world_height(&self, config: &UnitConfig) -> f32 {
        let physical_width = self.physical_width(config);
        let physical_height = self.physical_height(config);
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
    /// where `scale` is `points_mpu`.
    #[must_use]
    pub fn anchor_offsets(&self, config: &UnitConfig) -> (f32, f32) {
        let w = self.world_width(config);
        let h = self.world_height(config);
        let (fx, fy) = self.anchor.offset_fraction();
        (w * fx, h * fy)
    }

    /// Conversion factor from layout points to world meters.
    ///
    /// The layout engine works in points internally. Multiply a layout-space
    /// value (in points) by this factor to get world meters. Incorporates
    /// `world_width`/`world_height` scaling.
    #[must_use]
    pub fn points_to_world(&self, config: &UnitConfig) -> f32 {
        let layout_unit = self.resolved_layout_unit(config);
        let viewport_pts_h = self.height * layout_unit.to_points();
        let wh = self.world_height(config);
        if viewport_pts_h > 0.0 {
            wh / viewport_pts_h
        } else {
            Unit::Points.meters_per_unit()
        }
    }

    /// Font-to-layout conversion factor for this panel.
    ///
    /// Multiply a font size by this value to convert from font units
    /// to layout units.
    #[must_use]
    pub fn font_scale(&self, config: &UnitConfig) -> f32 {
        let font_mpu = self.resolved_font_unit(config).meters_per_unit();
        let layout_mpu = self.resolved_layout_unit(config).meters_per_unit();
        font_mpu / layout_mpu
    }
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

    /// Stores the computed layout result.
    pub fn set_result(&mut self, result: LayoutResult) { self.result = Some(result); }

    /// Sets the content dimensions in world units.
    pub const fn set_content_size(&mut self, width: f32, height: f32) {
        self.content_width = width;
        self.content_height = height;
    }
}

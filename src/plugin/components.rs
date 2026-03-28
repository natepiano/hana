//! Components and resources for diegetic UI panels.

use std::sync::Arc;

use bevy::prelude::*;

use super::config::PanelSize;
use super::config::UnitConfig;
use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::LayoutBuilder;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::MeasureTextFn;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;
use crate::layout::Unit;

/// How the panel's visual content is rendered to the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum RenderMode {
    /// Render-to-texture: all content composited into an offscreen texture,
    /// displayed as a single textured quad. Fixed resolution, one draw call.
    #[default]
    Texture,
    /// Direct 3D geometry: backgrounds, borders, and text rendered as
    /// separate meshes in the scene. Infinite resolution, multiple draw
    /// calls. Layer ordering uses `depth_bias` on the transparent sort key.
    Geometry,
}

/// A diegetic UI panel attached to a 3D entity.
///
/// Defines a layout tree and the panel's dimensions in layout units.
/// World-space size is computed automatically from the `layout_unit`
/// (or the global [`UnitConfig`] default). Font sizes in the tree are
/// interpreted in `font_unit` (defaults to [`Unit::Points`]).
///
/// Use [`DiegeticPanel::builder`] for ergonomic construction:
///
/// ```ignore
/// commands.spawn(
///     DiegeticPanel::builder()
///         .size(PaperSize::USLetter)
///         .layout_unit(Unit::Points)
///         .world_height(3.1)
///         .layout(|b| {
///             b.text("Hello", LayoutTextStyle::new(48.0));
///         })
///         .build()
/// );
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
    pub tree:         LayoutTree,
    /// Panel width in layout units.
    pub width:        f32,
    /// Panel height in layout units.
    pub height:       f32,
    /// Unit for `width`/`height`. `None` inherits from [`UnitConfig::layout`].
    pub layout_unit:  Option<Unit>,
    /// Unit for font sizes in the layout tree. `None` inherits from [`UnitConfig::font`].
    pub font_unit:    Option<Unit>,
    /// Which point on the panel sits at the entity's [`Transform`] position.
    /// Defaults to [`Anchor::TopLeft`].
    pub anchor:       Anchor,
    /// Target world width in meters. When set, the panel is uniformly scaled
    /// so its width matches this value (height follows aspect ratio).
    /// If both `world_width` and `world_height` are set, non-uniform scaling
    /// is applied.
    pub world_width:  Option<f32>,
    /// Target world height in meters. When set, the panel is uniformly scaled
    /// so its height matches this value (width follows aspect ratio).
    pub world_height: Option<f32>,
    /// How the panel renders its content. Defaults to [`RenderMode::Texture`].
    pub render_mode:  RenderMode,
}

impl Default for DiegeticPanel {
    fn default() -> Self {
        Self {
            tree:         LayoutTree::default(),
            width:        0.0,
            height:       0.0,
            layout_unit:  None,
            font_unit:    None,
            anchor:       Anchor::TopLeft,
            world_width:  None,
            world_height: None,
            render_mode:  RenderMode::Texture,
        }
    }
}

impl DiegeticPanel {
    /// Returns a builder for ergonomic panel construction.
    #[must_use]
    pub fn builder() -> DiegeticPanelBuilder { DiegeticPanelBuilder::default() }
}

/// Builder for [`DiegeticPanel`].
///
/// Eliminates the need to specify layout dimensions twice (once for the
/// `LayoutBuilder`, once for the panel). The `.layout()` closure receives
/// a pre-sized `LayoutBuilder`.
///
/// # Example
///
/// ```ignore
/// DiegeticPanel::builder()
///     .size(PaperSize::A4)
///     .layout_unit(Unit::Millimeters)
///     .world_height(0.5)
///     .layout(|b| {
///         b.with(El::new()..., |b| { ... });
///     })
///     .build()
/// ```
#[derive(Default)]
pub struct DiegeticPanelBuilder {
    width:        f32,
    height:       f32,
    layout_unit:  Option<Unit>,
    font_unit:    Option<Unit>,
    anchor:       Option<Anchor>,
    world_width:  Option<f32>,
    world_height: Option<f32>,
    render_mode:  RenderMode,
    tree:         Option<LayoutTree>,
}

impl DiegeticPanelBuilder {
    /// Sets the panel dimensions. Accepts [`PaperSize`], or a tuple of
    /// values convertible to `f32` (e.g., `(Pt(612.0), Pt(792.0))`).
    #[must_use]
    pub fn size(mut self, size: impl PanelSize) -> Self {
        let (w, h) = size.dimensions();
        self.width = w;
        self.height = h;
        self
    }

    /// Builds the layout tree from a closure. The closure receives a
    /// [`LayoutBuilder`] pre-configured with the panel's dimensions.
    #[must_use]
    pub fn layout(mut self, f: impl FnOnce(&mut LayoutBuilder)) -> Self {
        let mut builder = LayoutBuilder::new(self.width, self.height);
        f(&mut builder);
        self.tree = Some(builder.build());
        self
    }

    /// Overrides the layout unit (default inherits from [`UnitConfig::layout`]).
    #[must_use]
    pub const fn layout_unit(mut self, unit: Unit) -> Self {
        self.layout_unit = Some(unit);
        self
    }

    /// Overrides the font unit (default inherits from [`UnitConfig::font`]).
    #[must_use]
    pub const fn font_unit(mut self, unit: Unit) -> Self {
        self.font_unit = Some(unit);
        self
    }

    /// Sets the anchor point (default [`Anchor::TopLeft`]).
    #[must_use]
    pub const fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Scales the panel uniformly so its world width matches this value in meters.
    /// Height follows the aspect ratio.
    #[must_use]
    pub const fn world_width(mut self, meters: f32) -> Self {
        self.world_width = Some(meters);
        self
    }

    /// Scales the panel uniformly so its world height matches this value in meters.
    /// Width follows the aspect ratio.
    #[must_use]
    pub const fn world_height(mut self, meters: f32) -> Self {
        self.world_height = Some(meters);
        self
    }

    /// Sets the rendering mode. Defaults to [`RenderMode::Texture`].
    #[must_use]
    pub const fn render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Consumes the builder and returns a [`DiegeticPanel`] component.
    #[must_use]
    pub fn build(self) -> DiegeticPanel {
        DiegeticPanel {
            tree:         self.tree.unwrap_or_default(),
            width:        self.width,
            height:       self.height,
            layout_unit:  self.layout_unit,
            font_unit:    self.font_unit,
            anchor:       self.anchor.unwrap_or(Anchor::TopLeft),
            world_width:  self.world_width,
            world_height: self.world_height,
            render_mode:  self.render_mode,
        }
    }
}

impl DiegeticPanel {
    /// Resolves the layout unit, falling back to the global [`UnitConfig`].
    fn resolved_layout_unit(&self, config: &UnitConfig) -> Unit {
        self.layout_unit.unwrap_or(config.layout)
    }

    /// Resolves the font unit, falling back to the global [`UnitConfig`].
    fn resolved_font_unit(&self, config: &UnitConfig) -> Unit {
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
        let phys_w = self.physical_width(config);
        let phys_h = self.physical_height(config);
        match (self.world_width, self.world_height) {
            (Some(ww), _) => ww,
            (None, Some(wh)) => {
                if phys_h > 0.0 {
                    phys_w * (wh / phys_h)
                } else {
                    phys_w
                }
            },
            (None, None) => phys_w,
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
        let phys_w = self.physical_width(config);
        let phys_h = self.physical_height(config);
        match (self.world_width, self.world_height) {
            (_, Some(wh)) => wh,
            (Some(ww), None) => {
                if phys_w > 0.0 {
                    phys_h * (ww / phys_w)
                } else {
                    phys_h
                }
            },
            (None, None) => phys_h,
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

/// Hue rotation applied to all text in a panel, in radians.
///
/// Attach to the same entity as a [`DiegeticPanel`] to rotate the hue of
/// every vertex color in the panel's text mesh. This is a GPU-side effect —
/// changing it does not trigger layout recomputation or mesh rebuilds.
///
/// Individual text elements retain their per-element colors set via
/// [`TextConfig::with_color`]. This rotation shifts all of them by the
/// same amount. A value of `TAU / 3` (~2.09) shifts reds to greens,
/// greens to blues, etc. A full `TAU` (6.28) cycles back to the original
/// colors.
///
/// See the `text_stress` example for usage.
#[derive(Component, Default, Clone, Copy, Debug, Reflect)]
pub struct HueOffset(pub f32);

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
    // ── Public read-only accessors ───────────────────────────────────────

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

    // ── Crate-internal accessors ─────────────────────────────────────────

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

/// Resource providing text measurement for layout computation.
///
/// Insert this resource before adding [`DiegeticUiPlugin`] to override the
/// default monospace approximation with a real text measurement function.
///
/// The default measurer estimates text dimensions using a fixed character
/// width (60% of font size). For accurate measurement backed by real font
/// shaping, the plugin automatically replaces this with a parley-backed
/// measurer when [`DiegeticUiPlugin`] is added.
///
/// Custom measurers are useful when bridging to external layout engines
/// that need text measurement callbacks. See the `side_by_side` example
/// for a real-world case where clay-layout delegates measurement through
/// this interface.
///
/// # Example
///
/// ```ignore
/// app.insert_resource(DiegeticTextMeasurer {
///     measure_fn: Arc::new(|text, measure| {
///         // Custom measurement logic here.
///         TextDimensions { width: 100.0, height: 12.0 }
///     }),
/// });
/// ```
#[derive(Resource)]
pub struct DiegeticTextMeasurer {
    /// The measurement function. Takes a text string and a [`TextMeasure`]
    /// describing the font configuration, returns [`TextDimensions`].
    pub measure_fn: MeasureTextFn,
}

impl Default for DiegeticTextMeasurer {
    fn default() -> Self {
        Self {
            measure_fn: Arc::new(|text: &str, measure: &TextMeasure| {
                let char_width = measure.size * 0.6;
                #[allow(clippy::cast_precision_loss)]
                let width = char_width * text.len() as f32;
                TextDimensions {
                    width,
                    height: measure.size,
                    line_height: measure.size,
                }
            }),
        }
    }
}

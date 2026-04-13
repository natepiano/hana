//! Components and resources for diegetic UI panels.

use std::sync::Arc;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::config::PanelSize;
use super::config::UnitConfig;
use crate::constants::MONOSPACE_WIDTH_RATIO;
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
    ///
    /// Text is rasterized to the intermediate texture and resampled on
    /// display, which causes visible softness compared to [`Geometry`] mode.
    /// Use only when a single draw call is required or when the panel is
    /// viewed at a distance where per-glyph MSDF meshes are unnecessary.
    Texture,
    /// Direct 3D geometry: backgrounds, borders, and text rendered as
    /// separate meshes in the scene. Infinite resolution, multiple draw
    /// calls. Layer ordering uses `depth_bias` on the transparent sort key.
    #[default]
    Geometry,
}

/// Whether the panel's surface geometry casts 3D shadows.
///
/// "Surface" means backgrounds, borders, and the RTT display quad — the
/// structural parts of the panel. Text shadow casting is controlled
/// independently per text element via [`GlyphShadowMode`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum SurfaceShadow {
    /// Surface geometry does not cast shadows (default).
    #[default]
    Off,
    /// Surface geometry participates in shadow casting.
    On,
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
    pub tree:           LayoutTree,
    /// Panel width in layout units.
    pub width:          f32,
    /// Panel height in layout units.
    pub height:         f32,
    /// Unit for `width`/`height`. `None` inherits from [`UnitConfig::layout`].
    pub layout_unit:    Option<Unit>,
    /// Unit for font sizes in the layout tree. `None` inherits from [`UnitConfig::font`].
    pub font_unit:      Option<Unit>,
    /// Which point on the panel sits at the entity's [`Transform`] position.
    /// Defaults to [`Anchor::TopLeft`].
    pub anchor:         Anchor,
    /// Target world width in meters. When set, the panel is uniformly scaled
    /// so its width matches this value (height follows aspect ratio).
    /// If both `world_width` and `world_height` are set, non-uniform scaling
    /// is applied.
    pub world_width:    Option<f32>,
    /// Target world height in meters. When set, the panel is uniformly scaled
    /// so its height matches this value (width follows aspect ratio).
    pub world_height:   Option<f32>,
    /// How the panel renders its content. Defaults to [`RenderMode::Geometry`].
    pub render_mode:    RenderMode,
    /// Whether the panel surface casts 3D shadows. Defaults to [`SurfaceShadow::Off`].
    /// Text shadow casting is controlled per-element via [`GlyphShadowMode`].
    pub surface_shadow: SurfaceShadow,
    /// Default PBR material for backgrounds and borders. When `None`, the
    /// library uses a matte default (roughness 0.95, reflectance 0.02).
    /// Individual elements can override via [`El::material`].
    /// `base_color` is overridden by the layout color when both are set.
    #[reflect(ignore)]
    pub material:       Option<StandardMaterial>,
    /// Default PBR material for text. When `None`, uses the same default as
    /// `material`. Individual text elements can override.
    /// `base_color` is overridden by [`LayoutTextStyle::color`] when set.
    #[reflect(ignore)]
    pub text_material:  Option<StandardMaterial>,
}

impl Default for DiegeticPanel {
    fn default() -> Self {
        Self {
            tree:           LayoutTree::default(),
            width:          0.0,
            height:         0.0,
            layout_unit:    None,
            font_unit:      None,
            anchor:         Anchor::TopLeft,
            world_width:    None,
            world_height:   None,
            render_mode:    RenderMode::Geometry,
            surface_shadow: SurfaceShadow::Off,
            material:       None,
            text_material:  None,
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
    width:          f32,
    height:         f32,
    layout_unit:    Option<Unit>,
    font_unit:      Option<Unit>,
    anchor:         Option<Anchor>,
    world_width:    Option<f32>,
    world_height:   Option<f32>,
    render_mode:    RenderMode,
    surface_shadow: SurfaceShadow,
    material:       Option<StandardMaterial>,
    text_material:  Option<StandardMaterial>,
    tree:           Option<LayoutTree>,
    screen_space:   Option<ScreenSpace>,
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

    /// Sets the rendering mode. Defaults to [`RenderMode::Geometry`].
    #[must_use]
    pub const fn render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Sets whether the panel surface casts 3D shadows.
    /// Defaults to [`SurfaceShadow::Off`].
    #[must_use]
    pub const fn surface_shadow(mut self, shadow: SurfaceShadow) -> Self {
        self.surface_shadow = shadow;
        self
    }

    /// Sets the default PBR material for backgrounds and borders.
    ///
    /// `base_color` is overridden by the layout color when both are set.
    /// Individual elements can override via [`El::material`].
    #[must_use]
    pub fn material(mut self, material: StandardMaterial) -> Self {
        self.material = Some(material);
        self
    }

    /// Sets the default PBR material for text.
    ///
    /// `base_color` is overridden by [`LayoutTextStyle::color`] when set.
    #[must_use]
    pub fn text_material(mut self, material: StandardMaterial) -> Self {
        self.text_material = Some(material);
        self
    }

    /// Sets the panel dimensions in logical pixels and uses [`Unit::Points`]
    /// as the layout unit (1 pt ≈ 1 px in the layout engine).
    ///
    /// For [`ScreenSpace`] panels, these map 1:1 to on-screen pixels.
    /// For world-space panels, the system can resolve pixel dimensions
    /// per-frame using the active camera's projection.
    #[must_use]
    pub const fn size_px(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self.layout_unit = Some(Unit::Pixels);
        self
    }

    /// Stores a custom [`ScreenSpace`] configuration for use with
    /// [`build_screen_space`](Self::build_screen_space).
    ///
    /// If not called, [`build_screen_space`](Self::build_screen_space)
    /// uses [`ScreenSpace::default`].
    #[must_use]
    pub fn screen_space_with(mut self, config: ScreenSpace) -> Self {
        self.screen_space = Some(config);
        self
    }

    /// Places the panel at an explicit pixel position (top-left origin, y-down).
    /// The panel's [`Anchor`] determines which point sits at this position.
    #[must_use]
    pub fn screen_position(mut self, x: f32, y: f32) -> Self {
        let ss = self.screen_space.get_or_insert_with(ScreenSpace::default);
        ss.position = ScreenPosition::At(Vec2::new(x, y));
        self
    }

    /// Panel width fills a fraction of the window (0.0–1.0).
    #[must_use]
    pub fn width_percent(mut self, fraction: f32) -> Self {
        let ss = self.screen_space.get_or_insert_with(ScreenSpace::default);
        ss.width = Some(ScreenDimension::Percent(fraction));
        self
    }

    /// Panel height fills a fraction of the window (0.0–1.0).
    #[must_use]
    pub fn height_percent(mut self, fraction: f32) -> Self {
        let ss = self.screen_space.get_or_insert_with(ScreenSpace::default);
        ss.height = Some(ScreenDimension::Percent(fraction));
        self
    }

    /// Panel width is a fixed pixel value, managed by the plugin.
    #[must_use]
    pub fn width_px(mut self, pixels: f32) -> Self {
        let ss = self.screen_space.get_or_insert_with(ScreenSpace::default);
        ss.width = Some(ScreenDimension::Fixed(pixels));
        self
    }

    /// Panel height is a fixed pixel value, managed by the plugin.
    #[must_use]
    pub fn height_px(mut self, pixels: f32) -> Self {
        let ss = self.screen_space.get_or_insert_with(ScreenSpace::default);
        ss.height = Some(ScreenDimension::Fixed(pixels));
        self
    }

    /// Consumes the builder and returns a `(DiegeticPanel, ScreenSpace)` tuple.
    ///
    /// The tuple is a valid Bevy bundle — pass it directly to
    /// `commands.spawn(...)`. Sets `layout_unit` to [`Unit::Pixels`] and
    /// `world_height` to match the panel height so that `points_to_world`
    /// equals 1.0 (1 layout point = 1 world unit = 1 screen pixel under
    /// the orthographic overlay camera).
    ///
    /// When [`ScreenDimension::Percent`] is used for width or height,
    /// the layout tree's root element is automatically set to
    /// `Sizing::GROW` on that axis. This means the plugin can resize
    /// the panel by updating `panel.width`/`panel.height` and the
    /// layout engine reflows children without a tree rebuild. If you
    /// rebuild the tree for state changes, use
    /// [`LayoutBuilder::with_root`] with a `Sizing::GROW` root so
    /// the rebuilt tree also reflows correctly on resize.
    ///
    /// # Example
    ///
    /// ```ignore
    /// commands.spawn(
    ///     DiegeticPanel::builder()
    ///         .size_px(400.0, 300.0)
    ///         .layout(|b| {
    ///             b.text("Score: 999", LayoutTextStyle::new(24.0));
    ///         })
    ///         .build_screen_space()
    /// );
    /// ```
    #[must_use]
    pub fn build_screen_space(mut self) -> (DiegeticPanel, ScreenSpace) {
        let screen_space = self.screen_space.take().unwrap_or_default();
        // Ensure layout unit is pixel-compatible.
        if self.layout_unit.is_none() {
            self.layout_unit = Some(Unit::Pixels);
        }
        // Set world_height = height so points_to_world = 1.0.
        // (viewport_pts_h = height * to_points() = height * 1.0 = height,
        //  so points_to_world = world_height / viewport_pts_h = height / height = 1.0)
        if self.world_height.is_none() && self.world_width.is_none() {
            self.world_height = Some(self.height);
        }
        // For percent-sized axes, set the root element to GROW so that
        // changing panel.width/height reflows without a tree rebuild.
        if let Some(ref mut tree) = self.tree {
            if matches!(screen_space.width, Some(ScreenDimension::Percent(_))) {
                tree.set_root_grow_width();
            }
            if matches!(screen_space.height, Some(ScreenDimension::Percent(_))) {
                tree.set_root_grow_height();
            }
        }
        (self.build(), screen_space)
    }

    /// Consumes the builder and returns a [`DiegeticPanel`] component.
    #[must_use]
    pub fn build(self) -> DiegeticPanel {
        DiegeticPanel {
            tree:           self.tree.unwrap_or_default(),
            width:          self.width,
            height:         self.height,
            layout_unit:    self.layout_unit,
            font_unit:      self.font_unit,
            anchor:         self.anchor.unwrap_or(Anchor::TopLeft),
            world_width:    self.world_width,
            world_height:   self.world_height,
            render_mode:    self.render_mode,
            surface_shadow: self.surface_shadow,
            material:       self.material,
            text_material:  self.text_material,
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

/// Marker component that renders a [`DiegeticPanel`] in screen space.
///
/// When attached, the plugin spawns a dedicated orthographic camera
/// (1 world unit = 1 logical pixel) on a separate [`RenderLayers`] layer,
/// plus a directional light on the same layer. The panel renders as a 2D
/// overlay on top of the 3D scene, with layout units mapping 1:1 to
/// screen pixels.
///
/// Use [`DiegeticPanelBuilder::build_screen_space`] to construct a panel
/// with this component already attached.
///
/// # Camera order
///
/// `camera_order` controls rendering priority relative to other cameras.
/// The default (`1`) renders after the typical scene camera (`0`). Override
/// if your app already uses camera order 1 for something else.
///
/// # Render layer
///
/// How a screen-space panel derives its size along one axis from the window.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum ScreenDimension {
    /// Explicit pixel size. The panel's width/height is set to this value
    /// regardless of window size.
    Fixed(f32),
    /// Fraction of the window along this axis (0.0–1.0).
    /// `Percent(1.0)` fills the full window width or height.
    ///
    /// When used with [`DiegeticPanelBuilder::build_screen_space`], the
    /// layout tree's root element is automatically set to `Sizing::GROW`
    /// on the percent-sized axis. This allows the layout engine to reflow
    /// children when `panel.width`/`panel.height` changes — no tree
    /// rebuild is needed for pure resize.
    Percent(f32),
}

/// Where a screen-space panel is placed within the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub enum ScreenPosition {
    /// Pin to the window edge/corner that matches the panel's [`Anchor`].
    /// `Anchor::TopLeft` pins to the window's top-left corner,
    /// `Anchor::Center` pins to the window's center, etc.
    #[default]
    Screen,
    /// Place at an explicit pixel position (top-left origin, y-down).
    /// The panel's [`Anchor`] determines which point of the panel sits
    /// at this position.
    At(Vec2),
}

/// Marks a panel for screen-space rendering with an orthographic overlay camera.
///
/// The plugin automatically positions and (optionally) sizes the panel each
/// frame based on window dimensions, the panel's [`Anchor`], and the fields
/// below. No per-example update system is needed for positioning or resize.
///
/// `render_layer` isolates the panel from the scene camera. The default
/// (`31`) uses the highest standard Bevy render layer to minimize
/// collisions with user-defined layers.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct ScreenSpace {
    /// Camera render order. Matches [`Camera::order`] (`isize`).
    /// Higher orders render on top. Default: `1`.
    pub camera_order:  isize,
    /// Render layers for isolation from the scene camera.
    /// Default: `RenderLayers::layer(31)`.
    pub render_layers: RenderLayers,
    /// Where to place the panel within the window.
    /// Default: [`ScreenPosition::Screen`] (pin to the panel's anchor corner).
    pub position:      ScreenPosition,
    /// How to derive panel width from the window.
    /// `None` means the panel keeps whatever width was set at spawn time.
    pub width:         Option<ScreenDimension>,
    /// How to derive panel height from the window.
    /// `None` means the panel keeps its spawn-time height.
    pub height:        Option<ScreenDimension>,
}

/// Default camera order for [`ScreenSpace`] overlay cameras.
const DEFAULT_SCREEN_SPACE_CAMERA_ORDER: isize = 1;

/// Default render layer for [`ScreenSpace`] panels.
const DEFAULT_SCREEN_SPACE_RENDER_LAYER: usize = 31;

impl Default for ScreenSpace {
    fn default() -> Self {
        Self {
            camera_order:  DEFAULT_SCREEN_SPACE_CAMERA_ORDER,
            render_layers: RenderLayers::layer(DEFAULT_SCREEN_SPACE_RENDER_LAYER),
            position:      ScreenPosition::default(),
            width:         None,
            height:        None,
        }
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
                let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
                let width = char_width * text.len().to_f32();
                TextDimensions {
                    width,
                    height: measure.size,
                    line_height: measure.size,
                }
            }),
        }
    }
}

//! Components and resources for diegetic UI panels.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::constants::DEFAULT_SCREEN_SPACE_CAMERA_ORDER;
use super::constants::DEFAULT_SCREEN_SPACE_RENDER_LAYER;
use super::config::DimensionMatch;
use super::config::InvalidSize;
use super::config::PanelSize;
use super::config::PaperSize;
use super::config::Pt;
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

/// How a screen-space panel derives its size along one axis from the window.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum ScreenDimension {
    /// Explicit pixel size. The panel's width/height is set to this value
    /// regardless of window size.
    Fixed(f32),
    /// Fraction of the window along this axis (0.0–1.0).
    /// `Percent(1.0)` fills the full window width or height.
    ///
    /// When used, the layout tree's root element is automatically set to
    /// `Sizing::GROW` on the percent-sized axis. This allows the layout
    /// engine to reflow children when the panel resizes — no tree rebuild
    /// is needed for pure resize.
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

/// Whether the panel lives in 3D world space or as a 2D screen overlay.
///
/// `World` panels are positioned and scaled in 3D space.
/// `Screen` panels render via an orthographic overlay camera.
#[derive(Clone, Debug, Default, Reflect)]
pub enum PanelMode {
    /// Panel lives in 3D world space.
    #[default]
    World,
    /// Panel renders as a 2D screen overlay.
    Screen {
        /// Where to place the panel within the window.
        position:      ScreenPosition,
        /// How to derive panel width from the window.
        /// `None` keeps the panel's spawn-time width.
        width:         Option<ScreenDimension>,
        /// How to derive panel height from the window.
        /// `None` keeps the panel's spawn-time height.
        height:        Option<ScreenDimension>,
        /// Camera render order. Higher orders render on top. Default: `1`.
        camera_order:  isize,
        /// Render layers for isolation from the scene camera.
        /// Default: `RenderLayers::layer(31)`.
        render_layers: RenderLayers,
    },
}

impl PanelMode {
    /// Returns `true` if this is a screen-space panel.
    #[must_use]
    pub const fn is_screen(&self) -> bool { matches!(self, Self::Screen { .. }) }
}

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
    tree:           LayoutTree,
    /// Panel width in layout units. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    width:          f32,
    /// Panel height in layout units. Prefer [`set_size`](Self::set_size) for
    /// mutation to keep dimensions and unit in sync.
    height:         f32,
    /// Unit for `width`/`height`. Set automatically by
    /// [`DiegeticPanelBuilder::size`] or [`set_size`](Self::set_size).
    layout_unit:    Unit,
    /// Unit for font sizes in the layout tree. `None` inherits from [`UnitConfig::font`].
    font_unit:      Option<Unit>,
    /// Which point on the panel sits at the entity's [`Transform`] position.
    /// Defaults to [`Anchor::TopLeft`].
    anchor:         Anchor,
    /// Target world width in meters. When set, the panel is uniformly scaled
    /// so its width matches this value (height follows aspect ratio).
    /// If both `world_width` and `world_height` are set, non-uniform scaling
    /// is applied.
    world_width:    Option<f32>,
    /// Target world height in meters. When set, the panel is uniformly scaled
    /// so its height matches this value (width follows aspect ratio).
    world_height:   Option<f32>,
    /// How the panel renders its content. Defaults to [`RenderMode::Geometry`].
    render_mode:    RenderMode,
    /// Whether the panel surface casts 3D shadows. Defaults to [`SurfaceShadow::Off`].
    /// Text shadow casting is controlled per-element via [`GlyphShadowMode`].
    surface_shadow: SurfaceShadow,
    /// Default PBR material for backgrounds and borders. When `None`, the
    /// library uses a matte default (roughness 0.95, reflectance 0.02).
    /// Individual elements can override via [`El::material`].
    /// `base_color` is overridden by the layout color when both are set.
    #[reflect(ignore)]
    material:       Option<StandardMaterial>,
    /// Default PBR material for text. When `None`, uses the same default as
    /// `material`. Individual text elements can override.
    /// `base_color` is overridden by [`LayoutTextStyle::color`] when set.
    #[reflect(ignore)]
    text_material:  Option<StandardMaterial>,
    /// Whether the panel is world-space or screen-space.
    mode:           PanelMode,
}

impl Default for DiegeticPanel {
    fn default() -> Self {
        Self {
            tree:           LayoutTree::default(),
            width:          0.0,
            height:         0.0,
            layout_unit:    Unit::Meters,
            font_unit:      None,
            anchor:         Anchor::TopLeft,
            world_width:    None,
            world_height:   None,
            render_mode:    RenderMode::Geometry,
            surface_shadow: SurfaceShadow::Off,
            material:       None,
            text_material:  None,
            mode:           PanelMode::World,
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
    pub fn world() -> DiegeticPanelBuilder<World, NeedsSize> {
        DiegeticPanelBuilder {
            data:    BuilderData::default(),
            _marker: PhantomData,
        }
    }

    /// Returns a builder for a screen-space panel.
    ///
    /// Bare floats in `.size()` default to [`Unit::Pixels`].
    #[must_use]
    pub fn screen() -> DiegeticPanelBuilder<Screen, NeedsSize> {
        DiegeticPanelBuilder {
            data:    BuilderData {
                mode: PanelMode::Screen {
                    position:      ScreenPosition::default(),
                    width:         None,
                    height:        None,
                    camera_order:  DEFAULT_SCREEN_SPACE_CAMERA_ORDER,
                    render_layers: RenderLayers::layer(DEFAULT_SCREEN_SPACE_RENDER_LAYER),
                },
                ..BuilderData::default()
            },
            _marker: PhantomData,
        }
    }
}

// ── Computation methods ─────────────────────────────────────────────────────

impl DiegeticPanel {
    /// Returns the layout unit.
    pub(super) const fn resolved_layout_unit(&self, _config: &UnitConfig) -> Unit { self.layout_unit }

    /// Resolves the font unit, falling back to the global [`UnitConfig`].
    pub(super) fn resolved_font_unit(&self, config: &UnitConfig) -> Unit { self.font_unit.unwrap_or(config.font) }

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

// ── Typestate marker types ──────────────────────────────────────────────────

/// Marker: panel lives in 3D world space.
pub struct World;

/// Marker: panel renders as a screen overlay.
pub struct Screen;

/// Marker: builder needs `.size()` or `.paper()` before `.layout()` or `.build()`.
pub struct NeedsSize;

/// Marker: dimensions are set, `.layout()` or `.build()` are available.
pub struct HasSize;

/// Marker: layout tree is built, `.build()` is available.
pub struct Ready;

// ── Builder data (shared across all states) ─────────────────────────────────

#[derive(Default)]
struct BuilderData {
    width:          f32,
    height:         f32,
    layout_unit:    Unit,
    font_unit:      Option<Unit>,
    anchor:         Option<Anchor>,
    world_width:    Option<f32>,
    world_height:   Option<f32>,
    render_mode:    RenderMode,
    surface_shadow: SurfaceShadow,
    material:       Option<StandardMaterial>,
    text_material:  Option<StandardMaterial>,
    tree:           Option<LayoutTree>,
    mode:           PanelMode,
}

/// Builder for [`DiegeticPanel`].
///
/// Constructed via [`DiegeticPanel::world()`] or [`DiegeticPanel::screen()`].
/// The type parameters enforce the correct method call order at compile time:
///
/// - `Mode`: [`World`] or [`Screen`] — determines which methods are available
/// - `State`: [`NeedsSize`] → [`HasSize`] → [`Ready`] — enforces `.size()` before `.layout()`, and
///   `.layout()` before or instead of `.build()`
///
/// # State machine
///
/// ```text
/// NeedsSize ──.size()/.paper()──→ HasSize ──.layout()──→ Ready
///                                    │                     │
///                                    └──.build()───────────┘──→ Result<DiegeticPanel>
/// ```
pub struct DiegeticPanelBuilder<Mode, State> {
    data:    BuilderData,
    _marker: PhantomData<(Mode, State)>,
}

// ── Shared methods (all modes, all states) ──────────────────────────────────

impl<M, S> DiegeticPanelBuilder<M, S> {
    /// Sets the anchor point (default [`Anchor::TopLeft`]).
    #[must_use]
    pub const fn anchor(mut self, anchor: Anchor) -> Self {
        self.data.anchor = Some(anchor);
        self
    }

    /// Sets the default PBR material for backgrounds and borders.
    ///
    /// `base_color` is overridden by the layout color when both are set.
    /// Individual elements can override via [`El::material`].
    #[must_use]
    pub fn material(mut self, material: StandardMaterial) -> Self {
        self.data.material = Some(material);
        self
    }

    /// Sets the default PBR material for text.
    ///
    /// `base_color` is overridden by [`LayoutTextStyle::color`] when set.
    #[must_use]
    pub fn text_material(mut self, material: StandardMaterial) -> Self {
        self.data.text_material = Some(material);
        self
    }

    /// Overrides the font unit (default inherits from [`UnitConfig::font`]).
    #[must_use]
    pub const fn font_unit(mut self, unit: Unit) -> Self {
        self.data.font_unit = Some(unit);
        self
    }

    /// Sets the rendering mode. Defaults to [`RenderMode::Geometry`].
    #[must_use]
    pub const fn render_mode(mut self, mode: RenderMode) -> Self {
        self.data.render_mode = mode;
        self
    }

    /// Sets whether the panel surface casts 3D shadows.
    /// Defaults to [`SurfaceShadow::Off`].
    #[must_use]
    pub const fn surface_shadow(mut self, shadow: SurfaceShadow) -> Self {
        self.data.surface_shadow = shadow;
        self
    }
}

// ── NeedsSize → HasSize transitions ────────────────────────────────────────

impl DiegeticPanelBuilder<World, NeedsSize> {
    /// Sets the panel dimensions and layout unit.
    ///
    /// Bare floats default to [`Unit::Meters`] for world-space panels.
    /// Typed wrappers like [`Mm`](super::config::Mm) or [`Pt`](super::config::Pt)
    /// set the unit explicitly. Both arguments must have the same type;
    /// mixed-unit panel sizing is unsupported.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// DiegeticPanel::world().size(0.5, 0.3)              // 0.5 × 0.3 meters
    /// DiegeticPanel::world().size(Mm(210.0), Mm(297.0))  // A4 in mm
    /// ```
    #[must_use]
    pub fn size<DM: DimensionMatch>(
        mut self,
        w: DM,
        h: DM,
    ) -> DiegeticPanelBuilder<World, HasSize> {
        let wd = w.into();
        let hd = h.into();
        let unit = wd.unit.or(hd.unit).unwrap_or(Unit::Meters);
        self.data.width = wd.value;
        self.data.height = hd.value;
        self.data.layout_unit = unit;
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }

    /// Sets the panel dimensions from a predefined paper size.
    ///
    /// Uses the paper's native unit (mm for ISO, inches for North American).
    #[must_use]
    pub fn paper(mut self, paper: PaperSize) -> DiegeticPanelBuilder<World, HasSize> {
        let (w, h, unit) = paper.dimensions();
        self.data.width = w;
        self.data.height = h;
        self.data.layout_unit = unit;
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }
}

impl DiegeticPanelBuilder<Screen, NeedsSize> {
    /// Sets the panel dimensions and layout unit.
    ///
    /// Bare floats default to [`Unit::Pixels`] for screen-space panels.
    /// Typed wrappers like [`Pt`](super::config::Pt) set the unit explicitly.
    /// Both arguments must have the same type; mixed-unit panel sizing is
    /// unsupported.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// DiegeticPanel::screen().size(800.0, 600.0)          // 800 × 600 pixels
    /// DiegeticPanel::screen().size(Pt(595.0), Pt(842.0))  // A4 in points
    /// ```
    #[must_use]
    pub fn size<DM: DimensionMatch>(
        mut self,
        w: DM,
        h: DM,
    ) -> DiegeticPanelBuilder<Screen, HasSize> {
        let wd = w.into();
        let hd = h.into();
        let unit = wd.unit.or(hd.unit).unwrap_or(Unit::Pixels);
        self.data.width = wd.value;
        self.data.height = hd.value;
        self.data.layout_unit = unit;
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }

    /// Sets the panel dimensions from a predefined paper size.
    ///
    /// Converts to pixels at 72 PPI (1 pt = 1 px in our system).
    #[must_use]
    pub fn paper(mut self, paper: PaperSize) -> DiegeticPanelBuilder<Screen, HasSize> {
        let w = paper.width_as::<Pt>();
        let h = paper.height_as::<Pt>();
        self.data.width = w;
        self.data.height = h;
        self.data.layout_unit = Unit::Pixels;
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }
}

// ── World-only methods (any state) ──────────────────────────────────────────

impl<S> DiegeticPanelBuilder<World, S> {
    /// Scales the panel uniformly so its world width matches this value in meters.
    /// Height follows the aspect ratio.
    #[must_use]
    pub const fn world_width(mut self, meters: f32) -> Self {
        self.data.world_width = Some(meters);
        self
    }

    /// Scales the panel uniformly so its world height matches this value in meters.
    /// Width follows the aspect ratio.
    #[must_use]
    pub const fn world_height(mut self, meters: f32) -> Self {
        self.data.world_height = Some(meters);
        self
    }
}

// ── Screen-only methods on HasSize ──────────────────────────────────────────

impl DiegeticPanelBuilder<Screen, HasSize> {
    /// Panel width fills a fraction of the window (0.0–1.0).
    #[must_use]
    pub const fn width_percent(mut self, fraction: f32) -> Self {
        if let PanelMode::Screen { width, .. } = &mut self.data.mode {
            *width = Some(ScreenDimension::Percent(fraction));
        }
        self
    }

    /// Panel height fills a fraction of the window (0.0–1.0).
    #[must_use]
    pub const fn height_percent(mut self, fraction: f32) -> Self {
        if let PanelMode::Screen { height, .. } = &mut self.data.mode {
            *height = Some(ScreenDimension::Percent(fraction));
        }
        self
    }

    /// Panel width is a fixed pixel value, managed by the plugin.
    #[must_use]
    pub const fn width_px(mut self, pixels: f32) -> Self {
        if let PanelMode::Screen { width, .. } = &mut self.data.mode {
            *width = Some(ScreenDimension::Fixed(pixels));
        }
        self
    }

    /// Panel height is a fixed pixel value, managed by the plugin.
    #[must_use]
    pub const fn height_px(mut self, pixels: f32) -> Self {
        if let PanelMode::Screen { height, .. } = &mut self.data.mode {
            *height = Some(ScreenDimension::Fixed(pixels));
        }
        self
    }

    /// Places the panel at an explicit pixel position (top-left origin, y-down).
    #[must_use]
    pub const fn screen_position(mut self, x: f32, y: f32) -> Self {
        if let PanelMode::Screen { position, .. } = &mut self.data.mode {
            *position = ScreenPosition::At(Vec2::new(x, y));
        }
        self
    }

    /// Sets the overlay camera render order. Default: `1`.
    #[must_use]
    pub const fn camera_order(mut self, order: isize) -> Self {
        if let PanelMode::Screen { camera_order, .. } = &mut self.data.mode {
            *camera_order = order;
        }
        self
    }

    /// Sets the render layers for camera isolation. Default: layer 31.
    #[must_use]
    pub fn render_layers(mut self, layers: RenderLayers) -> Self {
        if let PanelMode::Screen { render_layers, .. } = &mut self.data.mode {
            *render_layers = layers;
        }
        self
    }
}

// ── HasSize → Ready transition (layout) ─────────────────────────────────────

impl DiegeticPanelBuilder<World, HasSize> {
    /// Builds the layout tree from a closure. The closure receives a
    /// [`LayoutBuilder`] pre-configured with the panel's dimensions.
    #[must_use]
    pub fn layout(
        mut self,
        f: impl FnOnce(&mut LayoutBuilder),
    ) -> DiegeticPanelBuilder<World, Ready> {
        let mut builder = LayoutBuilder::new(self.data.width, self.data.height);
        f(&mut builder);
        self.data.tree = Some(builder.build());
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }

    /// Attaches a pre-built layout tree.
    #[must_use]
    pub fn with_tree(mut self, tree: LayoutTree) -> DiegeticPanelBuilder<World, Ready> {
        self.data.tree = Some(tree);
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }
}

impl DiegeticPanelBuilder<Screen, HasSize> {
    /// Builds the layout tree from a closure. The closure receives a
    /// [`LayoutBuilder`] pre-configured with the panel's dimensions.
    #[must_use]
    pub fn layout(
        mut self,
        f: impl FnOnce(&mut LayoutBuilder),
    ) -> DiegeticPanelBuilder<Screen, Ready> {
        let mut builder = LayoutBuilder::new(self.data.width, self.data.height);
        f(&mut builder);
        self.data.tree = Some(builder.build());
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }

    /// Attaches a pre-built layout tree.
    #[must_use]
    pub fn with_tree(mut self, tree: LayoutTree) -> DiegeticPanelBuilder<Screen, Ready> {
        self.data.tree = Some(tree);
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }
}

// ── Sealed CanBuild trait ───────────────────────────────────────────────────

mod sealed {
    pub trait CanBuild {}
}

impl sealed::CanBuild for HasSize {}
impl sealed::CanBuild for Ready {}

// ── Build (on HasSize or Ready, either mode) ────────────────────────────────

impl<S: sealed::CanBuild> DiegeticPanelBuilder<World, S> {
    /// Consumes the builder and returns a [`DiegeticPanel`] component.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSize`] if width or height is zero or negative.
    pub fn build(self) -> Result<DiegeticPanel, InvalidSize> {
        if self.data.width <= 0.0 || self.data.height <= 0.0 {
            return Err(InvalidSize {
                width:  self.data.width,
                height: self.data.height,
            });
        }
        Ok(build_panel(self.data))
    }
}

impl<S: sealed::CanBuild> DiegeticPanelBuilder<Screen, S> {
    /// Consumes the builder and returns a [`DiegeticPanel`] component.
    ///
    /// For screen-space panels, sets `world_height = height` if neither
    /// `world_width` nor `world_height` is set, so that `points_to_world`
    /// equals 1.0 (1 layout unit = 1 world unit = 1 screen pixel under
    /// the orthographic overlay camera).
    ///
    /// When [`ScreenDimension::Percent`] is used for width or height,
    /// the layout tree's root element is automatically set to
    /// `Sizing::GROW` on that axis.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSize`] if width or height is zero or negative
    /// and no percent-based sizing will fill them later.
    pub fn build(mut self) -> Result<DiegeticPanel, InvalidSize> {
        let has_percent_width = matches!(
            self.data.mode,
            PanelMode::Screen {
                width: Some(ScreenDimension::Percent(_)),
                ..
            }
        );
        let has_percent_height = matches!(
            self.data.mode,
            PanelMode::Screen {
                height: Some(ScreenDimension::Percent(_)),
                ..
            }
        );

        if !has_percent_width
            && !has_percent_height
            && (self.data.width <= 0.0 || self.data.height <= 0.0)
        {
            return Err(InvalidSize {
                width:  self.data.width,
                height: self.data.height,
            });
        }

        // Set world_height = height so points_to_world = 1.0.
        if self.data.world_height.is_none() && self.data.world_width.is_none() {
            self.data.world_height = Some(self.data.height);
        }

        // For percent-sized axes, set the root element to GROW.
        if let Some(ref mut tree) = self.data.tree {
            if has_percent_width {
                crate::layout::set_root_grow_width(tree);
            }
            if has_percent_height {
                crate::layout::set_root_grow_height(tree);
            }
        }

        Ok(build_panel(self.data))
    }
}

/// Internal build from `BuilderData`.
fn build_panel(data: BuilderData) -> DiegeticPanel {
    DiegeticPanel {
        tree:           data.tree.unwrap_or_default(),
        width:          data.width,
        height:         data.height,
        layout_unit:    data.layout_unit,
        font_unit:      data.font_unit,
        anchor:         data.anchor.unwrap_or(Anchor::TopLeft),
        world_width:    data.world_width,
        world_height:   data.world_height,
        render_mode:    data.render_mode,
        surface_shadow: data.surface_shadow,
        material:       data.material,
        text_material:  data.text_material,
        mode:           data.mode,
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

//! [`DiegeticPanelBuilder`] with compile-time state machine enforcing call
//! order on panel construction.

use std::marker::PhantomData;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::diegetic_panel::DiegeticPanel;
use super::modes::PanelMode;
use super::modes::RenderMode;
use super::modes::ScreenDimension;
use super::modes::ScreenPosition;
use super::modes::SurfaceShadow;
use crate::layout::Anchor;
use crate::layout::DimensionMatch;
use crate::layout::InvalidSize;
use crate::layout::LayoutBuilder;
use crate::layout::LayoutTree;
use crate::layout::PanelSize;
use crate::layout::PaperSize;
use crate::layout::Pt;
use crate::layout::Unit;

/// Default camera render order for screen-space overlay panels.
const DEFAULT_SCREEN_SPACE_CAMERA_ORDER: isize = 1;
/// Default render layer for screen-space overlay panels.
const DEFAULT_SCREEN_SPACE_RENDER_LAYER: usize = 31;

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
pub(super) struct BuilderData {
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
    text_alpha_mode: Option<AlphaMode>,
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

impl DiegeticPanelBuilder<World, NeedsSize> {
    pub(super) fn new_world() -> Self {
        Self {
            data:    BuilderData::default(),
            _marker: PhantomData,
        }
    }
}

impl DiegeticPanelBuilder<Screen, NeedsSize> {
    pub(super) fn new_screen() -> Self {
        Self {
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
    /// Individual elements can override via `El::material`.
    #[must_use]
    pub fn material(mut self, material: StandardMaterial) -> Self {
        self.data.material = Some(material);
        self
    }

    /// Sets the default PBR material for text.
    ///
    /// `base_color` is overridden by `LayoutTextStyle::color` when set.
    #[must_use]
    pub fn text_material(mut self, material: StandardMaterial) -> Self {
        self.data.text_material = Some(material);
        self
    }

    /// Sets a panel-wide [`AlphaMode`] default for every text chunk in this
    /// panel. Per-style overrides still win.
    ///
    /// See [`StableTransparency`](crate::StableTransparency) when pairing
    /// this with [`AlphaMode::Blend`].
    #[must_use]
    pub const fn text_alpha_mode(mut self, mode: AlphaMode) -> Self {
        self.data.text_alpha_mode = Some(mode);
        self
    }

    /// Overrides the font unit (default inherits from
    /// [`UnitConfig::font`](crate::UnitConfig::font)).
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
    /// Typed wrappers like [`Mm`](crate::Mm) or [`Pt`] set the unit
    /// explicitly. Both arguments must have the same type; mixed-unit
    /// panel sizing is unsupported.
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
    /// Typed wrappers like [`Pt`] set the unit explicitly. Both arguments
    /// must have the same type; mixed-unit panel sizing is unsupported.
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

        if self.data.world_height.is_none() && self.data.world_width.is_none() {
            self.data.world_height = Some(self.data.height);
        }

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
        text_alpha_mode: data.text_alpha_mode,
        mode:           data.mode,
    }
}

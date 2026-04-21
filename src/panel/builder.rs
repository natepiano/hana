//! [`DiegeticPanelBuilder`] with compile-time state machine enforcing call
//! order on panel construction.

use std::marker::PhantomData;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::diegetic_panel::DiegeticPanel;
use super::modes::PanelMode;
use super::modes::RenderMode;
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
use crate::layout::Px;
use crate::layout::Sizing;
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
    width:           f32,
    height:          f32,
    layout_unit:     Unit,
    font_unit:       Option<Unit>,
    anchor:          Option<Anchor>,
    world_width:     Option<f32>,
    world_height:    Option<f32>,
    render_mode:     RenderMode,
    surface_shadow:  SurfaceShadow,
    material:        Option<StandardMaterial>,
    text_material:   Option<StandardMaterial>,
    text_alpha_mode: Option<AlphaMode>,
    tree:            Option<LayoutTree>,
    mode:            PanelMode,
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
                    // Placeholder — `.size()` overwrites both axes before build().
                    width:         Sizing::fixed(Px(0.0)),
                    height:        Sizing::fixed(Px(0.0)),
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
    /// panel. Per-style overrides still win; this in turn overrides the
    /// app-wide [`TextAlphaModeDefault`](crate::TextAlphaModeDefault)
    /// resource.
    ///
    /// See [`TextAlphaModeDefault`](crate::TextAlphaModeDefault) for the
    /// full two-path overview ([`AlphaMode::Blend`] +
    /// [`StableTransparency`](crate::StableTransparency) vs
    /// [`AlphaMode::AlphaToCoverage`] + MSAA) and guidance on mixing modes
    /// for creative effects.
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
    /// Sets the panel dimensions using the layout engine's [`Sizing`] enum
    /// on each axis. Screen panels always operate in logical pixels.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Fixed-pixel panel.
    /// DiegeticPanel::screen()
    ///     .size(Sizing::fixed(Px(600.0)), Sizing::fixed(Px(44.0)))
    ///
    /// // Percent of window width, fixed-pixel height.
    /// DiegeticPanel::screen()
    ///     .size(Sizing::percent(0.25), Sizing::fixed(Px(440.0)))
    ///
    /// // Fit content up to 400 px wide, content-sized height.
    /// DiegeticPanel::screen()
    ///     .size(Sizing::fit_max(Px(400.0)), Sizing::fit())
    ///
    /// // Fill the window.
    /// DiegeticPanel::screen().size(Sizing::GROW, Sizing::GROW)
    /// ```
    #[must_use]
    pub fn size(
        mut self,
        w: Sizing,
        h: Sizing,
    ) -> DiegeticPanelBuilder<Screen, HasSize> {
        // Screen panels always use `Unit::Pixels` for their layout dimension.
        self.data.layout_unit = Unit::Pixels;
        self.data.width = initial_panel_size(w);
        self.data.height = initial_panel_size(h);
        if let PanelMode::Screen { width, height, .. } = &mut self.data.mode {
            *width = w;
            *height = h;
        }
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }

    /// Sets the panel dimensions from a predefined paper size.
    ///
    /// Uses [`Sizing::Fixed`] for both axes, in pixels (1 pt → 1 px under
    /// the orthographic overlay camera).
    #[must_use]
    pub fn paper(mut self, paper: PaperSize) -> DiegeticPanelBuilder<Screen, HasSize> {
        let w = paper.width_as::<Pt>();
        let h = paper.height_as::<Pt>();
        self.data.width = w;
        self.data.height = h;
        self.data.layout_unit = Unit::Pixels;
        if let PanelMode::Screen { width, height, .. } = &mut self.data.mode {
            *width = Sizing::fixed(Px(w));
            *height = Sizing::fixed(Px(h));
        }
        DiegeticPanelBuilder {
            data:    self.data,
            _marker: PhantomData,
        }
    }
}

/// Initial panel width/height to stash in [`BuilderData`]. For `Fixed` we know
/// the exact value; for any dynamic [`Sizing`] we use `0.0` as a placeholder
/// that the screen-space system resolves each frame against the window and
/// the layout result.
fn initial_panel_size(s: Sizing) -> f32 {
    match s {
        Sizing::Fixed(d) => d.value,
        _ => 0.0,
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
    /// When the panel's width or height is [`Sizing::Percent`], [`Sizing::Fit`],
    /// or [`Sizing::Grow`] the layout tree's root element is automatically
    /// updated so the engine reflows against the resolved panel dimensions
    /// without needing a tree rebuild.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSize`] if width or height is zero or negative
    /// and no dynamic sizing will fill it later.
    pub fn build(mut self) -> Result<DiegeticPanel, InvalidSize> {
        let (w_sizing, h_sizing) = match self.data.mode {
            PanelMode::Screen { width, height, .. } => (width, height),
            PanelMode::World => unreachable!("Screen builder cannot have World mode"),
        };
        let has_dynamic_width = !matches!(w_sizing, Sizing::Fixed(_));
        let has_dynamic_height = !matches!(h_sizing, Sizing::Fixed(_));

        if !has_dynamic_width
            && !has_dynamic_height
            && (self.data.width <= 0.0 || self.data.height <= 0.0)
        {
            return Err(InvalidSize {
                width:  self.data.width,
                height: self.data.height,
            });
        }

        if self.data.world_height.is_none() && self.data.world_width.is_none() {
            self.data.world_height = Some(self.data.height.max(1.0));
        }

        if let Some(ref mut tree) = self.data.tree {
            // The layout engine's two passes (bottom-up `propagate_fit_sizes`
            // and top-down `size_along_axis`) already resolve `Fit` roots to
            // their natural content size, so we route each Sizing variant to
            // the matching root kind here.
            match w_sizing {
                Sizing::Fit { min, max } => crate::layout::set_root_fit_width(tree, min, max),
                Sizing::Grow { .. } | Sizing::Percent(_) => crate::layout::set_root_grow_width(tree),
                Sizing::Fixed(_) => {},
            }
            match h_sizing {
                Sizing::Fit { min, max } => crate::layout::set_root_fit_height(tree, min, max),
                Sizing::Grow { .. } | Sizing::Percent(_) => crate::layout::set_root_grow_height(tree),
                Sizing::Fixed(_) => {},
            }
        }
        let _ = (has_dynamic_width, has_dynamic_height);

        Ok(build_panel(self.data))
    }
}

fn build_panel(data: BuilderData) -> DiegeticPanel {
    DiegeticPanel {
        tree:            data.tree.unwrap_or_default(),
        width:           data.width,
        height:          data.height,
        layout_unit:     data.layout_unit,
        font_unit:       data.font_unit,
        anchor:          data.anchor.unwrap_or(Anchor::TopLeft),
        world_width:     data.world_width,
        world_height:    data.world_height,
        render_mode:     data.render_mode,
        surface_shadow:  data.surface_shadow,
        material:        data.material,
        text_material:   data.text_material,
        text_alpha_mode: data.text_alpha_mode,
        mode:            data.mode,
    }
}

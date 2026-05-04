//! [`DiegeticPanelBuilder`] with compile-time state machine enforcing call
//! order on panel construction.

use std::marker::PhantomData;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::constants::DEFAULT_SCREEN_SPACE_CAMERA_ORDER;
use super::constants::DEFAULT_SCREEN_SPACE_RENDER_LAYER;
use super::constants::MIN_PANEL_WORLD_HEIGHT;
use super::coordinate_space::CoordinateSpace;
use super::coordinate_space::RenderMode;
use super::coordinate_space::ScreenPosition;
use super::coordinate_space::SurfaceShadow;
use super::diegetic_panel::DiegeticPanel;
use super::sizing::CompatibleUnits;
use super::sizing::PanelSizing;
use crate::layout;
use crate::layout::Anchor;
use crate::layout::Dimension;
use crate::layout::InvalidSize;
use crate::layout::LayoutBuilder;
use crate::layout::LayoutTree;
use crate::layout::PanelSize;
use crate::layout::PaperSize;
use crate::layout::Pt;
use crate::layout::Px;
use crate::layout::Sizing;
use crate::layout::Unit;

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
    width:            f32,
    height:           f32,
    layout_unit:      Unit,
    font_unit:        Option<Unit>,
    anchor:           Option<Anchor>,
    world_width:      Option<f32>,
    world_height:     Option<f32>,
    render_mode:      RenderMode,
    surface_shadow:   SurfaceShadow,
    material:         Option<StandardMaterial>,
    text_material:    Option<StandardMaterial>,
    text_alpha_mode:  Option<AlphaMode>,
    tree:             Option<LayoutTree>,
    coordinate_space: CoordinateSpace,
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
///
/// # Screen vs World sizing — divergence table
///
/// `.size<W, H>(w, h)` accepts any value implementing
/// [`PanelSizing<Mode>`](super::sizing::PanelSizing). The trait is
/// implemented differently for each mode:
///
/// | Value | `PanelSizing<Screen>` | `PanelSizing<World>` | Rationale |
/// |---|:-:|:-:|---|
/// | [`Px`] / [`Mm`] / [`Pt`] / [`In`](crate::layout::In) / bare `f32` | ✅ | ✅ | physical sizes — legal anywhere |
/// | [`Sizing`] (engine enum — escape hatch) | ✅ | ✅ | both modes share one engine |
/// | [`Fit`](super::sizing::Fit) / [`FitMax`](super::sizing::FitMax) / [`FitRange`](super::sizing::FitRange) | ✅ | ✅ | shrink-to-content works on both |
/// | [`Percent`](super::sizing::Percent) | ✅ | ❌ compile error | world has no parent |
/// | [`Grow`](super::sizing::Grow) / [`GrowMax`](super::sizing::GrowMax) / [`GrowRange`](super::sizing::GrowRange) | ✅ | ❌ compile error | world has no bounding container |
/// | Physical-unit cross-axis match | N/A (pixel-only) | enforced via [`CompatibleUnits`](super::sizing::CompatibleUnits) | unit mixing in 3D is always a bug |
/// | `.world_width(m)` / `.world_height(m)` | — | ✅ | world-space scaling |
///
/// # Zero / negative size is a runtime check
///
/// `Px(0.0)` is valid at compile time — the type system alone can't
/// reject zero or negative sizes without reaching for `NonZeroDimension`
/// wrappers that would break `Px(0.0)` as a legitimate `GrowRange` floor
/// or `FitMax` sentinel. So `.build()` returns
/// [`InvalidSize`](crate::layout::InvalidSize) when both axes are fixed
/// and either is zero or negative. Dynamic axes
/// (`Fit` / `Percent` / `Grow`) start at `0.0` and resolve later — they
/// do *not* trip this check.
///
/// # `Sizing` escape-hatch footgun on world panels
///
/// Every value type routes to the engine's [`Sizing`] enum via
/// [`PanelSizing::to_sizing`](super::sizing::PanelSizing::to_sizing).
/// The enum itself implements `PanelSizing<World>`, so a caller can
/// sneak a `Sizing::Grow { .. }` or `Sizing::Percent(_)` onto a world
/// panel through the escape hatch, even though the compile-time guard
/// rejects [`Grow`](super::sizing::Grow) / [`Percent`](super::sizing::Percent)
/// directly. Debug builds catch this with a `debug_assert!` in the
/// world `build()` path; release builds silently clamp to the resolved
/// dimensions.
pub struct DiegeticPanelBuilder<Mode, State> {
    data:   BuilderData,
    marker: PhantomData<(Mode, State)>,
}

impl DiegeticPanelBuilder<World, NeedsSize> {
    pub(super) fn new_world() -> Self {
        Self {
            data:   BuilderData::default(),
            marker: PhantomData,
        }
    }
}

impl DiegeticPanelBuilder<Screen, NeedsSize> {
    pub(super) fn new_screen() -> Self {
        Self {
            data:   BuilderData {
                coordinate_space: CoordinateSpace::Screen {
                    position:      ScreenPosition::default(),
                    // Placeholder — `.size()` overwrites both axes before build().
                    width:         Sizing::fixed(Px(0.0)),
                    height:        Sizing::fixed(Px(0.0)),
                    camera_order:  DEFAULT_SCREEN_SPACE_CAMERA_ORDER,
                    render_layers: RenderLayers::layer(DEFAULT_SCREEN_SPACE_RENDER_LAYER),
                },
                ..BuilderData::default()
            },
            marker: PhantomData,
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
    /// panel. Per-style overrides still win; this in turn overrides
    /// [`CascadeDefaults::text_alpha`](crate::CascadeDefaults).
    ///
    /// See [`StableTransparency`](crate::StableTransparency) for the full
    /// two-path overview ([`AlphaMode::Blend`] + `StableTransparency` vs
    /// [`AlphaMode::AlphaToCoverage`] + MSAA) and guidance on mixing modes
    /// for creative effects.
    #[must_use]
    pub const fn text_alpha_mode(mut self, mode: AlphaMode) -> Self {
        self.data.text_alpha_mode = Some(mode);
        self
    }

    /// Overrides the font unit (default inherits from
    /// [`CascadeDefaults::panel_font_unit`](crate::CascadeDefaults)).
    #[must_use]
    pub const fn font_unit(mut self, unit: Unit) -> Self {
        self.data.font_unit = Some(unit);
        self
    }

    /// Sets the rendering mode. Defaults to [`RenderMode::Geometry`].
    #[must_use]
    pub const fn render_mode(mut self, render_mode: RenderMode) -> Self {
        self.data.render_mode = render_mode;
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
    /// Each axis may use any value implementing
    /// [`PanelSizing<World>`](super::sizing::PanelSizing): [`Px`], [`Mm`],
    /// [`Pt`], [`In`](crate::layout::In), bare `f32`,
    /// [`Fit`](super::sizing::Fit), [`FitMax`](super::sizing::FitMax),
    /// [`FitRange`](super::sizing::FitRange), or the engine [`Sizing`]
    /// enum (escape hatch). Screen-only variants
    /// ([`Percent`](super::sizing::Percent), [`Grow`](super::sizing::Grow),
    /// [`GrowMax`](super::sizing::GrowMax),
    /// [`GrowRange`](super::sizing::GrowRange)) are compile errors on a
    /// world panel — there is no parent container to resolve against.
    ///
    /// Cross-axis physical-unit consistency is enforced at compile time
    /// via [`CompatibleUnits`](super::sizing::CompatibleUnits): two
    /// concrete physical units must match (e.g. `(Mm, Mm)` is fine;
    /// `(Mm, Px)` is a compile error). Unit-less values like [`Fit`] or
    /// bare `f32` adopt the other axis's unit, or fall back to
    /// [`Unit::Meters`] when both axes are unit-less.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// DiegeticPanel::world().size(0.5, 0.3)              // 0.5 × 0.3 meters
    /// DiegeticPanel::world().size(Mm(210.0), Mm(297.0))  // A4 in mm
    /// DiegeticPanel::world().size(Fit, FitMax(Mm(300.0)))
    /// ```
    #[must_use]
    pub fn size<W, H>(mut self, w: W, h: H) -> DiegeticPanelBuilder<World, HasSize>
    where
        W: PanelSizing<World>,
        H: PanelSizing<World>,
        (W::Unit, H::Unit): CompatibleUnits,
    {
        let w_sizing = w.to_sizing();
        let h_sizing = h.to_sizing();
        let unit = sizing_unit(w_sizing)
            .or_else(|| sizing_unit(h_sizing))
            .unwrap_or(Unit::Meters);
        self.data.width = initial_panel_size(w_sizing);
        self.data.height = initial_panel_size(h_sizing);
        self.data.layout_unit = unit;
        self.data.coordinate_space = CoordinateSpace::World {
            width:  w_sizing,
            height: h_sizing,
        };
        DiegeticPanelBuilder {
            data:   self.data,
            marker: PhantomData,
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
        self.data.coordinate_space = CoordinateSpace::World {
            width:  Sizing::fixed(Dimension {
                value: w,
                unit:  Some(unit),
            }),
            height: Sizing::fixed(Dimension {
                value: h,
                unit:  Some(unit),
            }),
        };
        DiegeticPanelBuilder {
            data:   self.data,
            marker: PhantomData,
        }
    }
}

impl DiegeticPanelBuilder<Screen, NeedsSize> {
    /// Sets the panel dimensions on each axis.
    ///
    /// Each axis may use any value implementing
    /// [`PanelSizing<Screen>`](super::sizing::PanelSizing): [`Px`], [`Mm`],
    /// [`Pt`], [`In`](crate::layout::In), bare `f32`,
    /// [`Fit`](super::sizing::Fit), [`FitMax`](super::sizing::FitMax),
    /// [`FitRange`](super::sizing::FitRange),
    /// [`Percent`](super::sizing::Percent), [`Grow`](super::sizing::Grow),
    /// [`GrowMax`](super::sizing::GrowMax),
    /// [`GrowRange`](super::sizing::GrowRange), or the engine [`Sizing`]
    /// enum (escape hatch). Axes are independent — no cross-axis unit
    /// check applies.
    ///
    /// Screen panels always operate in logical pixels for layout; the
    /// panel's internal `layout_unit` is always [`Unit::Pixels`]
    /// regardless of what the argument types carry.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// DiegeticPanel::screen().size(Px(600.0), Px(44.0))            // fixed
    /// DiegeticPanel::screen().size(Percent(0.25), Px(44.0))        // % width
    /// DiegeticPanel::screen().size(FitMax(Px(400.0)), Fit)         // auto-shrink
    /// DiegeticPanel::screen().size(Grow, Grow)                     // fill
    /// ```
    #[must_use]
    pub fn size<W, H>(mut self, w: W, h: H) -> DiegeticPanelBuilder<Screen, HasSize>
    where
        W: PanelSizing<Screen>,
        H: PanelSizing<Screen>,
    {
        let w_sizing = w.to_sizing();
        let h_sizing = h.to_sizing();
        // Screen panels always use `Unit::Pixels` for their layout dimension.
        self.data.layout_unit = Unit::Pixels;
        self.data.width = initial_panel_size(w_sizing);
        self.data.height = initial_panel_size(h_sizing);
        if let CoordinateSpace::Screen { width, height, .. } = &mut self.data.coordinate_space {
            *width = w_sizing;
            *height = h_sizing;
        }
        DiegeticPanelBuilder {
            data:   self.data,
            marker: PhantomData,
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
        if let CoordinateSpace::Screen { width, height, .. } = &mut self.data.coordinate_space {
            *width = Sizing::fixed(Px(w));
            *height = Sizing::fixed(Px(h));
        }
        DiegeticPanelBuilder {
            data:   self.data,
            marker: PhantomData,
        }
    }
}

/// Initial panel width/height to stash in [`BuilderData`]. For `Fixed` we know
/// the exact value; for any dynamic [`Sizing`] we use `0.0` as a placeholder
/// that the screen-space system resolves each frame against the window and
/// the layout result.
const fn initial_panel_size(s: Sizing) -> f32 {
    match s {
        Sizing::Fixed(d) => d.value,
        _ => 0.0,
    }
}

/// Returns the first explicit [`Unit`] observed inside a [`Sizing`] value,
/// or `None` if the sizing carries only unit-less dimensions.
///
/// `Sizing::Fixed(Dimension { unit: Some(u), .. })` yields `Some(u)`;
/// `Sizing::Fit { min, max }` prefers `min.unit` over `max.unit`;
/// `Sizing::Percent` has no backing dimension and yields `None`.
const fn sizing_unit(s: Sizing) -> Option<Unit> {
    match s {
        Sizing::Fixed(d) => d.unit,
        Sizing::Fit { min, max } | Sizing::Grow { min, max } => match min.unit {
            Some(u) => Some(u),
            None => max.unit,
        },
        Sizing::Percent(_) => None,
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
        if let CoordinateSpace::Screen { position, .. } = &mut self.data.coordinate_space {
            *position = ScreenPosition::At(Vec2::new(x, y));
        }
        self
    }

    /// Sets the overlay camera render order. Default: `1`.
    #[must_use]
    pub const fn camera_order(mut self, order: isize) -> Self {
        if let CoordinateSpace::Screen { camera_order, .. } = &mut self.data.coordinate_space {
            *camera_order = order;
        }
        self
    }

    /// Sets the render layers for camera isolation. Default: layer 31.
    #[must_use]
    pub fn render_layers(mut self, layers: RenderLayers) -> Self {
        if let CoordinateSpace::Screen { render_layers, .. } = &mut self.data.coordinate_space {
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
            data:   self.data,
            marker: PhantomData,
        }
    }

    /// Attaches a pre-built layout tree.
    #[must_use]
    pub fn with_tree(mut self, tree: LayoutTree) -> DiegeticPanelBuilder<World, Ready> {
        self.data.tree = Some(tree);
        DiegeticPanelBuilder {
            data:   self.data,
            marker: PhantomData,
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
            data:   self.data,
            marker: PhantomData,
        }
    }

    /// Attaches a pre-built layout tree.
    #[must_use]
    pub fn with_tree(mut self, tree: LayoutTree) -> DiegeticPanelBuilder<Screen, Ready> {
        self.data.tree = Some(tree);
        DiegeticPanelBuilder {
            data:   self.data,
            marker: PhantomData,
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
    /// Dynamic axes (`Fit { .. }`) start at `0.0`; the post-layout system
    /// resolves them to content bounds each frame, so zero-size inputs
    /// are only rejected when *both* axes are `Fixed(_)`.
    ///
    /// `Percent` / `Grow` are compile-rejected for world panels via
    /// [`PanelSizing<World>`](super::sizing::PanelSizing); a caller that
    /// routes them in via [`Sizing`] (the escape hatch) gets them clamped
    /// to the resolved dimensions — a `debug_assert!` flags the footgun
    /// in debug builds.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSize`] if both axes are fixed-size and width or
    /// height is zero or negative.
    pub fn build(mut self) -> Result<DiegeticPanel, InvalidSize> {
        let (w_sizing, h_sizing) = match self.data.coordinate_space {
            CoordinateSpace::World { width, height } => (width, height),
            CoordinateSpace::Screen { .. } => {
                return Err(InvalidSize {
                    width:  self.data.width,
                    height: self.data.height,
                });
            },
        };

        debug_assert!(
            !matches!(w_sizing, Sizing::Grow { .. } | Sizing::Percent(_)),
            "world panel width is Grow/Percent — the escape-hatch `Sizing` bypassed the \
             compile-time guard. These variants have no parent container in world space; \
             use `Fit`, `FitMax`, `FitRange`, or a fixed physical unit instead."
        );
        debug_assert!(
            !matches!(h_sizing, Sizing::Grow { .. } | Sizing::Percent(_)),
            "world panel height is Grow/Percent — the escape-hatch `Sizing` bypassed the \
             compile-time guard. These variants have no parent container in world space; \
             use `Fit`, `FitMax`, `FitRange`, or a fixed physical unit instead."
        );

        let w_dynamic = !matches!(w_sizing, Sizing::Fixed(_));
        let h_dynamic = !matches!(h_sizing, Sizing::Fixed(_));

        if !w_dynamic && !h_dynamic && (self.data.width <= 0.0 || self.data.height <= 0.0) {
            return Err(InvalidSize {
                width:  self.data.width,
                height: self.data.height,
            });
        }

        if let Some(ref mut tree) = self.data.tree {
            match w_sizing {
                Sizing::Fit { min, max } => layout::set_root_fit_width(tree, min, max),
                Sizing::Grow { min, max } => {
                    layout::set_root_grow_width(tree, min, max);
                },
                Sizing::Percent(_) | Sizing::Fixed(_) => {},
            }
            match h_sizing {
                Sizing::Fit { min, max } => layout::set_root_fit_height(tree, min, max),
                Sizing::Grow { min, max } => {
                    layout::set_root_grow_height(tree, min, max);
                },
                Sizing::Percent(_) | Sizing::Fixed(_) => {},
            }
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
        let CoordinateSpace::Screen {
            width: w_sizing,
            height: h_sizing,
            ..
        } = self.data.coordinate_space
        else {
            return Err(InvalidSize {
                width:  self.data.width,
                height: self.data.height,
            });
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

        // Freeze `world_height` for fixed screen panels so 1 layout pixel
        // maps to 1 world unit under the ortho camera. Dynamic-axis
        // panels skip this: their `self.data.height` is still `0.0` at
        // build time, and `anchor_offsets()` short-circuits to
        // `panel.width`/`panel.height` directly for screen panels so
        // the frozen field isn't needed for positioning.
        if !has_dynamic_width
            && !has_dynamic_height
            && self.data.world_height.is_none()
            && self.data.world_width.is_none()
        {
            self.data.world_height = Some(self.data.height.max(MIN_PANEL_WORLD_HEIGHT));
        }

        if let Some(ref mut tree) = self.data.tree {
            // The layout engine's two passes (bottom-up `propagate_fit_sizes`
            // and top-down `size_along_axis`) already resolve `Fit` roots to
            // their natural content size, so we route each Sizing variant to
            // the matching root kind here. `Percent` routes to an unbounded
            // `Grow` at the root because `resolve_screen_axis` has already
            // pre-multiplied the panel width by the percent fraction — the
            // viewport the engine receives is the post-percent panel size.
            match w_sizing {
                Sizing::Fit { min, max } => layout::set_root_fit_width(tree, min, max),
                Sizing::Grow { min, max } => {
                    layout::set_root_grow_width(tree, min, max);
                },
                Sizing::Percent(_) => {
                    layout::set_root_grow_width(
                        tree,
                        Dimension {
                            value: 0.0,
                            unit:  None,
                        },
                        Dimension {
                            value: f32::MAX,
                            unit:  None,
                        },
                    );
                },
                Sizing::Fixed(_) => {},
            }
            match h_sizing {
                Sizing::Fit { min, max } => layout::set_root_fit_height(tree, min, max),
                Sizing::Grow { min, max } => {
                    layout::set_root_grow_height(tree, min, max);
                },
                Sizing::Percent(_) => {
                    layout::set_root_grow_height(
                        tree,
                        Dimension {
                            value: 0.0,
                            unit:  None,
                        },
                        Dimension {
                            value: f32::MAX,
                            unit:  None,
                        },
                    );
                },
                Sizing::Fixed(_) => {},
            }
        }
        let _ = (has_dynamic_width, has_dynamic_height);

        Ok(build_panel(self.data))
    }
}

fn build_panel(data: BuilderData) -> DiegeticPanel {
    DiegeticPanel {
        tree:             data.tree.unwrap_or_default(),
        width:            data.width,
        height:           data.height,
        layout_unit:      data.layout_unit,
        font_unit:        data.font_unit,
        anchor:           data.anchor.unwrap_or(Anchor::TopLeft),
        world_width:      data.world_width,
        world_height:     data.world_height,
        render_mode:      data.render_mode,
        surface_shadow:   data.surface_shadow,
        material:         data.material,
        text_material:    data.text_material,
        text_alpha_mode:  data.text_alpha_mode,
        coordinate_space: data.coordinate_space,
    }
}

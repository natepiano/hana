//! Components and resources for diegetic UI panels.

use std::sync::Arc;

use bevy::prelude::*;

use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::MeasureTextFn;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;
use crate::plugin::config::Unit;
use crate::plugin::config::UnitConfig;

/// A diegetic UI panel attached to a 3D entity.
///
/// Defines a layout tree and the panel's dimensions in layout units.
/// World-space size is computed automatically from the `layout_unit`
/// (or the global [`UnitConfig`] default). Font sizes in the tree are
/// interpreted in `font_unit` (defaults to [`Unit::Points`]).
///
/// The layout engine runs automatically when this component changes,
/// storing results in [`ComputedDiegeticPanel`].
///
/// Requires a [`Transform`] for world-space positioning.
///
/// # Examples
///
/// ```ignore
/// // A4 page in millimeters with point-sized fonts:
/// DiegeticPanel {
///     tree,
///     width: 210.0,
///     height: 297.0,
///     layout_unit: Some(Unit::Millimeters),
///     ..default()
/// }
///
/// // US business card in inches:
/// DiegeticPanel {
///     tree,
///     width: 3.5,
///     height: 2.0,
///     layout_unit: Some(Unit::Inches),
///     ..default()
/// }
/// ```
#[derive(Component, Reflect)]
#[reflect(Component)]
#[require(ComputedDiegeticPanel, Transform, Visibility)]
pub struct DiegeticPanel {
    /// The layout tree defining this panel's UI structure.
    #[reflect(ignore)]
    pub tree:        LayoutTree,
    /// Panel width in layout units.
    pub width:       f32,
    /// Panel height in layout units.
    pub height:      f32,
    /// Unit for `width`/`height`. `None` inherits from [`UnitConfig::layout`].
    pub layout_unit: Option<Unit>,
    /// Unit for font sizes in the layout tree. `None` inherits from [`UnitConfig::font`].
    pub font_unit:   Option<Unit>,
    /// Which point on the panel sits at the entity's [`Transform`] position.
    /// Defaults to [`Anchor::TopLeft`].
    pub anchor:      Anchor,
}

impl Default for DiegeticPanel {
    fn default() -> Self {
        Self {
            tree:        LayoutTree::default(),
            width:       0.0,
            height:      0.0,
            layout_unit: None,
            font_unit:   None,
            anchor:      Anchor::TopLeft,
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

    /// Panel width in meters (world units).
    #[must_use]
    pub fn world_width(&self, config: &UnitConfig) -> f32 {
        self.width * self.resolved_layout_unit(config).meters_per_unit()
    }

    /// Panel height in meters (world units).
    #[must_use]
    pub fn world_height(&self, config: &UnitConfig) -> f32 {
        self.height * self.resolved_layout_unit(config).meters_per_unit()
    }

    /// Returns the (x_offset, y_offset) in world meters for converting
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
        (w * fx, h * (1.0 - fy))
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

//! Components and resources for diegetic UI panels.

use std::sync::Arc;

use bevy::prelude::*;

use crate::layout::BoundingBox;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::MeasureTextFn;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;

/// A diegetic UI panel attached to a 3D entity.
///
/// Defines a layout tree and the mapping between abstract layout units and
/// physical world-space dimensions. The layout engine runs automatically
/// when this component changes, storing results in [`ComputedDiegeticPanel`].
///
/// Requires a [`Transform`] for world-space positioning.
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
#[require(ComputedDiegeticPanel, Transform, Visibility)]
pub struct DiegeticPanel {
    /// The layout tree defining this panel's UI structure.
    #[reflect(ignore)]
    pub tree:          LayoutTree,
    /// Width in abstract layout units (viewport width for the layout engine).
    pub layout_width:  f32,
    /// Height in abstract layout units (viewport height for the layout engine).
    pub layout_height: f32,
    /// Width of the panel in world units.
    pub world_width:   f32,
    /// Height of the panel in world units.
    pub world_height:  f32,
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
    pub fn content_width(&self) -> f32 { self.content_width }

    /// Actual computed content height in world units.
    #[must_use]
    pub fn content_height(&self) -> f32 { self.content_height }

    /// Returns the bounding box of the panel's content in layout units,
    /// or `None` if layout has not yet been computed.
    #[must_use]
    pub fn content_bounds(&self) -> Option<BoundingBox> {
        self.result.as_ref().and_then(LayoutResult::content_bounds)
    }

    // ── Crate-internal accessors ─────────────────────────────────────────

    #[must_use]
    pub(crate) fn result(&self) -> Option<&LayoutResult> { self.result.as_ref() }

    pub(crate) fn set_result(&mut self, result: LayoutResult) { self.result = Some(result); }

    pub(crate) fn set_content_size(&mut self, width: f32, height: f32) {
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
                let height = measure.effective_line_height();
                TextDimensions { width, height }
            }),
        }
    }
}

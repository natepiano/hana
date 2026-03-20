//! Components and resources for diegetic UI panels.

use std::sync::Arc;

use bevy::prelude::*;

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
#[derive(Component, Reflect)]
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

/// Computed layout result for a [`DiegeticPanel`].
///
/// Automatically added via required components when a [`DiegeticPanel`] is inserted.
/// Updated by the layout system whenever the panel changes.
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct ComputedDiegeticPanel {
    /// The computed layout result, populated after the first layout pass.
    #[reflect(ignore)]
    pub result:             Option<LayoutResult>,
    /// Actual computed content width in world units.
    pub world_width:        f32,
    /// Actual computed content height in world units.
    pub world_height:       f32,
    /// Hash of the last fully computed layout (excludes colors).
    ///
    /// Used by `compute_panel_layouts` to skip layout recomputation when only
    /// render-only properties (like text color) changed.
    #[reflect(ignore)]
    pub last_layout_hash:   u64,
    /// `true` when the most recent update only changed colors, not layout.
    ///
    /// Set by `compute_panel_layouts` so the text renderer can take a fast
    /// path (patch vertex colors) instead of rebuilding meshes from scratch.
    #[reflect(ignore)]
    pub color_only:         bool,
    /// Layout width used for the last full computation.
    #[reflect(ignore)]
    pub last_layout_width:  f32,
    /// Layout height used for the last full computation.
    #[reflect(ignore)]
    pub last_layout_height: f32,
}

impl ComputedDiegeticPanel {
    /// Returns `true` when the panel's tree has only changed render-only
    /// properties (colors) since the last full layout computation.
    ///
    /// The guard checks that:
    /// - The tree has a valid (non-zero) layout hash.
    /// - The hash matches the last fully computed layout.
    /// - The panel dimensions haven't changed since the last computation.
    /// - A previous layout result exists to patch into.
    #[must_use]
    pub(super) fn is_color_only_change(
        &self,
        tree_layout_hash: u64,
        layout_width: f32,
        layout_height: f32,
    ) -> bool {
        tree_layout_hash != 0
            && tree_layout_hash == self.last_layout_hash
            && (self.last_layout_width - layout_width).abs() < f32::EPSILON
            && (self.last_layout_height - layout_height).abs() < f32::EPSILON
            && self.result.is_some()
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

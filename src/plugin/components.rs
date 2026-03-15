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
#[derive(Component)]
#[require(ComputedDiegeticPanel, Transform)]
pub struct DiegeticPanel {
    /// The layout tree defining this panel's UI structure.
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
#[derive(Component, Default)]
pub struct ComputedDiegeticPanel {
    /// The computed layout result, populated after the first layout pass.
    pub result: Option<LayoutResult>,
}

/// Resource providing text measurement for layout computation.
///
/// Insert this resource before adding [`DiegeticUiPlugin`] to override the default
/// monospace approximation with a real text measurement function (e.g. backed by
/// `bevy_rich_text3d`).
#[derive(Resource)]
pub struct DiegeticTextMeasurer(pub MeasureTextFn);

impl Default for DiegeticTextMeasurer {
    fn default() -> Self {
        Self(Arc::new(|text: &str, measure: &TextMeasure| {
            let char_width = measure.size * 0.6;
            #[allow(clippy::cast_precision_loss)]
            let width = char_width * text.len() as f32;
            let height = measure.effective_line_height();
            TextDimensions { width, height }
        }))
    }
}

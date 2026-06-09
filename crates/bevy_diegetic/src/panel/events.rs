//! Public panel lifecycle events.

use bevy::prelude::*;

use super::constants::PANEL_RESIZE_EPSILON;
use super::diegetic_panel::ComputedDiegeticPanel;
use super::diegetic_panel::DiegeticPanel;

/// Final panel dimensions for the current layout pass.
///
/// Both sizes use the same units as [`DiegeticPanel::width`] /
/// [`DiegeticPanel::height`]. For screen-space panels, that is logical pixels
/// after screen sizing resolves; for world panels, it is the panel's layout
/// unit after any `Fit` axis has been applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelDimensions {
    /// The visible panel surface size after fixed / fit / percent / grow sizing
    /// has resolved.
    pub resolved_size: Vec2,
    /// The measured size of the panel's layout-tree content.
    pub content_size:  Vec2,
}

impl PanelDimensions {
    pub(super) const fn from_panel(
        panel: &DiegeticPanel,
        computed: &ComputedDiegeticPanel,
    ) -> Self {
        Self {
            resolved_size: Vec2::new(panel.width(), panel.height()),
            content_size:  Vec2::new(computed.content_width(), computed.content_height()),
        }
    }

    fn differs_from(self, other: Self) -> bool {
        let resolved_delta = (self.resolved_size - other.resolved_size).abs();
        let content_delta = (self.content_size - other.content_size).abs();
        resolved_delta.max_element() > PANEL_RESIZE_EPSILON
            || content_delta.max_element() > PANEL_RESIZE_EPSILON
    }
}

/// Fired on a [`DiegeticPanel`] entity when its computed dimensions become
/// available for the first time or change later.
///
/// Observe this when another panel or system depends on a panel's measured
/// size. The event fires after world `Fit` sizing and screen-space sizing have
/// resolved for the frame, before screen-space transforms are positioned.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct PanelDimensionsChanged {
    /// The panel entity whose dimensions changed.
    pub entity:     Entity,
    /// The newly resolved dimensions.
    pub dimensions: PanelDimensions,
    /// The last emitted dimensions, or `None` for the first valid measurement.
    pub previous:   Option<PanelDimensions>,
}

/// Last panel dimensions emitted through [`PanelDimensionsChanged`].
#[derive(Component, Default)]
pub(crate) struct LastPanelDimensions {
    last: Option<PanelDimensions>,
}

enum DimensionRecord {
    Unchanged,
    Changed { previous: Option<PanelDimensions> },
}

impl LastPanelDimensions {
    fn record(&mut self, dimensions: PanelDimensions) -> DimensionRecord {
        match self.last {
            Some(previous) if !dimensions.differs_from(previous) => DimensionRecord::Unchanged,
            previous => {
                self.last = Some(dimensions);
                DimensionRecord::Changed { previous }
            },
        }
    }
}

pub(crate) fn trigger_panel_dimensions_changed(
    commands: &mut Commands,
    entity: Entity,
    panel: &DiegeticPanel,
    computed: &ComputedDiegeticPanel,
    last: &mut LastPanelDimensions,
) {
    if computed.result().is_none() {
        return;
    }
    let dimensions = PanelDimensions::from_panel(panel, computed);
    let DimensionRecord::Changed { previous } = last.record(dimensions) else {
        return;
    };
    commands
        .entity(entity)
        .trigger(move |current_entity| PanelDimensionsChanged {
            entity: current_entity,
            dimensions,
            previous,
        });
}

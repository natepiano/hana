//! Per-frame screen-space panel rectangles used during attachment resolution.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::window;
use crate::layout::Anchor;
use crate::layout::Unit;
use crate::panel;
use crate::panel::CoordinateSpace;
use crate::panel::DiegeticPanel;
use crate::panel::PanelScreenBounds;
use crate::panel::ResolvedScreenPanelPosition;

#[derive(Clone, Copy, Debug)]
pub(super) struct ScreenPanelRect {
    pub(super) anchor_position: Vec2,
    pub(super) anchor:          Anchor,
    size:                       Vec2,
    layout_unit:                Unit,
}

impl ScreenPanelRect {
    fn from_panel(panel: &DiegeticPanel, window_size: Vec2) -> Option<Self> {
        let anchor_position = panel::screen_anchor_position(panel, window_size, None).ok()?;
        let size = Vec2::new(panel.width(), panel.height());
        PanelScreenBounds::from_anchor_position(anchor_position, panel.anchor(), size).ok()?;
        Some(Self {
            anchor_position,
            anchor: panel.anchor(),
            size,
            layout_unit: panel.layout_unit(),
        })
    }

    pub(super) fn bounds(self) -> Option<PanelScreenBounds> {
        PanelScreenBounds::from_anchor_position(self.anchor_position, self.anchor, self.size).ok()
    }

    pub(super) const fn layout_unit(self) -> Unit { self.layout_unit }

    pub(super) const fn with_anchor_position(self, anchor_position: Vec2) -> Self {
        Self {
            anchor_position,
            ..self
        }
    }
}

pub(super) fn screen_panel_rects(
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> HashMap<Entity, ScreenPanelRect> {
    let mut rects = HashMap::default();
    for (entity, panel) in panels {
        let CoordinateSpace::Screen { window, .. } = panel.coordinate_space() else {
            continue;
        };
        let Ok((_, window_size)) = window::resolve_window(*window, primary, window_sizes) else {
            continue;
        };
        if let Some(rect) = ScreenPanelRect::from_panel(panel, window_size) {
            rects.insert(entity, rect);
        }
    }
    rects
}

//! Per-frame screen-space panel rectangles used during attachment resolution.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::screen_in_plane_angle;
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
    angle:                      f32,
    bounds:                     Option<PanelScreenBounds>,
    layout_unit:                Unit,
}

impl ScreenPanelRect {
    fn from_panel(panel: &DiegeticPanel, window_size: Vec2, angle: f32) -> Option<Self> {
        let anchor_position = panel::screen_anchor_position(panel, window_size, None).ok()?;
        let size = Vec2::new(panel.width(), panel.height());
        let bounds =
            PanelScreenBounds::from_anchor_position(anchor_position, panel.anchor(), size).ok()?;
        Some(Self {
            anchor_position,
            anchor: panel.anchor(),
            size,
            angle,
            bounds: Some(bounds),
            layout_unit: panel.layout_unit(),
        })
    }

    pub(super) const fn bounds(self) -> Option<PanelScreenBounds> { self.bounds }

    pub(super) const fn layout_unit(self) -> Unit { self.layout_unit }

    pub(super) const fn angle(self) -> f32 { self.angle }

    pub(super) fn with_anchor_position_and_angle(
        self,
        anchor_position: Vec2,
        angle: Option<f32>,
    ) -> Self {
        let bounds =
            PanelScreenBounds::from_anchor_position(anchor_position, self.anchor, self.size).ok();
        Self {
            anchor_position,
            angle: angle.unwrap_or(self.angle),
            bounds,
            ..self
        }
    }
}

pub(super) fn screen_panel_rects(
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    resolved_positions: &Query<&mut ResolvedScreenPanelPosition>,
    transforms: &Query<&Transform>,
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
        let authored_rotation = resolved_positions
            .get(entity)
            .ok()
            .and_then(|resolved_position| resolved_position.authored_rotation);
        let angle = authored_rotation.unwrap_or_else(|| {
            transforms
                .get(entity)
                .map_or(0.0, |transform| screen_in_plane_angle(transform.rotation))
        });
        if let Some(rect) = ScreenPanelRect::from_panel(panel, window_size, angle) {
            rects.insert(entity, rect);
        }
    }
    rects
}

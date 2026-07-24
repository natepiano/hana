//! Per-frame screen-space panel rectangles used during attachment resolution.

use bevy::camera::visibility::RenderLayers;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::projection;
use super::screen_in_plane_angle;
use super::window;
use crate::layout::Anchor;
use crate::layout::Unit;
use crate::panel;
use crate::panel::CoordinateSpace;
use crate::panel::DiegeticPanel;
use crate::panel::PanelScreenBounds;
use crate::panel::ResolvedScreenPanelPosition;
use crate::widgets::WidgetAnchorRect;

/// Placement and presentation data for a general screen-space anchor target.
#[derive(Clone, Component, Debug)]
pub struct ScreenAnchorTarget {
    bounds:        PanelScreenBounds,
    window:        Entity,
    camera_order:  isize,
    render_layers: RenderLayers,
    layout_unit:   Unit,
}

impl ScreenAnchorTarget {
    /// Creates general screen-space anchor data.
    #[must_use]
    pub const fn new(
        bounds: PanelScreenBounds,
        window: Entity,
        camera_order: isize,
        render_layers: RenderLayers,
        layout_unit: Unit,
    ) -> Self {
        Self {
            bounds,
            window,
            camera_order,
            render_layers,
            layout_unit,
        }
    }

    /// Current target rectangle in logical pixels.
    #[must_use]
    pub const fn bounds(&self) -> PanelScreenBounds { self.bounds }

    /// Window containing the target rectangle.
    #[must_use]
    pub const fn window(&self) -> Entity { self.window }

    /// Camera order used to present attachments.
    #[must_use]
    pub const fn camera_order(&self) -> isize { self.camera_order }

    /// Render layers used to present attachments.
    #[must_use]
    pub const fn render_layers(&self) -> &RenderLayers { &self.render_layers }

    /// Pixel-unit context for authored offsets and attached panels.
    #[must_use]
    pub const fn layout_unit(&self) -> Unit { self.layout_unit }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScreenPanelRect {
    pub(super) anchor_position: Vec2,
    pub(super) anchor:          Anchor,
    size:                       Vec2,
    angle:                      f32,
    bounds:                     Option<PanelScreenBounds>,
    layout_scale:               Vec2,
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
            layout_scale: Vec2::ONE,
            layout_unit: panel.layout_unit(),
        })
    }

    pub(super) const fn bounds(self) -> Option<PanelScreenBounds> { self.bounds }

    pub(super) const fn layout_unit(self) -> Unit { self.layout_unit }

    pub(super) const fn layout_scale(self) -> Vec2 { self.layout_scale }

    pub(super) const fn angle(self) -> f32 { self.angle }

    pub(super) fn from_widget(
        owner: Self,
        widget: WidgetAnchorRect,
        owner_transform: &Transform,
    ) -> Option<Self> {
        let scale = projection::screen_panel_scale(owner_transform)?;
        let panel_offset = widget.panel_offset().truncate();
        let anchor_position = owner.anchor_position
            + projection::project_panel_local_offset(panel_offset, scale, owner.angle);
        let size = widget.size() * scale.abs();
        let bounds =
            PanelScreenBounds::from_anchor_position(anchor_position, Anchor::Center, size).ok()?;
        Some(Self {
            anchor_position,
            anchor: Anchor::Center,
            size,
            angle: owner.angle,
            bounds: Some(bounds),
            layout_scale: scale,
            layout_unit: owner.layout_unit,
        })
    }

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

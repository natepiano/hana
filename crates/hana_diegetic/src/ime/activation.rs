//! Panel picking activation for authored editable fields.

use bevy::camera::NormalizedRenderTarget;
use bevy::diagnostic::FrameCount;
use bevy::ecs::entity::ContainsEntity;
use bevy::prelude::*;

use super::ImeOpenSession;
use super::ImeTarget;
use super::editor::PendingImePanelAnchor;
use crate::ComputedDiegeticPanel;
use crate::DiegeticPanel;
use crate::render;

pub(super) fn observe_panel_clicks(trigger: On<Add, DiegeticPanel>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .observe(open_from_panel_click);
}

fn open_from_panel_click(
    mut click: On<Pointer<Click>>,
    panels: Query<(&DiegeticPanel, &ComputedDiegeticPanel, &GlobalTransform)>,
    frame_count: Option<Res<FrameCount>>,
    mut pending_anchor: ResMut<PendingImePanelAnchor>,
    mut commands: Commands,
) {
    if click.button != PointerButton::Primary || click.count < 2 {
        return;
    }

    let panel_entity = click.event_target();
    let Ok((panel, computed, transform)) = panels.get(panel_entity) else {
        return;
    };
    let Some(panel_local) = click
        .hit
        .position
        .and_then(|position| render::project_flat_panel_hit(position, panel, transform))
    else {
        return;
    };
    let Some(record) = computed.field_at_local_position(panel_local) else {
        return;
    };
    let Some(window) = pointer_window(&click) else {
        return;
    };

    click.propagate(false);
    pending_anchor.store(
        panel_entity,
        record.field_id.clone(),
        click.hit.camera,
        window,
    );
    let target = if panel.coordinate_space().is_screen() {
        ImeTarget::ScreenPanelField {
            panel:    panel_entity,
            field_id: record.field_id.clone(),
        }
    } else {
        ImeTarget::WorldPanelField {
            panel:    panel_entity,
            field_id: record.field_id.clone(),
        }
    };

    commands.trigger(ImeOpenSession {
        window,
        target,
        initial_text: record.display_text.clone(),
        field_spec: record.field_spec.clone(),
        anchor: None,
    });

    if let Some(frame_count) = frame_count {
        bevy::log::trace!(
            target: "hana_diegetic::ime",
            "captured editable field activation on frame {}",
            frame_count.0
        );
    }
}

fn pointer_window(click: &Pointer<Click>) -> Option<Entity> {
    match &click.pointer_location.target {
        NormalizedRenderTarget::Window(window_ref) => Some(window_ref.entity()),
        NormalizedRenderTarget::Image(_)
        | NormalizedRenderTarget::TextureView(_)
        | NormalizedRenderTarget::None { .. } => None,
    }
}

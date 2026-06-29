//! Draw-order capacity warnings shared by SDF, text, and panel-line renderers.

use bevy::log::warn_once;
use bevy::prelude::*;
use bevy_kana::ToUsize;

use super::constants::COMMAND_SORT_OFFSET_CAPACITY;
use super::constants::OIT_DEPTH_STEP;
use super::constants::OIT_FOCUS_DEPTH;
use crate::layout::DrawZIndex;
use crate::panel::ComputedDiegeticPanel;

/// Warns when a panel's draw-order projection reaches screen or OIT limits.
pub(super) fn warn_panel_draw_order_limits(
    changed_panels: Query<(Entity, &ComputedDiegeticPanel), Changed<ComputedDiegeticPanel>>,
) {
    for (panel_entity, computed) in &changed_panels {
        let command_counts = computed.draw_order().command_counts_by_z_index();
        warn_panel_draw_order_limit_counts(panel_entity, &command_counts);
    }
}

fn warn_panel_draw_order_limit_counts(
    panel_entity: Entity,
    command_counts: &[(DrawZIndex, usize)],
) {
    if let Some((z_index, command_count)) = busiest_overflowing_z_index(command_counts) {
        warn_once!(
            "panel {:?} has {} draw commands at z-index {}, reaching the per-z-index screen band \
             cap ({}); coplanar geometry at that z-index reaches the shared line/text sub-lanes",
            panel_entity,
            command_count,
            i8::from(z_index),
            per_z_index_band_capacity(),
        );
    }

    let panel_total = panel_draw_command_count(command_counts);
    if oit_total_overflows(panel_total) {
        warn_once!(
            "panel {:?} has {} total draw commands, reaching the OIT depth budget ({}); the \
             panel-global ordinal span exhausts 24-bit OIT depth headroom and coplanar ordering \
             degrades to OIT-list insertion order",
            panel_entity,
            panel_total,
            oit_depth_budget(),
        );
    }
}

fn busiest_overflowing_z_index(
    command_counts: &[(DrawZIndex, usize)],
) -> Option<(DrawZIndex, usize)> {
    command_counts
        .iter()
        .copied()
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| per_z_index_band_overflows(*count))
}

fn panel_draw_command_count(command_counts: &[(DrawZIndex, usize)]) -> usize {
    command_counts.iter().map(|(_, count)| *count).sum()
}

fn per_z_index_band_capacity() -> usize {
    usize::try_from(COMMAND_SORT_OFFSET_CAPACITY).unwrap_or(usize::MAX)
}

fn per_z_index_band_overflows(busiest: usize) -> bool { busiest >= per_z_index_band_capacity() }

fn oit_depth_budget() -> usize {
    if OIT_DEPTH_STEP <= 0.0 {
        return usize::MAX;
    }
    (OIT_FOCUS_DEPTH / OIT_DEPTH_STEP).floor().to_usize()
}

fn oit_total_overflows(panel_total: usize) -> bool { panel_total >= oit_depth_budget() }

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;
    use crate::layout::BoundingBox;
    use crate::layout::DrawZIndex;
    use crate::layout::RectangleSource;
    use crate::layout::RenderCommand;
    use crate::layout::RenderCommandKind;
    use crate::layout::TextStyle;
    use crate::render::DrawOrderProjection;

    #[test]
    fn per_z_index_band_overflows_at_screen_band_capacity() {
        let capacity = per_z_index_band_capacity();

        assert!(!per_z_index_band_overflows(capacity.saturating_sub(1)));
        assert!(per_z_index_band_overflows(capacity));
    }

    #[test]
    fn oit_total_overflows_at_depth_budget() {
        let budget = oit_depth_budget();

        assert!(!oit_total_overflows(budget.saturating_sub(1)));
        assert!(oit_total_overflows(budget));
    }

    #[test]
    fn per_z_index_warning_detects_fill_only_panel() {
        assert_per_z_index_warning_for_commands(repeated_commands(
            rectangle(),
            per_z_index_band_capacity(),
        ));
    }

    #[test]
    fn per_z_index_warning_detects_text_only_panel() {
        assert_per_z_index_warning_for_commands(repeated_commands(
            text(),
            per_z_index_band_capacity(),
        ));
    }

    #[test]
    fn per_z_index_warning_detects_line_command_only_panel() {
        assert_per_z_index_warning_for_commands(repeated_commands(
            panel_shapes(),
            per_z_index_band_capacity(),
        ));
    }

    #[test]
    fn per_z_index_warning_detects_mixed_panel() {
        let kinds = [rectangle(), text(), panel_shapes()];
        let commands = (0..per_z_index_band_capacity())
            .map(|index| command(kinds[index % kinds.len()].clone(), index))
            .collect();

        assert_per_z_index_warning_for_commands(commands);
    }

    #[test]
    fn oit_warning_uses_full_draw_command_count() {
        let budget = oit_depth_budget();
        let projection = DrawOrderProjection::from_commands(&repeated_commands(text(), budget));
        let command_counts = projection.command_counts_by_z_index();

        assert_eq!(panel_draw_command_count(&command_counts), budget);
        assert!(oit_total_overflows(panel_draw_command_count(
            &command_counts
        )));
    }

    fn assert_per_z_index_warning_for_commands(commands: Vec<RenderCommand>) {
        let projection = DrawOrderProjection::from_commands(&commands);
        let command_counts = projection.command_counts_by_z_index();

        assert_eq!(
            busiest_overflowing_z_index(&command_counts),
            Some((DrawZIndex::default(), per_z_index_band_capacity())),
        );
    }

    fn repeated_commands(kind: RenderCommandKind, count: usize) -> Vec<RenderCommand> {
        (0..count)
            .map(|index| command(kind.clone(), index))
            .collect()
    }

    fn command(kind: RenderCommandKind, element_idx: usize) -> RenderCommand {
        RenderCommand {
            bounds: BoundingBox::default(),
            kind,
            element_idx,
            z_index: DrawZIndex::default(),
        }
    }

    fn rectangle() -> RenderCommandKind {
        RenderCommandKind::Rectangle {
            color:  Color::WHITE,
            source: RectangleSource::Background,
        }
    }

    fn panel_shapes() -> RenderCommandKind { RenderCommandKind::PanelShapes { shapes: Vec::new() } }

    fn text() -> RenderCommandKind {
        RenderCommandKind::Text {
            text:   String::new(),
            config: TextStyle::default(),
        }
    }
}

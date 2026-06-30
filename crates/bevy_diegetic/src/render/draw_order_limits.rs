//! Draw-order diagnostics shared by SDF, text, and panel-shape renderers.

use bevy::log::warn_once;
use bevy::prelude::*;
use bevy_kana::ToUsize;

use super::constants::OIT_DEPTH_STEP;
use super::constants::OIT_FOCUS_DEPTH;
use crate::layout::DrawZIndex;
use crate::panel::ComputedDiegeticPanel;

/// Warns when a panel's `DrawOrder` reaches the OIT depth budget.
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
    let panel_total = panel_draw_command_count(command_counts);
    if oit_total_overflows(panel_total) {
        warn_once!(
            "panel {:?} has {} total draw commands, reaching the OIT depth budget ({}); the \
             panel-global draw-order index span exhausts 24-bit OIT depth headroom and coplanar \
             ordering degrades to OIT-list insertion order",
            panel_entity,
            panel_total,
            oit_depth_budget(),
        );
    }
}

fn panel_draw_command_count(command_counts: &[(DrawZIndex, usize)]) -> usize {
    command_counts.iter().map(|(_, count)| *count).sum()
}

fn oit_depth_budget() -> usize {
    if OIT_DEPTH_STEP <= 0.0 {
        return usize::MAX;
    }
    (OIT_FOCUS_DEPTH / OIT_DEPTH_STEP).floor().to_usize()
}

fn oit_total_overflows(panel_total: usize) -> bool { panel_total >= oit_depth_budget() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::BoundingBox;
    use crate::layout::DrawZIndex;
    use crate::layout::RenderCommand;
    use crate::layout::RenderCommandKind;
    use crate::layout::TextStyle;
    use crate::render::DrawOrder;

    #[test]
    fn oit_total_overflows_at_depth_budget() {
        let budget = oit_depth_budget();

        assert!(!oit_total_overflows(budget.saturating_sub(1)));
        assert!(oit_total_overflows(budget));
    }

    #[test]
    fn single_z_index_count_below_oit_budget_does_not_overflow() {
        let budget = oit_depth_budget();
        let command_counts = [(DrawZIndex::default(), budget.saturating_sub(1))];

        assert!(!oit_total_overflows(panel_draw_command_count(
            &command_counts
        )));
    }

    #[test]
    fn oit_warning_uses_full_draw_command_count() {
        let budget = oit_depth_budget();
        let draw_order = DrawOrder::from_commands(&repeated_commands(text(), budget));
        let command_counts = draw_order.command_counts_by_z_index();

        assert_eq!(panel_draw_command_count(&command_counts), budget);
        assert!(oit_total_overflows(panel_draw_command_count(
            &command_counts
        )));
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

    fn text() -> RenderCommandKind {
        RenderCommandKind::Text {
            text:   String::new(),
            config: TextStyle::default(),
        }
    }
}

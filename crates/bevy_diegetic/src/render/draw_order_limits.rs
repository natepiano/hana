//! Draw-order capacity warnings shared by SDF, text, and panel-line renderers.

use bevy::log::warn_once;
use bevy::prelude::*;
use bevy_kana::ToUsize;

use super::constants::DRAW_LEVEL_GEOMETRY_LANES;
use super::constants::OIT_DEPTH_STEP;
use super::constants::OIT_FOCUS_DEPTH;
use crate::panel::ComputedDiegeticPanel;

/// Warns when a panel's draw-order projection reaches screen or OIT limits.
pub(super) fn warn_panel_draw_order_limits(
    changed_panels: Query<(Entity, &ComputedDiegeticPanel), Changed<ComputedDiegeticPanel>>,
) {
    for (panel_entity, computed) in &changed_panels {
        let occupancy = computed.draw_order().level_occupancy();
        warn_panel_draw_order_limit_occupancy(panel_entity, &occupancy);
    }
}

fn warn_panel_draw_order_limit_occupancy(panel_entity: Entity, occupancy: &[(i8, usize)]) {
    if let Some((z_level, level_count)) = busiest_overflowing_level(occupancy) {
        warn_once!(
            "panel {:?} has {} draw commands at z-level {}, reaching the per-level screen band \
             cap ({}); coplanar geometry at that level reaches the shared line/text sub-lanes",
            panel_entity,
            level_count,
            z_level,
            per_level_band_capacity(),
        );
    }

    let panel_total = panel_draw_command_count(occupancy);
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

fn busiest_overflowing_level(occupancy: &[(i8, usize)]) -> Option<(i8, usize)> {
    occupancy
        .iter()
        .copied()
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| per_level_band_overflows(*count))
}

fn panel_draw_command_count(occupancy: &[(i8, usize)]) -> usize {
    occupancy.iter().map(|(_, count)| *count).sum()
}

fn per_level_band_capacity() -> usize {
    usize::try_from(DRAW_LEVEL_GEOMETRY_LANES).unwrap_or(usize::MAX)
}

fn per_level_band_overflows(busiest: usize) -> bool { busiest >= per_level_band_capacity() }

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
    fn per_level_band_overflows_at_screen_band_capacity() {
        let capacity = per_level_band_capacity();

        assert!(!per_level_band_overflows(capacity.saturating_sub(1)));
        assert!(per_level_band_overflows(capacity));
    }

    #[test]
    fn oit_total_overflows_at_depth_budget() {
        let budget = oit_depth_budget();

        assert!(!oit_total_overflows(budget.saturating_sub(1)));
        assert!(oit_total_overflows(budget));
    }

    #[test]
    fn per_level_warning_detects_fill_only_panel() {
        assert_per_level_warning_for_commands(repeated_commands(
            rectangle(),
            per_level_band_capacity(),
        ));
    }

    #[test]
    fn per_level_warning_detects_text_only_panel() {
        assert_per_level_warning_for_commands(repeated_commands(text(), per_level_band_capacity()));
    }

    #[test]
    fn per_level_warning_detects_line_command_only_panel() {
        assert_per_level_warning_for_commands(repeated_commands(
            lines(),
            per_level_band_capacity(),
        ));
    }

    #[test]
    fn per_level_warning_detects_mixed_panel() {
        let kinds = [rectangle(), text(), lines()];
        let commands = (0..per_level_band_capacity())
            .map(|index| command(kinds[index % kinds.len()].clone(), index))
            .collect();

        assert_per_level_warning_for_commands(commands);
    }

    #[test]
    fn oit_warning_uses_full_draw_command_count() {
        let budget = oit_depth_budget();
        let projection = DrawOrderProjection::from_commands(&repeated_commands(text(), budget));
        let occupancy = projection.level_occupancy();

        assert_eq!(panel_draw_command_count(&occupancy), budget);
        assert!(oit_total_overflows(panel_draw_command_count(&occupancy)));
    }

    fn assert_per_level_warning_for_commands(commands: Vec<RenderCommand>) {
        let projection = DrawOrderProjection::from_commands(&commands);
        let occupancy = projection.level_occupancy();

        assert_eq!(
            busiest_overflowing_level(&occupancy),
            Some((DrawZIndex::default().0, per_level_band_capacity())),
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

    fn lines() -> RenderCommandKind { RenderCommandKind::Shapes { shapes: Vec::new() } }

    fn text() -> RenderCommandKind {
        RenderCommandKind::Text {
            text:   String::new(),
            config: TextStyle::default(),
        }
    }
}

//! Placement writes for resolved screen-space panel attachments.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use super::candidate::AnchorResolveSkip;
use super::rect::ScreenPanelRect;
use crate::panel::AnchoredToPanel;
use crate::panel::AttachmentResolveAction;
use crate::panel::AttachmentResolveReasons;
use crate::panel::DiegeticPanel;
use crate::panel::ResolvedScreenPanelPosition;

pub(super) fn desired_position_map(
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
) -> HashMap<Entity, Option<Vec2>> {
    let mut desired_positions = HashMap::default();
    for (entity, _) in panels {
        desired_positions.insert(entity, None);
    }
    desired_positions
}

pub(super) fn write_desired_positions(
    desired_positions: HashMap<Entity, Option<Vec2>>,
    resolved_positions: &mut Query<&mut ResolvedScreenPanelPosition>,
) {
    for (entity, anchor_position) in desired_positions {
        let Ok(mut resolved_position) = resolved_positions.get_mut(entity) else {
            continue;
        };
        if resolved_position.anchor_position != anchor_position {
            resolved_position.anchor_position = anchor_position;
        }
    }
}

pub(super) struct ScreenAttachmentPlacer<'a> {
    pub(super) rects:             &'a mut HashMap<Entity, ScreenPanelRect>,
    pub(super) desired_positions: &'a mut HashMap<Entity, Option<Vec2>>,
}

impl ScreenAttachmentPlacer<'_> {
    pub(super) fn handle(
        &mut self,
        action: AttachmentResolveAction,
    ) -> Result<(), AnchorResolveSkip> {
        match action {
            AttachmentResolveAction::Place {
                source,
                target,
                attachment,
            } => self.place(source, target, attachment),
            AttachmentResolveAction::Fallback { source } => {
                self.fallback(source);
                Ok(())
            },
        }
    }

    fn place(
        &mut self,
        source: Entity,
        target: Entity,
        attachment: AnchoredToPanel,
    ) -> Result<(), AnchorResolveSkip> {
        let Some(target_rect) = self.rects.get(&target).copied() else {
            return Err(AnchorResolveSkip::TargetWithoutPanel);
        };
        let Some(source_rect) = self.rects.get(&source).copied() else {
            return Err(AnchorResolveSkip::SourceWithoutPanel);
        };
        let Some(target_bounds) = target_rect.bounds() else {
            return Err(AnchorResolveSkip::TargetWithoutPanel);
        };
        let Some(source_bounds) = source_rect.bounds() else {
            return Err(AnchorResolveSkip::SourceWithoutPanel);
        };

        let target_point = target_bounds.point(attachment.target_anchor)
            + attachment.offset.to_layout_units(target_rect.layout_unit());
        let source_offset = source_bounds.anchor_offset(attachment.source_anchor);
        let panel_offset = source_bounds.anchor_offset(source_rect.anchor);
        let top_left = target_point - source_offset;
        let anchor_position = top_left + panel_offset;

        self.desired_positions.insert(source, Some(anchor_position));
        self.rects
            .insert(source, source_rect.with_anchor_position(anchor_position));
        Ok(())
    }

    fn fallback(&mut self, source: Entity) { self.desired_positions.insert(source, None); }
}

pub(super) const fn screen_attachment_resolve_reasons()
-> AttachmentResolveReasons<AnchorResolveSkip> {
    AttachmentResolveReasons {
        blocked_by_skipped_dependency: AnchorResolveSkip::BlockedBySkippedDependency,
        cycle:                         AnchorResolveSkip::Cycle,
        blocked_by_cycle:              AnchorResolveSkip::BlockedByCycle,
    }
}

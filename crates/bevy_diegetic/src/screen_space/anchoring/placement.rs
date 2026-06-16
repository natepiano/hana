//! Placement writes for resolved screen-space panel attachments.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use super::candidate::AnchorResolveSkip;
use super::rect::ScreenPanelRect;
use super::rotate_screen_offset;
use super::screen_in_plane_angle;
use crate::layout::Anchor;
use crate::panel::AnchoredToPanel;
use crate::panel::AttachmentResolveAction;
use crate::panel::AttachmentResolveReasons;
use crate::panel::DiegeticPanel;
use crate::panel::PanelAnchorPose;
use crate::panel::PanelScreenBounds;
use crate::panel::ResolvedScreenPanelPosition;

/// Per-frame resolver output for one panel: fields stay `None` on fallback so
/// the panel returns to its configured position and authored transform fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct DesiredScreenPlacement {
    pub(super) anchor_position: Option<Vec2>,
    pub(super) depth:           Option<f32>,
    pub(super) rotation:        Option<f32>,
}

pub(super) fn desired_placement_map(
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
) -> HashMap<Entity, DesiredScreenPlacement> {
    let mut desired_placements = HashMap::default();
    for (entity, _) in panels {
        desired_placements.insert(entity, DesiredScreenPlacement::default());
    }
    desired_placements
}

/// Current z of every screen panel, seeding chain depth accumulation.
///
/// Panels whose depth the resolver overwrote on a previous frame seed from
/// their captured authored z, so a detached chain root contributes its
/// authored depth in the same update.
pub(super) fn panel_depths(
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    resolved_positions: &Query<&mut ResolvedScreenPanelPosition>,
    transforms: &Query<&Transform>,
) -> HashMap<Entity, f32> {
    let mut depths = HashMap::default();
    for (entity, _) in panels {
        let authored_depth = resolved_positions
            .get(entity)
            .ok()
            .and_then(|resolved| resolved.authored_depth);
        let Some(depth) = authored_depth.or_else(|| {
            transforms
                .get(entity)
                .ok()
                .map(|transform| transform.translation.z)
        }) else {
            continue;
        };
        depths.insert(entity, depth);
    }
    depths
}

pub(super) fn panel_anchor_pose_map(
    anchor_poses: &Query<(Entity, &PanelAnchorPose)>,
) -> HashMap<Entity, PanelAnchorPose> {
    let mut pose_by_entity = HashMap::default();
    for (entity, pose) in anchor_poses {
        pose_by_entity.insert(entity, *pose);
    }
    pose_by_entity
}

pub(super) fn write_desired_placements(
    desired_placements: HashMap<Entity, DesiredScreenPlacement>,
    resolved_positions: &mut Query<&mut ResolvedScreenPanelPosition>,
) {
    for (entity, desired) in desired_placements {
        let Ok(mut resolved_position) = resolved_positions.get_mut(entity) else {
            continue;
        };
        if resolved_position.anchor_position != desired.anchor_position {
            resolved_position.anchor_position = desired.anchor_position;
        }
        if resolved_position.depth != desired.depth {
            resolved_position.depth = desired.depth;
        }
        if resolved_position.rotation != desired.rotation {
            resolved_position.rotation = desired.rotation;
        }
    }
}

pub(super) struct ScreenAttachmentPlacer<'a> {
    pub(super) rects:              &'a mut HashMap<Entity, ScreenPanelRect>,
    pub(super) desired_placements: &'a mut HashMap<Entity, DesiredScreenPlacement>,
    pub(super) depths:             &'a mut HashMap<Entity, f32>,
    pub(super) anchor_poses:       &'a HashMap<Entity, PanelAnchorPose>,
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

        let anchor_pose = self.anchor_poses.get(&source).copied();
        let pose_translation = anchor_pose.map_or(Vec3::ZERO, |pose| pose.translation);
        let pose_angle = anchor_pose.map(|pose| screen_in_plane_angle(pose.rotation));
        let offset = attachment.offset.to_layout_units(target_rect.layout_unit());
        let target_point =
            oriented_anchor_point(target_rect, target_bounds, attachment.target_anchor)
                + rotate_screen_offset(offset.truncate(), target_rect.angle())
                + Vec2::new(pose_translation.x, -pose_translation.y);
        let source_offset = source_bounds.anchor_offset(attachment.source_anchor);
        let panel_offset = source_bounds.anchor_offset(source_rect.anchor);
        let angle = pose_angle.unwrap_or_else(|| source_rect.angle());
        let top_left = target_point - rotate_screen_offset(source_offset, angle);
        let anchor_position = top_left + rotate_screen_offset(panel_offset, angle);
        let authors_depth = offset.z != 0.0 || pose_translation.z != 0.0;
        let depth = self.placed_depth(source, target, offset.z + pose_translation.z, authors_depth);

        self.desired_placements.insert(
            source,
            DesiredScreenPlacement {
                anchor_position: Some(anchor_position),
                depth,
                rotation: pose_angle,
            },
        );
        self.rects.insert(
            source,
            source_rect.with_anchor_position_and_angle(anchor_position, pose_angle),
        );
        Ok(())
    }

    /// Resolves depth as the target's depth plus the authored z inputs.
    ///
    /// Depth resolves only when the attachment or pose authors z, or when the
    /// target's own depth was resolved (chain propagation); otherwise the
    /// source keeps its authored z untouched.
    fn placed_depth(
        &mut self,
        source: Entity,
        target: Entity,
        offset_z: f32,
        authors_depth: bool,
    ) -> Option<f32> {
        let target_depth_resolved = self
            .desired_placements
            .get(&target)
            .is_some_and(|desired| desired.depth.is_some());
        if !authors_depth && !target_depth_resolved {
            return None;
        }
        let target_depth = self.depths.get(&target).copied().unwrap_or(0.0);
        let depth = target_depth + offset_z;
        self.depths.insert(source, depth);
        Some(depth)
    }

    fn fallback(&mut self, source: Entity) {
        self.desired_placements
            .insert(source, DesiredScreenPlacement::default());
    }
}

fn oriented_anchor_point(rect: ScreenPanelRect, bounds: PanelScreenBounds, anchor: Anchor) -> Vec2 {
    let resolved_anchor_offset = bounds.anchor_offset(anchor);
    let panel_offset = bounds.anchor_offset(rect.anchor);
    rect.anchor_position + rotate_screen_offset(resolved_anchor_offset - panel_offset, rect.angle())
}

pub(super) const fn screen_attachment_resolve_reasons()
-> AttachmentResolveReasons<AnchorResolveSkip> {
    AttachmentResolveReasons {
        blocked_by_skipped_dependency: AnchorResolveSkip::BlockedBySkippedDependency,
        cycle:                         AnchorResolveSkip::Cycle,
        blocked_by_cycle:              AnchorResolveSkip::BlockedByCycle,
    }
}

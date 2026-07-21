//! Screen-space panel attachment resolution.

mod candidate;
mod placement;
mod projection;
mod rect;
mod resolve;
mod window;

use bevy::prelude::*;
pub(crate) use candidate::AnchorResolveSkip;
use hana_valence::AnchorPose;
pub(crate) use resolve::AnchorResolveDiagnostics;

use super::CandidateQueries;
use crate::panel::PanelAnchorOffset;
use crate::panel::PanelAttachmentAuthored;
use crate::panel::ResolvedScreenPanelPosition;

pub(super) fn resolve_screen_space_panel_attachments(
    windows: Query<(Entity, &Window)>,
    attachments: Query<(Entity, &PanelAttachmentAuthored, &PanelAnchorOffset)>,
    anchor_poses: Query<(Entity, &AnchorPose)>,
    candidate_queries: CandidateQueries,
    resolved_positions: Query<&mut ResolvedScreenPanelPosition>,
    diagnostics: ResMut<AnchorResolveDiagnostics>,
) {
    resolve::resolve_screen_space_panel_attachments(
        windows,
        attachments,
        anchor_poses,
        candidate_queries,
        resolved_positions,
        diagnostics,
    );
}

pub(super) fn rotate_screen_offset(offset: Vec2, angle: f32) -> Vec2 {
    projection::rotate_screen_offset(offset, angle)
}

pub(crate) fn screen_in_plane_angle(rotation: Quat) -> f32 {
    projection::screen_in_plane_angle(rotation)
}

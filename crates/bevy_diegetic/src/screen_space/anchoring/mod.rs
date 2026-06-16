//! Screen-space panel attachment resolution.

mod candidate;
mod placement;
mod projection;
mod rect;
mod resolve;
mod window;

use bevy::prelude::*;
pub(crate) use candidate::AnchorResolveSkip;
pub(crate) use resolve::AnchorResolveDiagnostics;
pub(crate) use resolve::resolve_screen_space_panel_attachments;

pub(super) fn rotate_screen_offset(offset: Vec2, angle: f32) -> Vec2 {
    projection::rotate_screen_offset(offset, angle)
}

pub(super) fn screen_in_plane_angle(rotation: Quat) -> f32 {
    projection::screen_in_plane_angle(rotation)
}

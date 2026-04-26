//! Callout primitives for annotations.
//!
//! Public callout types render directly from SDF meshes/materials. Legacy
//! gizmo helpers remain crate-internal while the typography overlay is
//! migrated.

mod caps;
mod constants;
mod line;
mod render;

use bevy::prelude::App;
use bevy::prelude::Color;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::GizmoAsset;
use bevy::prelude::Plugin;
use bevy::prelude::PostUpdate;
use bevy::prelude::Vec3;
pub use caps::ArrowStyle;
pub use caps::CalloutCap;
pub use line::CalloutLine;

pub(crate) struct CalloutPlugin;

impl Plugin for CalloutPlugin {
    fn build(&self, app: &mut App) { app.add_systems(PostUpdate, render::update_callout_lines); }
}

pub(crate) fn spawn_callout_line(commands: &mut Commands, parent: Entity, line: &CalloutLine) {
    line::spawn_callout_line(commands, parent, line);
}

pub(crate) fn draw_dimension_arrow(
    gizmo: &mut GizmoAsset,
    from: Vec3,
    to: Vec3,
    color: Color,
    head_size: f32,
    gap: f32,
) {
    render::draw_dimension_arrow(gizmo, from, to, color, head_size, gap);
}

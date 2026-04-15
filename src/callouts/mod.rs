//! Callout primitives for annotations.
//!
//! Public callout types render directly from SDF meshes/materials. Legacy
//! gizmo helpers remain crate-internal while the typography overlay is
//! migrated.

use bevy::prelude::App;
use bevy::prelude::Plugin;
use bevy::prelude::PostUpdate;

mod primitives;

pub use primitives::ArrowStyle;
pub use primitives::CalloutCap;
pub use primitives::CalloutLine;
pub(crate) use primitives::draw_dashed_line;
pub(crate) use primitives::draw_dimension_arrow;
pub use primitives::spawn_callout_line;
pub(crate) use primitives::update_callout_lines;

pub(crate) struct CalloutPlugin;

impl Plugin for CalloutPlugin {
    fn build(&self, app: &mut App) { app.add_systems(PostUpdate, update_callout_lines); }
}

//! Callout primitives for annotations.
//!
//! Public callout types render directly from SDF meshes/materials.

mod caps;
mod constants;
mod line;
mod render;

use bevy::prelude::App;
use bevy::prelude::Plugin;
use bevy::prelude::PostUpdate;
pub use caps::ArrowStyle;
pub use caps::CalloutCap;
pub(crate) use caps::CalloutCapPrimitiveKind;
pub(crate) use caps::ResolvedCalloutCap;
pub(crate) use caps::ResolvedCalloutCapPrimitive;
pub use line::CalloutLine;

pub(crate) struct CalloutPlugin;

impl Plugin for CalloutPlugin {
    fn build(&self, app: &mut App) { app.add_systems(PostUpdate, render::update_callout_lines); }
}

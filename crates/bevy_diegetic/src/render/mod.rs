//! Rendering systems for diegetic UI panels and text.

mod clip;
mod constants;
mod panel_geometry;
mod panel_text;
mod sdf_material;
mod text_shaping;
mod transparency;
mod world_text;

use bevy::prelude::*;
pub(crate) use constants::LAYER_DEPTH_BIAS;
pub(crate) use constants::OIT_DEPTH_STEP;
pub(crate) use constants::SDF_AA_PADDING;
pub use constants::default_panel_material;
use panel_geometry::PanelGeometryPlugin;
pub use panel_text::PanelTextLayout;
use panel_text::TextRenderPlugin;
pub(crate) use sdf_material::SdfPanelMaterial;
pub(crate) use sdf_material::SdfPanelMaterialInput;
pub(crate) use sdf_material::SdfPrimitiveKind;
pub(crate) use sdf_material::SdfPrimitiveMaterialInput;
pub(crate) use sdf_material::sdf_panel_material;
pub(crate) use sdf_material::sdf_primitive_material;
pub use transparency::StableTransparency;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::PendingGlyphs;
pub use world_text::WorldText;
pub use world_text::WorldTextReady;

/// `PostUpdate` phase that spawns and despawns a panel's child entities —
/// text runs, images, glyph meshes, and SDF geometry.
///
/// Any system that reads a panel's [`Children`](bevy::prelude::Children) to act
/// on the child set — notably screen-space
/// [`RenderLayers`](bevy::camera::visibility::RenderLayers) propagation — must
/// be ordered `.after` this set. Reading the hierarchy mid-phase observes a
/// child that a reconcile system is despawning the same frame, which then
/// queues a command against an already-despawned entity and panics. Ordering
/// after the set inserts the sync point that applies those despawns (and the
/// `ChildOf` hooks that prune `Children`) first.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PanelChildSystems {
    /// Reconcile and mesh-build of every panel child entity.
    Build,
}

/// Umbrella render plugin — registers the render-side sub-plugins
/// (slug text, SDF panel geometry).
pub(crate) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((TextRenderPlugin, PanelGeometryPlugin))
            .add_observer(transparency::on_stable_transparency_added)
            .add_observer(transparency::on_stable_transparency_removed)
            .add_observer(transparency::on_screen_space_camera_added);
    }
}

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

use crate::text::TextMaterial;

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

/// Toggles sub-pixel supersampling of slug glyph coverage across all text
/// materials. On by default.
///
/// Supersampling evaluates each glyph's analytic coverage at four sub-pixel
/// offsets spanning the pixel footprint and averages them, anti-aliasing the
/// edge inside the fragment shader that already runs.
///
/// # Why it is the default
///
/// - **No extra pass.** SMAA, FXAA, and TAA each add a full-screen pass over the resolved frame,
///   with their own render-graph node and (for TAA) a history texture and depth/motion prepasses.
///   Supersampling adds none of that — it is extra work inside the coverage shader, nothing more.
/// - **Survives OIT.** Order-independent transparency forces `Msaa::Off`, which removes the only
///   hardware AA. Supersampling is independent of MSAA, so coplanar transparent text stays
///   anti-aliased at grazing angles, where a single coverage sample can't represent the
///   foreshortened pixel footprint and stair-steps.
/// - **Composes harmlessly.** If a post-process AA pass is present for other geometry,
///   supersampling does no harm — the two operate at different stages (coverage shader vs. resolved
///   frame) and don't interfere.
///
/// # Why it is a toggle
///
/// Each text fragment evaluates coverage four times instead of once, so the cost
/// scales with the number of pixels text covers. A frame dense with large text
/// is where that 4× is most visible; switching to [`Disabled`](Self::Disabled)
/// there trades grazing-angle edge quality for fill-rate. Distant or small text
/// covers few pixels, so it costs little regardless.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextSupersample {
    /// Evaluate coverage once per fragment.
    Disabled,
    /// Evaluate four sub-pixel coverage samples per fragment.
    #[default]
    Enabled,
}

impl TextSupersample {
    /// Flips between enabled and disabled supersampling.
    pub const fn toggle(&mut self) {
        *self = match self {
            Self::Disabled => Self::Enabled,
            Self::Enabled => Self::Disabled,
        };
    }
}

/// Mirrors [`TextSupersample`] into every text material's `supersample` uniform
/// whenever the setting changes.
fn sync_text_supersample(
    setting: Res<TextSupersample>,
    mut materials: ResMut<Assets<TextMaterial>>,
) {
    if !setting.is_changed() {
        return;
    }
    let value = match *setting {
        TextSupersample::Disabled => 0,
        TextSupersample::Enabled => 1,
    };
    for (_, material) in materials.iter_mut() {
        material.extension.uniforms.supersample = value;
    }
}

/// Umbrella render plugin — registers the render-side sub-plugins
/// (slug text, SDF panel geometry).
pub(crate) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((TextRenderPlugin, PanelGeometryPlugin))
            .init_resource::<TextSupersample>()
            .add_systems(Update, sync_text_supersample)
            .add_observer(transparency::on_stable_transparency_added)
            .add_observer(transparency::on_stable_transparency_removed)
            .add_observer(transparency::on_screen_space_camera_added);
    }
}

//! Rendering systems for diegetic UI panels and text.

pub(crate) mod analytic_paths;
mod batch_key;
#[cfg(feature = "batch_proof")]
pub(crate) mod batch_proof;
mod clip;
mod constants;
mod panel_geometry;
mod panel_lines;
mod panel_text;
mod sdf_material;
mod text_shaping;
mod transparency;
mod world_text;

use analytic_paths::AnalyticPathPlugin;
pub(crate) use analytic_paths::BandRecord;
pub(crate) use analytic_paths::BatchGpu;
pub(crate) use analytic_paths::BatchKey;
pub(crate) use analytic_paths::BatchTextMaterialInput;
pub(crate) use analytic_paths::Bounds;
pub(crate) use analytic_paths::CurveRecord;
pub(crate) use analytic_paths::DEFAULT_BAND_COUNT;
pub(crate) use analytic_paths::GlyphAtlasHandles;
pub(crate) use analytic_paths::GlyphBatchStore;
pub(crate) use analytic_paths::GlyphInstanceRecord;
pub(crate) use analytic_paths::GlyphOutline;
pub(crate) use analytic_paths::GlyphRecord;
pub(crate) use analytic_paths::PathContour;
pub(crate) use analytic_paths::PathOutline;
pub(crate) use analytic_paths::QuadraticSegment;
pub(crate) use analytic_paths::RenderMode;
pub(crate) use analytic_paths::RunRecord;
pub(crate) use analytic_paths::TextMaterial;
pub(crate) use analytic_paths::batch_text_material;
pub(crate) use analytic_paths::build_packed_path;
pub(crate) use analytic_paths::set_batch_text_material_buffers;
pub(crate) use analytic_paths::set_text_material_atlas;
#[cfg(feature = "batch_proof")]
pub(crate) use analytic_paths::toggle_text_material_debug_glyph_index;
pub(crate) use batch_key::BaseMaterialId;
pub(crate) use batch_key::BatchAlphaMode;
pub(crate) use batch_key::BatchRenderLayers;
pub(crate) use batch_key::VisualBatchKey;
pub(crate) use batch_key::VisualLighting;
pub(crate) use batch_key::VisualMaterialInterner;
pub(crate) use batch_key::VisualShadow;
pub(crate) use batch_key::VisualSidedness;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
pub(crate) use constants::LAYER_DEPTH_BIAS;
pub(crate) use constants::OIT_DEPTH_STEP;
pub(crate) use constants::SDF_AA_PADDING;
pub use constants::default_panel_material;
use panel_geometry::PanelGeometryPlugin;
use panel_lines::PanelLinePlugin;
pub use panel_text::DiegeticTextBatch;
pub use panel_text::DiegeticTextMut;
pub use panel_text::PanelText;
pub use panel_text::PanelTextLayout;
pub use panel_text::PanelTextReader;
pub use panel_text::PanelTextRuns;
pub use panel_text::TextEdit;
use panel_text::TextRenderPlugin;
pub use panel_text::TextRunOf;
pub(crate) use sdf_material::SdfPanelMaterial;
pub(crate) use sdf_material::SdfPanelMaterialInput;
pub(crate) use sdf_material::SdfPrimitiveKind;
pub(crate) use sdf_material::SdfPrimitiveMaterialInput;
pub(crate) use sdf_material::sdf_panel_material;
pub(crate) use sdf_material::sdf_primitive_material;
pub use transparency::StableTransparency;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::TextContent;
pub use world_text::WorldTextReady;
#[cfg(feature = "typography_overlay")]
pub(crate) use world_text::emit_computed_world_text;
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

/// Anti-aliasing applied to slug glyph edges, across all text materials.
///
/// Two orthogonal mechanisms back this, both running inside the coverage shader
/// (no extra pass, and they survive OIT, which forces `Msaa::Off`):
/// - a **screen-space band** sizes the edge ramp from the distance gradient so it stays ~1px per
///   screen axis — this keeps glyph edges crisp at grazing angles, where the scalar design-space
///   band otherwise widens the ramp into a blur.
/// - **anisotropic supersampling** strides several coverage samples along the foreshortened
///   footprint axis (one head-on, more as the angle steepens). Its one visible job is erasing the
///   wing a single band sample leaves off a sharp convex corner — and that wing only appears on
///   sharp convex corners at the most extreme viewing angles. Everywhere else it is a no-op.
///
/// The variants are the useful points on that quality/cost ladder.
///
/// # Performance
///
/// Cost is per text fragment and scales with how many pixels text covers.
/// [`Anisotropic`](Self::Anisotropic) is nearly free — one extra `fwidth` over the
/// baseline single sample. [`Supersample`](Self::Supersample) evaluates coverage at
/// four fixed sub-pixel points; [`Both`](Self::Both) instead strides one sample per
/// unit of footprint anisotropy (one head-on, up to 16 at the steepest grazing
/// angles), so it pays the cost only where the footprint is foreshortened. A frame
/// dense with large grazing text is where dropping to [`Anisotropic`](Self::Anisotropic)
/// — or [`Off`](Self::Off) — reclaims fill-rate.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAntiAlias {
    /// Scalar band, one sample. Edges blur at grazing angles and the sharp-corner
    /// wing shows. Mainly a reference.
    Off,
    /// Screen-space band, one sample. Crisp edges at every angle for almost no cost;
    /// the sharp-corner wing at the most extreme viewing angles remains.
    Anisotropic,
    /// Scalar band, four samples. Integrates the footprint, but without the
    /// screen-space band edges still blur at grazing — mainly a reference point.
    Supersample,
    /// Screen-space band plus anisotropic supersampling. Crisp edges at every angle
    /// AND erases the sharp-corner wing that the band alone leaves at the most
    /// extreme viewing angles. Highest cost.
    #[default]
    Both,
}

impl TextAntiAlias {
    /// Whether this mode supersamples the footprint (multiple samples vs. one).
    #[must_use]
    pub const fn supersamples(self) -> bool { matches!(self, Self::Supersample | Self::Both) }

    /// Whether this mode uses the screen-space anisotropic band (vs. scalar).
    #[must_use]
    pub const fn anisotropic(self) -> bool { matches!(self, Self::Anisotropic | Self::Both) }
}

/// Mirrors [`TextAntiAlias`] into every text material's `supersample` and
/// `aa_band` uniforms whenever the setting changes.
fn sync_text_anti_alias(setting: Res<TextAntiAlias>, mut materials: ResMut<Assets<TextMaterial>>) {
    if !setting.is_changed() {
        return;
    }
    for (_, material) in materials.iter_mut() {
        analytic_paths::set_text_material_anti_alias(
            material,
            setting.supersamples(),
            setting.anisotropic(),
        );
    }
}

/// Umbrella render plugin — registers the render-side sub-plugins
/// (slug text, SDF panel geometry).
pub(crate) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AnalyticPathPlugin,
            TextRenderPlugin,
            PanelGeometryPlugin,
            PanelLinePlugin,
        ))
        .init_resource::<TextAntiAlias>()
        // Bevy registers the OIT type without `ReflectComponent`; adding it
        // enables reflection-based (BRP) edits of OIT settings on a live camera.
        .register_type::<OrderIndependentTransparencySettings>()
        .register_type_data::<OrderIndependentTransparencySettings, ReflectComponent>()
        .add_systems(Update, sync_text_anti_alias)
        .add_observer(transparency::on_stable_transparency_added)
        .add_observer(transparency::on_stable_transparency_removed)
        .add_observer(transparency::on_screen_space_camera_added);
    }
}

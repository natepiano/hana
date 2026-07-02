//! Rendering systems for diegetic UI panels and text.

mod analytic_line_probe;
mod analytic_paths;
mod batch_key;
mod clip;
mod constants;
mod dirty;
mod draw_order;
mod fill_batch;
mod image_batch;
mod image_material;
mod material;
#[cfg(test)]
mod material_slot_lifetime_probe;
mod material_table;
mod panel_geometry;
mod panel_shapes;
mod panel_text;
mod precompose;
mod text_shaping;
mod transparency;
mod world_text;

pub use analytic_line_probe::AnalyticLine;
pub use analytic_line_probe::AnalyticLineProbe;
pub use analytic_line_probe::AnalyticLineProbePlugin;
use analytic_paths::AnalyticPathPlugin;
pub(crate) use analytic_paths::BandRecord;
pub(crate) use analytic_paths::Bounds;
pub(crate) use analytic_paths::CurveRecord;
pub(crate) use analytic_paths::DEFAULT_BAND_COUNT;
pub(crate) use analytic_paths::GeometryDirty;
pub(crate) use analytic_paths::MaterialDirty;
pub(crate) use analytic_paths::PackedPath;
pub(crate) use analytic_paths::PackedPathRecord;
pub(crate) use analytic_paths::PathAtlas;
pub(crate) use analytic_paths::PathAtlasHandles;
pub(crate) use analytic_paths::PathBatchKey;
pub(crate) use analytic_paths::PathBatchResources;
pub(crate) use analytic_paths::PathBatchStore;
pub(crate) use analytic_paths::PathContour;
pub(crate) use analytic_paths::PathExtendedMaterial;
#[allow(unused_imports, reason = "render module compatibility surface")]
pub(crate) use analytic_paths::PathExtension;
pub(crate) use analytic_paths::PathMaterialBuffers;
pub(crate) use analytic_paths::PathOutline;
pub(crate) use analytic_paths::PathQuadRecord;
pub(crate) use analytic_paths::PathRenderRecord;
pub(crate) use analytic_paths::PlacementDirty;
pub(crate) use analytic_paths::QuadraticSegment;
pub(crate) use analytic_paths::RenderMode;
pub(crate) use analytic_paths::analytic_material_slot_candidate;
pub(crate) use analytic_paths::build_packed_path;
#[cfg(test)]
pub(crate) use analytic_paths::path_material_oit_depth_offset;
pub(crate) use analytic_paths::set_path_material_atlas;
pub(crate) use analytic_paths::set_path_material_record_buffers;
pub(crate) use analytic_paths::set_path_material_table_buffer;
pub(crate) use batch_key::BatchAlphaMode;
pub(crate) use batch_key::BatchRenderLayers;
pub(crate) use batch_key::VisualShadow;
pub(crate) use batch_key::batch_summary;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::log::warn_once;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
pub(crate) use dirty::Dirty;
pub(crate) use draw_order::CommandIndex;
pub(crate) use draw_order::DrawOrder;
pub(crate) use draw_order::ElementIndex;
#[expect(
    unused_imports,
    reason = "Phase 2 publishes shared ordinals before Phase 9 panel-shape routing consumes them"
)]
pub(crate) use draw_order::PrimitiveOrdinal;
#[expect(
    unused_imports,
    reason = "Phase 2 publishes shared ordinals before Phase 9 panel-shape routing consumes them"
)]
pub(crate) use draw_order::ShapeOrdinal;
use fill_batch::FillBatchPlugin;
pub(crate) use fill_batch::SdfExtendedMaterial;
pub(crate) use fill_batch::set_sdf_material_table_buffer;
use image_batch::ImageBatchPlugin;
pub(crate) use material::apply_sidedness;
pub use material::default_panel_material;
pub(crate) use material::material_asset_for_frame;
use material_table::MaterialTablePlugin;
use panel_geometry::PanelGeometryPlugin;
use panel_shapes::PanelShapePlugin;
pub use panel_text::DiegeticTextBatch;
pub use panel_text::DiegeticTextMut;
pub use panel_text::PanelText;
pub use panel_text::PanelTextLayout;
pub use panel_text::PanelTextReader;
pub use panel_text::PanelTextRuns;
pub use panel_text::TextEdit;
use panel_text::TextRenderPlugin;
pub use panel_text::TextRunOf;
pub use transparency::StableTransparency;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::TextContent;
pub use world_text::WorldTextReady;
#[cfg(feature = "typography_overlay")]
pub(crate) use world_text::emit_computed_world_text;

use crate::cascade::CascadeDefault;
use crate::cascade::CascadePlugin;
use crate::cascade::CascadeSet;
use crate::cascade::SdfMaterial;
use crate::cascade::ShapeMaterial;
use crate::cascade::TextMaterial;
/// `PostUpdate` phase that spawns and despawns a panel's child entities —
/// text runs, glyph meshes, and SDF geometry.
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
/// This resource is the cascade root default: `sync_anti_alias` mirrors
/// it into `CascadeDefault<AntiAlias>`, and entities override it per
/// panel or per label via
/// [`override_anti_alias`](crate::cascade::CascadeEntityCommandsExt::override_anti_alias)
/// (line elements override via [`El::anti_alias`](crate::El::anti_alias)).
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
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Resource)]
pub enum AntiAlias {
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

/// `PathRenderRecord::aa_flags` bit for footprint supersampling. Mirrored by
/// `AA_FLAG_SUPERSAMPLE` in `analytic_path.wgsl`.
pub(crate) const AA_FLAG_SUPERSAMPLE: u32 = 1;

/// `PathRenderRecord::aa_flags` bit for the screen-space anisotropic edge band.
/// Mirrored by `AA_FLAG_BAND` in `analytic_path.wgsl`.
pub(crate) const AA_FLAG_BAND: u32 = 1 << 1;

impl AntiAlias {
    /// Whether this mode supersamples the footprint (multiple samples vs. one).
    #[must_use]
    pub const fn supersamples(self) -> bool { matches!(self, Self::Supersample | Self::Both) }

    /// Whether this mode uses the screen-space anisotropic band (vs. scalar).
    #[must_use]
    pub const fn anisotropic(self) -> bool { matches!(self, Self::Anisotropic | Self::Both) }

    /// The `PathRenderRecord::aa_flags` encoding of this mode — the one enum→bits
    /// conversion site, so the GPU encoding cannot drift from the variants.
    #[must_use]
    pub(crate) const fn aa_flags(self) -> u32 {
        let supersample = if self.supersamples() {
            AA_FLAG_SUPERSAMPLE
        } else {
            0
        };
        let band = if self.anisotropic() { AA_FLAG_BAND } else { 0 };
        supersample | band
    }
}

/// Mirrors [`AntiAlias`] into every text material's `supersample` and
/// `aa_band` uniforms and into the attribute's cascade root default whenever
/// the setting changes. The cascade write re-resolves every participant's
/// `Resolved<AntiAlias>`, which re-packs run records — per-record
/// `aa_flags` cannot be refreshed by rewriting a material uniform.
fn sync_anti_alias(
    setting: Res<AntiAlias>,
    mut cascade_default: ResMut<CascadeDefault<AntiAlias>>,
    mut materials: ResMut<Assets<PathExtendedMaterial>>,
) {
    if !setting.is_changed() {
        return;
    }
    if cascade_default.0 != *setting {
        cascade_default.0 = *setting;
    }
    for (_, material) in materials.iter_mut() {
        analytic_paths::set_path_material_anti_alias(material, *setting);
    }
}

/// Minimum on-screen width for hairline-dilated strokes (panel lines and
/// analytic lines), in logical pixels. The shader dilates any thinner stroke
/// up to this width, at full alpha.
///
/// The applied device-pixel value is `logical_px ×` the primary window's
/// scale factor, with a scale-dependent floor: 1.0 device pixels below
/// scale 2, 1.5 at scale 2 and above.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct HairlineWidth {
    /// Target stroke width in logical pixels.
    pub logical_px: f32,
    /// What happens to a stroke whose natural width falls below the floor.
    ///
    /// The cascade root default: `sync_hairline_fade` mirrors it into
    /// `CascadeDefault<HairlineFade>`, and line elements override it per
    /// panel via
    /// [`override_hairline_fade`](crate::cascade::CascadeEntityCommandsExt::override_hairline_fade),
    /// per element via [`El::hairline_fade`](crate::El::hairline_fade), or
    /// per line via [`PanelLine::hairline_fade`](crate::PanelLine::hairline_fade).
    pub fade:       HairlineFade,
}

impl Default for HairlineWidth {
    fn default() -> Self {
        Self {
            logical_px: 1.0,
            fade:       HairlineFade::Full,
        }
    }
}

/// Policy for strokes dilated up to the [`HairlineWidth`] floor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub enum HairlineFade {
    /// Dilate sub-floor strokes to the floor at full alpha (every stroke stays
    /// uniformly visible at any distance).
    #[default]
    Full,
    /// Dilate to the floor but scale alpha by
    /// `(natural_width / floor)^exponent`, so a stroke fades out as its
    /// natural on-screen width shrinks below the floor — a distance LOD for
    /// thin marks such as ruler ticks. Exponent `1.0` matches natural
    /// coverage; higher exponents fade sooner. Text glyphs are exempt
    /// structurally: their curves carry no dilating stroke width, so their
    /// fade factor is always 1.
    Fade {
        /// Fade curve exponent; must be positive and finite.
        exponent: f32,
    },
}

impl HairlineFade {
    /// The `CurveRecord::fade_exponent` encoding of this policy: `0.0`
    /// disables fade (`Full`), any positive value is the `Fade` exponent. The
    /// one validation site — the resource is Reflect/BRP-mutable, so a
    /// non-finite or non-positive exponent can arrive at runtime and falls
    /// back to `Full`.
    #[must_use]
    pub(crate) fn fade_exponent(self) -> f32 {
        match self {
            Self::Full => 0.0,
            Self::Fade { exponent } => {
                if exponent.is_finite() && exponent > 0.0 {
                    exponent
                } else {
                    warn_once!(
                        "HairlineFade::Fade exponent {exponent} is not positive and finite; \
                         rendering as HairlineFade::Full"
                    );
                    0.0
                }
            },
        }
    }
}

/// Floor for the applied hairline width at scale factor ≥ 2 — see
/// [`HairlineWidth`].
const HAIRLINE_MIN_DEVICE_PX_HIGH_DPI: f32 = 1.5;

/// Floor below scale factor 2 — the minimum applied hairline width when there
/// are few device pixels per stroke.
const HAIRLINE_MIN_DEVICE_PX_LOW_DPI: f32 = 1.0;

/// Mirrors [`HairlineWidth::fade`] into the attribute's cascade root default
/// whenever the resource changes. The cascade write re-resolves every
/// participant's `Resolved<HairlineFade>`, which re-packs line outlines (fade
/// is per-curve data in `CurveRecord::fade_exponent`).
fn sync_hairline_fade(
    hairline_width: Res<HairlineWidth>,
    mut cascade_default: ResMut<CascadeDefault<HairlineFade>>,
) {
    if !hairline_width.is_changed() {
        return;
    }
    if cascade_default.0 != hairline_width.fade {
        cascade_default.0 = hairline_width.fade;
    }
}

/// Mirrors [`HairlineWidth`] × window scale factor into every text material's
/// `hairline_min_px` uniform: all materials when the applied value changes,
/// plus newly added materials (which carry a constructor default).
fn sync_hairline_width(
    hairline_width: Res<HairlineWidth>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut materials: ResMut<Assets<PathExtendedMaterial>>,
    mut material_events: MessageReader<AssetEvent<PathExtendedMaterial>>,
    mut applied_device_px: Local<Option<f32>>,
) {
    let scale_factor = windows.iter().next().map_or(1.0, Window::scale_factor);
    let floor = if scale_factor >= 2.0 {
        HAIRLINE_MIN_DEVICE_PX_HIGH_DPI
    } else {
        HAIRLINE_MIN_DEVICE_PX_LOW_DPI
    };
    let device_px = (hairline_width.logical_px * scale_factor).max(floor);
    if *applied_device_px != Some(device_px) {
        *applied_device_px = Some(device_px);
        material_events.clear();
        for (_, material) in materials.iter_mut() {
            analytic_paths::set_path_material_hairline(material, device_px);
        }
        return;
    }
    let added: Vec<AssetId<PathExtendedMaterial>> = material_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id } => Some(*id),
            _ => None,
        })
        .collect();
    for id in added {
        if let Some(mut material) = materials.get_mut(id) {
            analytic_paths::set_path_material_hairline(&mut material, device_px);
        }
    }
}

/// Umbrella render plugin — registers the render-side sub-plugins
/// (slug text, SDF panel geometry).
pub(crate) struct RenderPlugin;

pub(crate) fn seed_default_material_cascades(app: &mut App) {
    app.init_asset::<StandardMaterial>();
    let default_material = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(default_panel_material());
    app.insert_resource(CascadeDefault(SdfMaterial(default_material.clone())));
    app.insert_resource(CascadeDefault(TextMaterial(default_material.clone())));
    app.insert_resource(CascadeDefault(ShapeMaterial(default_material)));
}

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        seed_default_material_cascades(app);

        app.add_plugins((
            CascadePlugin::<SdfMaterial>::default(),
            CascadePlugin::<TextMaterial>::default(),
            CascadePlugin::<ShapeMaterial>::default(),
        ));

        app.add_plugins((
            MaterialTablePlugin,
            AnalyticPathPlugin,
            FillBatchPlugin,
            ImageBatchPlugin,
            TextRenderPlugin,
            PanelGeometryPlugin,
            PanelShapePlugin,
        ))
        .init_resource::<AntiAlias>()
        .init_resource::<HairlineWidth>()
        // Bevy registers the OIT type without `ReflectComponent`; this enables
        // reflection-based (BRP) edits of OIT settings on a live camera.
        .register_type_data::<OrderIndependentTransparencySettings, ReflectComponent>()
        // The cascade-root mirrors must land before propagation so a global
        // change reaches every `Resolved<A>` the same frame.
        .add_systems(
            Update,
            (sync_anti_alias, sync_hairline_fade, sync_hairline_width)
                .before(CascadeSet::Propagate),
        )
        .add_observer(transparency::on_stable_transparency_added)
        .add_observer(transparency::on_stable_transparency_removed)
        .add_observer(transparency::on_screen_space_camera_added);
    }
}

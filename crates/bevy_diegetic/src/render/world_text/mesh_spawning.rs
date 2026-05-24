use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::render::storage::ShaderStorageBuffer;

use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::slug_text_spike;
use crate::slug_text_spike::SlugBackend;
use crate::slug_text_spike::SlugPreparedTextRun;
use crate::slug_text_spike::SlugRenderMode;
use crate::slug_text_spike::SlugRunStorageKey;
use crate::slug_text_spike::SlugTextMaterial;
use crate::slug_text_spike::SlugTextMaterialInput;

/// Marker for mesh entities spawned by the world text renderer.
#[derive(Component)]
pub struct WorldTextMesh;

/// Marker for shadow proxy entities spawned by the world text renderer.
#[derive(Component)]
pub struct WorldTextShadowProxy;

/// Despawns existing mesh children (text meshes and shadow proxies) of the
/// given parent entity.
pub(super) fn despawn_mesh_children(
    parent: Entity,
    old_meshes: &Query<(Entity, &ChildOf), Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>>,
    commands: &mut Commands,
) {
    for (mesh_entity, child_of) in old_meshes {
        if child_of.parent() == parent {
            commands.entity(mesh_entity).despawn();
        }
    }
}

/// Configures a `StandardMaterial`'s `double_sided` and `cull_mode` fields
/// from a [`GlyphSidedness`] choice.
const fn apply_sidedness(base: &mut StandardMaterial, sidedness: GlyphSidedness) {
    match sidedness {
        GlyphSidedness::DoubleSided => {
            base.double_sided = true;
            base.cull_mode = None;
        },
        GlyphSidedness::OneSided => {
            base.double_sided = false;
            base.cull_mode = Some(Face::Back);
        },
    }
}

pub(super) struct SlugMeshSpawnAssets<'a, 'w, 's> {
    pub(super) meshes:          &'a mut Assets<Mesh>,
    pub(super) materials:       &'a mut Assets<SlugTextMaterial>,
    pub(super) storage_buffers: &'a mut Assets<ShaderStorageBuffer>,
    pub(super) commands:        &'a mut Commands<'w, 's>,
}

/// Spawns Slug visible mesh and optional shadow proxy entities.
pub(super) fn spawn_slug_world_text_meshes(
    prepared: &SlugPreparedTextRun,
    slug_backend: &mut SlugBackend,
    entity: Entity,
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    assets: &mut SlugMeshSpawnAssets<'_, '_, '_>,
) -> f32 {
    let mesh_start = Instant::now();
    let Ok(storage) =
        slug_backend.ensure_run_storage(prepared, None, assets.meshes, assets.storage_buffers)
    else {
        return 0.0;
    };
    let mesh_handle = storage.mesh;
    let curve_buffer = storage.curves;
    let band_buffer = storage.bands;
    let glyph_buffer = storage.glyphs;

    let is_invisible = style.render_mode() == GlyphRenderMode::Invisible;
    let needs_proxy = if is_invisible {
        style.shadow_mode() != GlyphShadowMode::None
    } else {
        matches!(
            style.shadow_mode(),
            GlyphShadowMode::Text | GlyphShadowMode::PunchOut
        )
    };
    let suppress_shadow =
        is_invisible || needs_proxy || style.shadow_mode() == GlyphShadowMode::None;

    if !is_invisible {
        let material_handle = assets.materials.add(slug_world_text_material(
            style,
            alpha_mode,
            style.render_mode().into(),
            curve_buffer.clone(),
            band_buffer.clone(),
            glyph_buffer.clone(),
        ));
        spawn_slug_visible_mesh(
            entity,
            mesh_handle.clone(),
            prepared.storage_key,
            material_handle,
            suppress_shadow,
            assets.commands,
        );
    }

    if needs_proxy {
        let material_handle = assets.materials.add(slug_world_text_shadow_proxy_material(
            style,
            AlphaMode::Mask(0.5),
            slug_shadow_render_mode(style.shadow_mode()),
            curve_buffer,
            band_buffer,
            glyph_buffer,
        ));
        assets.commands.entity(entity).with_child((
            WorldTextShadowProxy,
            prepared.storage_key,
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            Transform::IDENTITY,
        ));
    }

    mesh_start
        .elapsed()
        .as_secs_f32()
        .mul_add(MILLISECONDS_PER_SECOND, 0.0)
}

fn slug_world_text_material(
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    render_mode: SlugRenderMode,
    curves: Handle<ShaderStorageBuffer>,
    bands: Handle<ShaderStorageBuffer>,
    glyphs: Handle<ShaderStorageBuffer>,
) -> SlugTextMaterial {
    slug_text_spike::slug_text_material(slug_world_text_input(
        style,
        alpha_mode,
        render_mode,
        curves,
        bands,
        glyphs,
    ))
}

fn slug_world_text_shadow_proxy_material(
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    render_mode: SlugRenderMode,
    curves: Handle<ShaderStorageBuffer>,
    bands: Handle<ShaderStorageBuffer>,
    glyphs: Handle<ShaderStorageBuffer>,
) -> SlugTextMaterial {
    slug_text_spike::slug_text_shadow_proxy_material(slug_world_text_input(
        style,
        alpha_mode,
        render_mode,
        curves,
        bands,
        glyphs,
    ))
}

fn slug_world_text_input(
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    render_mode: SlugRenderMode,
    curves: Handle<ShaderStorageBuffer>,
    bands: Handle<ShaderStorageBuffer>,
    glyphs: Handle<ShaderStorageBuffer>,
) -> SlugTextMaterialInput {
    let mut base = StandardMaterial {
        depth_bias: -constants::LAYER_DEPTH_BIAS,
        alpha_mode,
        ..Default::default()
    };
    apply_sidedness(&mut base, style.sidedness());
    SlugTextMaterialInput {
        base,
        fill_color: style.color(),
        render_mode,
        curves,
        bands,
        glyphs,
    }
}

const fn slug_shadow_render_mode(shadow_mode: GlyphShadowMode) -> SlugRenderMode {
    match shadow_mode {
        GlyphShadowMode::SolidQuad => SlugRenderMode::SolidQuad,
        GlyphShadowMode::PunchOut => SlugRenderMode::PunchOut,
        GlyphShadowMode::None | GlyphShadowMode::Text => SlugRenderMode::Text,
    }
}

fn spawn_slug_visible_mesh(
    entity: Entity,
    mesh: Handle<Mesh>,
    storage_key: SlugRunStorageKey,
    material: Handle<SlugTextMaterial>,
    suppress_shadow: bool,
    commands: &mut Commands,
) {
    if suppress_shadow {
        commands.entity(entity).with_child((
            WorldTextMesh,
            storage_key,
            NotShadowCaster,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::IDENTITY,
        ));
    } else {
        commands.entity(entity).with_child((
            WorldTextMesh,
            storage_key,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::IDENTITY,
        ));
    }
}

impl From<GlyphRenderMode> for SlugRenderMode {
    fn from(render_mode: GlyphRenderMode) -> Self {
        match render_mode {
            GlyphRenderMode::Invisible => Self::Invisible,
            GlyphRenderMode::Text => Self::Text,
            GlyphRenderMode::PunchOut => Self::PunchOut,
            GlyphRenderMode::SolidQuad => Self::SolidQuad,
        }
    }
}

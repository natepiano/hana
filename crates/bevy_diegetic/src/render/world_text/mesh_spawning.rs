use std::collections::HashMap;
use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_kana::ToF32;

use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::render::glyph_material;
use crate::render::glyph_material::GlyphMaterial;
use crate::render::glyph_material::GlyphMaterialInput;
use crate::render::glyph_material::GlyphShadowProxyMaterialInput;
use crate::render::glyph_quad;
use crate::render::glyph_quad::GlyphQuadData;
use crate::slug_text_spike::SlugBackend;
use crate::slug_text_spike::SlugPreparedTextRun;
use crate::slug_text_spike::SlugRenderMode;
use crate::slug_text_spike::SlugRunStorageKey;
use crate::slug_text_spike::SlugTextMaterial;
use crate::slug_text_spike::SlugTextMaterialInput;
use crate::slug_text_spike::slug_text_material as make_slug_text_material;
use crate::text::GlyphAtlas;

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

pub(super) struct MeshSpawnAssets<'a, 'w, 's> {
    pub(super) meshes:    &'a mut Assets<Mesh>,
    pub(super) materials: &'a mut Assets<GlyphMaterial>,
    pub(super) commands:  &'a mut Commands<'w, 's>,
}

pub(super) struct SlugMeshSpawnAssets<'a, 'w, 's> {
    pub(super) meshes:          &'a mut Assets<Mesh>,
    pub(super) materials:       &'a mut Assets<SlugTextMaterial>,
    pub(super) storage_buffers: &'a mut Assets<ShaderStorageBuffer>,
    pub(super) commands:        &'a mut Commands<'w, 's>,
}

/// Spawns visible mesh and optional shadow proxy entities for each atlas page
/// of glyph quads under the given `entity`. Returns accumulated mesh build time
/// in milliseconds.
pub(super) fn spawn_world_text_meshes(
    page_quads: &HashMap<u32, Vec<GlyphQuadData>>,
    entity: Entity,
    style: &WorldTextStyle,
    atlas: &GlyphAtlas,
    alpha_mode: AlphaMode,
    assets: &mut MeshSpawnAssets<'_, '_, '_>,
) -> f32 {
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

    let mut mesh_ms = 0.0_f32;
    for (page_index, page_quads) in page_quads {
        let Some(page_image) = atlas.image_handle(*page_index).cloned() else {
            continue;
        };

        let mesh_start = Instant::now();
        let mesh = glyph_quad::build_glyph_mesh(page_quads);
        let mesh_handle = assets.meshes.add(mesh);

        // Spawn visible mesh (skip for Invisible render mode).
        if !is_invisible {
            let mut visible_base = StandardMaterial {
                depth_bias: -constants::LAYER_DEPTH_BIAS,
                ..Default::default()
            };
            apply_sidedness(&mut visible_base, style.sidedness());
            let mat = glyph_material::glyph_material(GlyphMaterialInput {
                base: visible_base,
                sdf_range: GlyphAtlas::sdf_range().to_f32(),
                atlas_dimensions: UVec2::new(atlas.width(), atlas.height()),
                atlas_texture: page_image.clone(),
                hue_offset: 0.0,
                render_mode: u32::from(style.render_mode()),
                distance_field: atlas.distance_field(),
                clip_rect: constants::UNCLIPPED_TEXT_CLIP_RECT,
                oit_depth_offset: 0.0,
                alpha_mode,
            });

            let material_handle = assets.materials.add(mat);

            if suppress_shadow {
                assets.commands.entity(entity).with_child((
                    WorldTextMesh,
                    NotShadowCaster,
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle),
                    Transform::IDENTITY,
                ));
            } else {
                assets.commands.entity(entity).with_child((
                    WorldTextMesh,
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle),
                    Transform::IDENTITY,
                ));
            }
        }

        // Shadow proxy for shaped shadows (or any shadow when Invisible).
        if needs_proxy {
            let shadow_render_mode = match style.shadow_mode() {
                GlyphShadowMode::SolidQuad => u32::from(GlyphRenderMode::SolidQuad),
                GlyphShadowMode::PunchOut => u32::from(GlyphRenderMode::PunchOut),
                GlyphShadowMode::None | GlyphShadowMode::Text => u32::from(GlyphRenderMode::Text),
            };

            let mut proxy_base = StandardMaterial {
                depth_bias: -constants::LAYER_DEPTH_BIAS,
                ..Default::default()
            };
            apply_sidedness(&mut proxy_base, style.sidedness());
            let proxy_material = assets
                .materials
                .add(glyph_material::glyph_shadow_proxy_material(
                    GlyphShadowProxyMaterialInput {
                        base:             proxy_base,
                        sdf_range:        GlyphAtlas::sdf_range().to_f32(),
                        atlas_dimensions: UVec2::new(atlas.width(), atlas.height()),
                        atlas_texture:    page_image,
                        hue_offset:       0.0,
                        render_mode:      shadow_render_mode,
                        distance_field:   atlas.distance_field(),
                        clip_rect:        constants::UNCLIPPED_TEXT_CLIP_RECT,
                        oit_depth_offset: 0.0,
                    },
                ));

            assets.commands.entity(entity).with_child((
                WorldTextShadowProxy,
                Mesh3d(mesh_handle),
                MeshMaterial3d(proxy_material),
                Transform::IDENTITY,
            ));
        }
        mesh_ms = mesh_start
            .elapsed()
            .as_secs_f32()
            .mul_add(MILLISECONDS_PER_SECOND, mesh_ms);
    }
    mesh_ms
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
        let material_handle = assets.materials.add(slug_world_text_material(
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
    let mut base = StandardMaterial {
        depth_bias: -constants::LAYER_DEPTH_BIAS,
        alpha_mode,
        ..Default::default()
    };
    apply_sidedness(&mut base, style.sidedness());
    make_slug_text_material(SlugTextMaterialInput {
        base,
        fill_color: style.color(),
        render_mode,
        curves,
        bands,
        glyphs,
    })
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

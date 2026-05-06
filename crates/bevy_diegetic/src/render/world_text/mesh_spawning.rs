use std::collections::HashMap;
use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy_kana::ToF32;

use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::render::glyph_quad;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::msdf_material;
use crate::render::msdf_material::MsdfShadowProxyMaterialInput;
use crate::render::msdf_material::MsdfTextMaterial;
use crate::render::msdf_material::MsdfTextMaterialInput;
use crate::text::MsdfAtlas;

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
    pub(super) materials: &'a mut Assets<MsdfTextMaterial>,
    pub(super) commands:  &'a mut Commands<'w, 's>,
}

/// Spawns visible mesh and optional shadow proxy entities for each atlas page
/// of glyph quads under the given `entity`. Returns accumulated mesh build time
/// in milliseconds.
pub(super) fn spawn_world_text_meshes(
    page_quads: &HashMap<u32, Vec<GlyphQuadData>>,
    entity: Entity,
    style: &WorldTextStyle,
    atlas: &MsdfAtlas,
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
            let mat = msdf_material::msdf_text_material(MsdfTextMaterialInput {
                base: visible_base,
                sdf_range: MsdfAtlas::sdf_range().to_f32(),
                atlas_dimensions: UVec2::new(atlas.width(), atlas.height()),
                atlas_texture: page_image.clone(),
                hue_offset: 0.0,
                render_mode: u32::from(style.render_mode()),
                clip_rect: constants::UNCLIPPED_TEXT_CLIP_RECT,
                oit_depth_offset: constants::OIT_DEPTH_STEP,
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
                .add(msdf_material::msdf_shadow_proxy_material(
                    MsdfShadowProxyMaterialInput {
                        base:             proxy_base,
                        sdf_range:        MsdfAtlas::sdf_range().to_f32(),
                        atlas_dimensions: UVec2::new(atlas.width(), atlas.height()),
                        atlas_texture:    page_image,
                        hue_offset:       0.0,
                        render_mode:      shadow_render_mode,
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

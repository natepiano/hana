use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::render::storage::ShaderBuffer;

use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::text;
use crate::text::GlyphCache;
use crate::text::PreparedTextRun;
use crate::text::RenderMode;
use crate::text::RunStorageKey;
use crate::text::TextMaterial;
use crate::text::TextMaterialInput;

/// Marker for mesh entities spawned by the world text renderer.
#[derive(Component)]
pub struct WorldTextMesh;

/// Despawns existing text mesh children of the given parent entity.
pub(super) fn despawn_mesh_children(
    parent: Entity,
    old_meshes: &Query<(Entity, &ChildOf), With<WorldTextMesh>>,
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
    pub(super) meshes:          &'a mut Assets<Mesh>,
    pub(super) materials:       &'a mut Assets<TextMaterial>,
    pub(super) storage_buffers: &'a mut Assets<ShaderBuffer>,
    pub(super) commands:        &'a mut Commands<'w, 's>,
}

/// Spawns the visible mesh for a world-text run. The mesh casts its
/// own coverage-silhouette shadow unless the style's shadow mode is
/// [`GlyphShadowMode::None`].
pub(super) fn spawn_world_text_meshes(
    prepared: &PreparedTextRun,
    backend: &mut GlyphCache,
    entity: Entity,
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    assets: &mut MeshSpawnAssets<'_, '_, '_>,
) -> f32 {
    let mesh_start = Instant::now();
    let Ok(storage) =
        backend.ensure_run_storage(prepared, None, assets.meshes, assets.storage_buffers)
    else {
        return 0.0;
    };

    let material_handle = assets.materials.add(world_text_material(
        style,
        alpha_mode,
        style.render_mode().into(),
        storage.curves,
        storage.bands,
        storage.glyphs,
    ));
    spawn_visible_mesh(
        entity,
        storage.mesh,
        prepared.storage_key(),
        material_handle,
        style.shadow_mode(),
        assets.commands,
    );

    mesh_start
        .elapsed()
        .as_secs_f32()
        .mul_add(MILLISECONDS_PER_SECOND, 0.0)
}

fn world_text_material(
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    render_mode: RenderMode,
    curves: Handle<ShaderBuffer>,
    bands: Handle<ShaderBuffer>,
    glyphs: Handle<ShaderBuffer>,
) -> TextMaterial {
    let mut base = StandardMaterial {
        depth_bias: -constants::LAYER_DEPTH_BIAS,
        alpha_mode,
        ..Default::default()
    };
    apply_sidedness(&mut base, style.sidedness());
    text::text_material(TextMaterialInput {
        base,
        fill_color: style.color(),
        render_mode,
        curves,
        bands,
        glyphs,
    })
}

fn spawn_visible_mesh(
    entity: Entity,
    mesh: Handle<Mesh>,
    storage_key: RunStorageKey,
    material: Handle<TextMaterial>,
    shadow_mode: GlyphShadowMode,
    commands: &mut Commands,
) {
    match shadow_mode {
        GlyphShadowMode::None => {
            commands.entity(entity).with_child((
                WorldTextMesh,
                storage_key,
                NotShadowCaster,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::IDENTITY,
            ));
        },
        GlyphShadowMode::Cast => {
            commands.entity(entity).with_child((
                WorldTextMesh,
                storage_key,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::IDENTITY,
            ));
        },
    }
}

impl From<GlyphRenderMode> for RenderMode {
    fn from(render_mode: GlyphRenderMode) -> Self {
        match render_mode {
            GlyphRenderMode::Text => Self::Text,
            GlyphRenderMode::PunchOut => Self::PunchOut,
        }
    }
}

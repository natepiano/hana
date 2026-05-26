use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToF32;

use super::PanelText;
use super::PanelTextLayout;
use crate::cascade::CascadeDefaults;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphShadowMode;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render::constants;
use crate::render::world_text::PanelChild;
use crate::text;
use crate::text::GlyphCache;
use crate::text::RenderMode;
use crate::text::RunStorage;
use crate::text::RunStorageKey;
use crate::text::TextMaterial;
use crate::text::TextMaterialInput;

/// Marker component for text mesh entities spawned by the renderer.
#[derive(Component)]
pub(super) struct DiegeticTextMesh;

/// Builds text meshes for panels whose prepared text runs changed.
pub(super) fn build_panel_text_meshes(
    changed_runs: Query<
        &ChildOf,
        (
            With<PanelChild>,
            Or<(Changed<PanelText>, Changed<Resolved<TextAlpha>>)>,
        ),
    >,
    panel_children: Query<(Entity, &PanelText, &PanelTextLayout, &ChildOf)>,
    old_meshes: Query<(Entity, &ChildOf, Option<&RunStorageKey>), With<DiegeticTextMesh>>,
    panels: Query<(&DiegeticPanel, Option<&RenderLayers>)>,
    resolved_alphas: Query<&Resolved<TextAlpha>, With<PanelChild>>,
    defaults: Res<CascadeDefaults>,
    mut backend: ResMut<GlyphCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let mesh_build_start = Instant::now();
    let dirty_panels = collect_dirty_panels(&changed_runs);

    for panel_entity in dirty_panels {
        let Ok((panel, panel_layers)) = panels.get(panel_entity) else {
            continue;
        };
        for (mesh_entity, child_of, storage_key) in &old_meshes {
            if child_of.parent() == panel_entity {
                if let Some(storage_key) = storage_key {
                    backend.remove_run_storage(*storage_key);
                }
                commands.entity(mesh_entity).despawn();
            }
        }

        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
        let text_base = panel_base_material(panel);

        for (child_entity, panel_run, panel_text_child, child_of) in &panel_children {
            if child_of.parent() != panel_entity {
                continue;
            }
            let resolved_alpha = resolved_alphas
                .get(child_entity)
                .map_or(defaults.text_alpha, |resolved| resolved.0.0);
            spawn_panel_text_run(PanelTextSpawnRequest {
                panel_entity,
                panel_run,
                panel_text_child,
                text_base: &text_base,
                resolved_alpha,
                content_layer: &scene_layer,
                backend: &mut backend,
                meshes: &mut meshes,
                materials: &mut materials,
                storage_buffers: &mut storage_buffers,
                commands: &mut commands,
            });
        }
    }

    perf.panel_text.mesh_build_ms =
        mesh_build_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

fn collect_dirty_panels(
    changed_children: &Query<
        &ChildOf,
        (
            With<PanelChild>,
            Or<(Changed<PanelText>, Changed<Resolved<TextAlpha>>)>,
        ),
    >,
) -> Vec<Entity> {
    let mut dirty_panels = Vec::new();
    for child_of in changed_children {
        let panel_entity = child_of.parent();
        if !dirty_panels.contains(&panel_entity) {
            dirty_panels.push(panel_entity);
        }
    }
    dirty_panels
}

struct PanelTextSpawnRequest<'a, 'w, 's> {
    panel_entity:     Entity,
    panel_run:        &'a PanelText,
    panel_text_child: &'a PanelTextLayout,
    text_base:        &'a StandardMaterial,
    resolved_alpha:   AlphaMode,
    content_layer:    &'a RenderLayers,
    backend:          &'a mut GlyphCache,
    meshes:           &'a mut Assets<Mesh>,
    materials:        &'a mut Assets<TextMaterial>,
    storage_buffers:  &'a mut Assets<ShaderBuffer>,
    commands:         &'a mut Commands<'w, 's>,
}

fn spawn_panel_text_run(request: PanelTextSpawnRequest<'_, '_, '_>) {
    let PanelTextSpawnRequest {
        panel_entity,
        panel_run,
        panel_text_child,
        text_base,
        resolved_alpha,
        content_layer,
        backend,
        meshes,
        materials,
        storage_buffers,
        commands,
    } = request;
    let Ok(storage) = backend.ensure_run_storage(
        &panel_run.prepared,
        panel_run.clip_rect,
        meshes,
        storage_buffers,
    ) else {
        return;
    };

    let command_depth = panel_text_child.command_index.saturating_add(1).to_f32();
    let text_depth_bias = command_depth * constants::LAYER_DEPTH_BIAS;

    let material = materials.add(panel_material(
        text_base,
        text_depth_bias,
        resolved_alpha,
        panel_run.fill_color,
        panel_run.render_mode.into(),
        &storage,
    ));
    spawn_visible_mesh(
        panel_entity,
        storage.mesh,
        panel_run.prepared.storage_key(),
        material,
        panel_run.shadow_mode,
        content_layer,
        commands,
    );
}

fn panel_base_material(panel: &DiegeticPanel) -> StandardMaterial {
    let mut base = panel
        .text_material()
        .cloned()
        .unwrap_or_else(constants::default_panel_material);
    base.alpha_mode = AlphaMode::Blend;
    base.double_sided = true;
    base.cull_mode = None;
    base
}

fn panel_material(
    base: &StandardMaterial,
    depth_bias: f32,
    alpha_mode: AlphaMode,
    fill_color: Color,
    render_mode: RenderMode,
    storage: &RunStorage,
) -> TextMaterial {
    let mut base = base.clone();
    base.depth_bias = depth_bias;
    base.alpha_mode = alpha_mode;
    text::text_material(TextMaterialInput {
        base,
        fill_color,
        render_mode,
        curves: storage.curves.clone(),
        bands: storage.bands.clone(),
        glyphs: storage.glyphs.clone(),
    })
}

fn spawn_visible_mesh(
    panel_entity: Entity,
    mesh_handle: Handle<Mesh>,
    storage_key: RunStorageKey,
    material_handle: Handle<TextMaterial>,
    shadow_mode: GlyphShadowMode,
    content_layer: &RenderLayers,
    commands: &mut Commands,
) {
    match shadow_mode {
        GlyphShadowMode::None => {
            commands.entity(panel_entity).with_child((
                DiegeticTextMesh,
                storage_key,
                NotShadowCaster,
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::IDENTITY,
                content_layer.clone(),
            ));
        },
        GlyphShadowMode::Cast => {
            commands.entity(panel_entity).with_child((
                DiegeticTextMesh,
                storage_key,
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::IDENTITY,
                content_layer.clone(),
            ));
        },
    }
}

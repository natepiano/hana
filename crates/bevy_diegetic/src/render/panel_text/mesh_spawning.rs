use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
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
use crate::panel::RenderMode as PanelRenderMode;
use crate::render::constants;
use crate::render::world_text::PanelChild;
use crate::text;
use crate::text::SlugBackend;
use crate::text::SlugRenderMode;
use crate::text::SlugRunStorage;
use crate::text::SlugRunStorageKey;
use crate::text::SlugTextMaterial;
use crate::text::SlugTextMaterialInput;

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
    old_meshes: Query<(Entity, &ChildOf, Option<&SlugRunStorageKey>), With<DiegeticTextMesh>>,
    panels: Query<(&DiegeticPanel, Option<&RenderLayers>)>,
    resolved_alphas: Query<&Resolved<TextAlpha>, With<PanelChild>>,
    defaults: Res<CascadeDefaults>,
    mut backend: ResMut<SlugBackend>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SlugTextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderStorageBuffer>>,
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
        let is_geometry = panel.render_mode() == PanelRenderMode::Geometry;
        let text_base = panel_base_material(panel, is_geometry);

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
                is_geometry,
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
    is_geometry:      bool,
    content_layer:    &'a RenderLayers,
    backend:          &'a mut SlugBackend,
    meshes:           &'a mut Assets<Mesh>,
    materials:        &'a mut Assets<SlugTextMaterial>,
    storage_buffers:  &'a mut Assets<ShaderStorageBuffer>,
    commands:         &'a mut Commands<'w, 's>,
}

fn spawn_panel_text_run(request: PanelTextSpawnRequest<'_, '_, '_>) {
    let PanelTextSpawnRequest {
        panel_entity,
        panel_run,
        panel_text_child,
        text_base,
        resolved_alpha,
        is_geometry,
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
    let text_depth_bias = if is_geometry {
        command_depth * constants::LAYER_DEPTH_BIAS
    } else {
        0.0
    };

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

fn panel_base_material(panel: &DiegeticPanel, is_geometry: bool) -> StandardMaterial {
    let mut base = panel
        .text_material()
        .cloned()
        .unwrap_or_else(constants::default_panel_material);
    base.alpha_mode = AlphaMode::Blend;
    base.double_sided = true;
    base.cull_mode = None;
    if !is_geometry {
        base.unlit = true;
    }
    base
}

fn panel_material(
    base: &StandardMaterial,
    depth_bias: f32,
    alpha_mode: AlphaMode,
    fill_color: Color,
    render_mode: SlugRenderMode,
    storage: &SlugRunStorage,
) -> SlugTextMaterial {
    let mut base = base.clone();
    base.depth_bias = depth_bias;
    base.alpha_mode = alpha_mode;
    text::slug_text_material(SlugTextMaterialInput {
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
    storage_key: SlugRunStorageKey,
    material_handle: Handle<SlugTextMaterial>,
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

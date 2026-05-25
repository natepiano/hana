use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::math::Vec4;
use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_kana::ToF32;

use crate::cascade::CascadeDefaults;
use crate::cascade::CascadePanelChild;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::BoundingBox;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::panel::HueOffset;
use crate::panel::RenderMode;
use crate::render::constants;
use crate::render::world_text::PanelTextChild;
use crate::text::slug;
use crate::text::slug::SlugBackend;
use crate::text::slug::SlugPreparedTextRun;
use crate::text::slug::SlugRenderMode;
use crate::text::slug::SlugRunStorage;
use crate::text::slug::SlugRunStorageKey;
use crate::text::slug::SlugTextMaterial;
use crate::text::slug::SlugTextMaterialInput;

/// Marker component for text mesh entities spawned by the renderer.
#[derive(Component)]
pub(super) struct DiegeticTextMesh;

/// Marker component for shadow proxy mesh entities.
#[derive(Component)]
pub(super) struct DiegeticShadowProxy;

/// Whether a spawned visible text mesh casts shadows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextMeshShadow {
    Cast,
    Suppress,
}

/// Stores a prepared Slug run for a panel [`WorldText`](crate::WorldText) child.
#[derive(Component)]
pub(super) struct PanelSlugTextRun {
    /// Prepared Slug run.
    pub prepared:    SlugPreparedTextRun,
    /// Glyph render mode for this text element.
    pub render_mode: GlyphRenderMode,
    /// Glyph shadow mode for this text element.
    pub shadow_mode: GlyphShadowMode,
    /// Per-style alpha-mode override.
    pub alpha_mode:  Option<AlphaMode>,
    /// Text fill color.
    pub fill_color:  Color,
    /// Optional panel-local clipping rect.
    pub clip_rect:   Option<[f32; 4]>,
}

/// Cascading attribute for panel-text alpha mode.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(super) struct PanelTextAlpha(pub AlphaMode);

impl CascadePanelChild for PanelTextAlpha {
    type EntityOverride = PanelSlugTextRun;
    type PanelOverride = DiegeticPanel;

    fn entity_value(entity_override: &PanelSlugTextRun) -> Option<Self> {
        entity_override.alpha_mode.map(Self)
    }

    fn panel_value(panel_override: &DiegeticPanel) -> Option<Self> {
        panel_override.text_alpha_mode().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}

pub(super) fn panel_clip_rect_local(
    clip_rect: Option<BoundingBox>,
    scale_x: f32,
    scale_y: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> Vec4 {
    clip_rect.map_or(constants::UNCLIPPED_TEXT_CLIP_RECT, |clip| {
        Vec4::new(
            clip.x.mul_add(scale_x, -anchor_x),
            (clip.y + clip.height).mul_add(-scale_y, anchor_y),
            (clip.x + clip.width).mul_add(scale_x, -anchor_x),
            clip.y.mul_add(-scale_y, anchor_y),
        )
    })
}

/// Builds Slug meshes for panels whose Slug text runs changed.
pub(super) fn build_panel_slug_meshes(
    changed_runs: Query<&ChildOf, (With<PanelTextChild>, Changed<PanelSlugTextRun>)>,
    panel_children: Query<(Entity, &PanelSlugTextRun, &PanelTextChild, &ChildOf)>,
    old_meshes: Query<
        (Entity, &ChildOf, Option<&SlugRunStorageKey>),
        Or<(With<DiegeticTextMesh>, With<DiegeticShadowProxy>)>,
    >,
    panels: Query<(&DiegeticPanel, Option<&HueOffset>, Option<&RenderLayers>)>,
    resolved_alphas: Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: Res<CascadeDefaults>,
    mut slug_backend: ResMut<SlugBackend>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SlugTextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let mesh_build_start = Instant::now();
    let dirty_panels = collect_dirty_panels(&changed_runs);

    for panel_entity in dirty_panels {
        let Ok((panel, _hue_offset, panel_layers)) = panels.get(panel_entity) else {
            continue;
        };
        for (mesh_entity, child_of, storage_key) in &old_meshes {
            if child_of.parent() == panel_entity {
                if let Some(storage_key) = storage_key {
                    slug_backend.remove_run_storage(*storage_key);
                }
                commands.entity(mesh_entity).despawn();
            }
        }

        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
        let is_geometry = panel.render_mode() == RenderMode::Geometry;
        let text_base = slug_panel_base_material(panel, is_geometry);

        for (child_entity, panel_run, panel_text_child, child_of) in &panel_children {
            if child_of.parent() != panel_entity {
                continue;
            }
            let resolved_alpha = resolved_alphas.get(child_entity).map_or_else(
                |_| PanelTextAlpha::global_default(&defaults).0,
                |resolved| resolved.0.0,
            );
            spawn_panel_slug_run(PanelSlugSpawnRequest {
                panel_entity,
                panel_run,
                panel_text_child,
                text_base: &text_base,
                resolved_alpha,
                is_geometry,
                content_layer: &scene_layer,
                scene_layer: &scene_layer,
                slug_backend: &mut slug_backend,
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
    changed_children: &Query<&ChildOf, (With<PanelTextChild>, Changed<PanelSlugTextRun>)>,
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

struct PanelSlugSpawnRequest<'a, 'w, 's> {
    panel_entity:     Entity,
    panel_run:        &'a PanelSlugTextRun,
    panel_text_child: &'a PanelTextChild,
    text_base:        &'a StandardMaterial,
    resolved_alpha:   AlphaMode,
    is_geometry:      bool,
    content_layer:    &'a RenderLayers,
    scene_layer:      &'a RenderLayers,
    slug_backend:     &'a mut SlugBackend,
    meshes:           &'a mut Assets<Mesh>,
    materials:        &'a mut Assets<SlugTextMaterial>,
    storage_buffers:  &'a mut Assets<ShaderStorageBuffer>,
    commands:         &'a mut Commands<'w, 's>,
}

fn spawn_panel_slug_run(request: PanelSlugSpawnRequest<'_, '_, '_>) {
    let PanelSlugSpawnRequest {
        panel_entity,
        panel_run,
        panel_text_child,
        text_base,
        resolved_alpha,
        is_geometry,
        content_layer,
        scene_layer,
        slug_backend,
        meshes,
        materials,
        storage_buffers,
        commands,
    } = request;
    let Ok(storage) = slug_backend.ensure_run_storage(
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
    let is_invisible = panel_run.render_mode == GlyphRenderMode::Invisible;
    let needs_proxy = if is_invisible {
        panel_run.shadow_mode != GlyphShadowMode::None
    } else {
        matches!(
            panel_run.shadow_mode,
            GlyphShadowMode::Text | GlyphShadowMode::PunchOut
        )
    };
    let shadow_mode =
        if is_invisible || needs_proxy || panel_run.shadow_mode == GlyphShadowMode::None {
            TextMeshShadow::Suppress
        } else {
            TextMeshShadow::Cast
        };

    if !is_invisible {
        let material = materials.add(slug_panel_material(
            text_base,
            text_depth_bias,
            resolved_alpha,
            panel_run.fill_color,
            slug_render_mode(panel_run.render_mode),
            &storage,
        ));
        spawn_slug_visible_mesh(
            panel_entity,
            storage.mesh.clone(),
            panel_run.prepared.storage_key,
            material,
            shadow_mode,
            content_layer,
            commands,
        );
    }

    if needs_proxy {
        let proxy_material = materials.add(slug_panel_shadow_proxy_material(
            text_base,
            text_depth_bias - constants::LAYER_DEPTH_BIAS,
            AlphaMode::Mask(0.5),
            panel_run.fill_color,
            slug_shadow_render_mode(panel_run.shadow_mode),
            &storage,
        ));
        commands.entity(panel_entity).with_child((
            DiegeticShadowProxy,
            panel_run.prepared.storage_key,
            Mesh3d(storage.mesh),
            MeshMaterial3d(proxy_material),
            Transform::IDENTITY,
            scene_layer.clone(),
        ));
    }
}

fn slug_panel_base_material(panel: &DiegeticPanel, is_geometry: bool) -> StandardMaterial {
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

fn slug_panel_material(
    base: &StandardMaterial,
    depth_bias: f32,
    alpha_mode: AlphaMode,
    fill_color: Color,
    render_mode: impl Into<SlugRenderMode>,
    storage: &SlugRunStorage,
) -> SlugTextMaterial {
    slug::slug_text_material(slug_panel_input(
        base,
        depth_bias,
        alpha_mode,
        fill_color,
        render_mode,
        storage,
    ))
}

fn slug_panel_shadow_proxy_material(
    base: &StandardMaterial,
    depth_bias: f32,
    alpha_mode: AlphaMode,
    fill_color: Color,
    render_mode: impl Into<SlugRenderMode>,
    storage: &SlugRunStorage,
) -> SlugTextMaterial {
    slug::slug_text_shadow_proxy_material(slug_panel_input(
        base,
        depth_bias,
        alpha_mode,
        fill_color,
        render_mode,
        storage,
    ))
}

fn slug_panel_input(
    base: &StandardMaterial,
    depth_bias: f32,
    alpha_mode: AlphaMode,
    fill_color: Color,
    render_mode: impl Into<SlugRenderMode>,
    storage: &SlugRunStorage,
) -> SlugTextMaterialInput {
    let mut base = base.clone();
    base.depth_bias = depth_bias;
    base.alpha_mode = alpha_mode;
    SlugTextMaterialInput {
        base,
        fill_color,
        render_mode: render_mode.into(),
        curves: storage.curves.clone(),
        bands: storage.bands.clone(),
        glyphs: storage.glyphs.clone(),
    }
}

fn spawn_slug_visible_mesh(
    panel_entity: Entity,
    mesh_handle: Handle<Mesh>,
    storage_key: SlugRunStorageKey,
    material_handle: Handle<SlugTextMaterial>,
    shadow_mode: TextMeshShadow,
    content_layer: &RenderLayers,
    commands: &mut Commands,
) {
    match shadow_mode {
        TextMeshShadow::Suppress => {
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
        TextMeshShadow::Cast => {
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

const fn slug_shadow_render_mode(shadow_mode: GlyphShadowMode) -> SlugRenderMode {
    match shadow_mode {
        GlyphShadowMode::SolidQuad => SlugRenderMode::SolidQuad,
        GlyphShadowMode::PunchOut => SlugRenderMode::PunchOut,
        GlyphShadowMode::None | GlyphShadowMode::Text => SlugRenderMode::Text,
    }
}

const fn slug_render_mode(render_mode: GlyphRenderMode) -> SlugRenderMode {
    match render_mode {
        GlyphRenderMode::Invisible => SlugRenderMode::Invisible,
        GlyphRenderMode::Text => SlugRenderMode::Text,
        GlyphRenderMode::PunchOut => SlugRenderMode::PunchOut,
        GlyphRenderMode::SolidQuad => SlugRenderMode::SolidQuad,
    }
}

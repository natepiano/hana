use std::collections::HashMap;
use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::math::Vec4;
use bevy::prelude::*;
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
use crate::render::glyph_quad;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::msdf_material;
use crate::render::msdf_material::MsdfShadowProxyMaterialInput;
use crate::render::msdf_material::MsdfTextMaterial;
use crate::render::msdf_material::MsdfTextMaterialInput;
use crate::render::panel_rtt::PanelRttRegistry;
use crate::render::world_text::PanelTextChild;
use crate::text::MsdfAtlas;

/// Marker component for text mesh entities spawned by the renderer.
#[derive(Component)]
pub(super) struct DiegeticTextMesh;

/// Marker component for shadow proxy mesh entities.
#[derive(Component)]
pub(super) struct DiegeticShadowProxy;

/// Key for grouping text quads that share the same material configuration.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TextBatchKey {
    render_mode:     GlyphRenderMode,
    shadow_mode:     GlyphShadowMode,
    page_index:      u32,
    clip_rect:       [u32; 4],
    alpha_mode_bits: u64,
}

/// Key for shared zero-hue MSDF text materials.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SharedMsdfMaterialKey {
    page_index:      u32,
    clip_rect:       [u32; 4],
    depth_bias_bits: u32,
    alpha_mode_bits: u64,
}

/// Cached default material handles shared across panels without a [`HueOffset`].
#[derive(Resource, Default)]
pub(super) struct SharedMsdfMaterials {
    handles: HashMap<SharedMsdfMaterialKey, Handle<MsdfTextMaterial>>,
}

impl SharedMsdfMaterials {
    pub(super) fn clear(&mut self) { self.handles.clear(); }
}

/// Stores shaped glyph quads for a panel [`WorldText`] child, along with its
/// render and shadow modes for batching into combined meshes.
#[derive(Component)]
pub(super) struct PanelTextQuads {
    /// Per-glyph quads keyed by atlas page index.
    pub quads:       Vec<(u32, GlyphQuadData)>,
    /// The glyph render mode for this text element.
    pub render_mode: GlyphRenderMode,
    /// The glyph shadow mode for this text element.
    pub shadow_mode: GlyphShadowMode,
    /// Per-style alpha-mode override (from `LayoutTextStyle`). `None` means
    /// the entity inherits from its parent panel, which in turn inherits from
    /// [`CascadeDefaults::text_alpha`].
    pub alpha_mode:  Option<AlphaMode>,
}

/// Cascading attribute for panel-text alpha mode.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(super) struct PanelTextAlpha(pub AlphaMode);

impl CascadePanelChild for PanelTextAlpha {
    type EntityOverride = PanelTextQuads;
    type PanelOverride = DiegeticPanel;

    fn entity_value(entity_override: &PanelTextQuads) -> Option<Self> {
        entity_override.alpha_mode.map(Self)
    }

    fn panel_value(panel_override: &DiegeticPanel) -> Option<Self> {
        panel_override.text_alpha_mode().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}

fn alpha_mode_bits(mode: AlphaMode) -> u64 {
    match mode {
        AlphaMode::Opaque => 1,
        AlphaMode::Mask(threshold) => 2 | (u64::from(threshold.to_bits()) << 8),
        AlphaMode::Blend => 3,
        AlphaMode::Premultiplied => 4,
        AlphaMode::AlphaToCoverage => 5,
        AlphaMode::Add => 6,
        AlphaMode::Multiply => 7,
    }
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

fn clip_rect_bits(clip_rect: Vec4) -> [u32; 4] {
    [
        clip_rect.x.to_bits(),
        clip_rect.y.to_bits(),
        clip_rect.z.to_bits(),
        clip_rect.w.to_bits(),
    ]
}

const fn clip_rect_from_bits(bits: [u32; 4]) -> Vec4 {
    Vec4::new(
        f32::from_bits(bits[0]),
        f32::from_bits(bits[1]),
        f32::from_bits(bits[2]),
        f32::from_bits(bits[3]),
    )
}

const fn glyph_render_mode_uniform(render_mode: GlyphRenderMode) -> u32 {
    match render_mode {
        GlyphRenderMode::Invisible => 0,
        GlyphRenderMode::Text => 1,
        GlyphRenderMode::PunchOut => 2,
        GlyphRenderMode::SolidQuad => 3,
    }
}

/// Builds batched meshes for panels whose [`PanelTextQuads`] changed.
pub(super) fn build_panel_batched_meshes(
    changed_quads: Query<&ChildOf, (With<PanelTextChild>, Changed<PanelTextQuads>)>,
    panel_children: Query<(Entity, &PanelTextQuads, &PanelTextChild, &ChildOf)>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<DiegeticTextMesh>, With<DiegeticShadowProxy>)>>,
    panels: Query<(&DiegeticPanel, Option<&HueOffset>, Option<&RenderLayers>)>,
    resolved_alphas: Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: Res<CascadeDefaults>,
    atlas: Res<MsdfAtlas>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut shared_mats: ResMut<SharedMsdfMaterials>,
    rtt_registry: Res<PanelRttRegistry>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let mesh_build_start = Instant::now();

    let mut dirty_panels: Vec<Entity> = Vec::new();
    for child_of in &changed_quads {
        let panel_entity = child_of.parent();
        if !dirty_panels.contains(&panel_entity) {
            dirty_panels.push(panel_entity);
        }
    }

    if !dirty_panels.is_empty() {
        shared_mats.handles.clear();
    }

    for panel_entity in dirty_panels {
        let Ok((panel, hue_offset, panel_layers)) = panels.get(panel_entity) else {
            continue;
        };
        let hue = hue_offset.map_or(0.0, |panel_hue_offset| panel_hue_offset.0);
        let is_geometry = panel.render_mode() == RenderMode::Geometry;
        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));

        let (batches, max_command_index) =
            collect_panel_batches(panel_entity, &panel_children, &resolved_alphas, &defaults);

        if batches.values().all(|(_, quads)| quads.is_empty()) {
            continue;
        }

        for (mesh_entity, child_of) in &old_meshes {
            if child_of.parent() == panel_entity {
                commands.entity(mesh_entity).despawn();
            }
        }

        let content_layer = rtt_registry
            .get_layer(panel_entity)
            .map_or_else(|| scene_layer.clone(), RenderLayers::layer);

        let mut text_base = panel
            .text_material()
            .cloned()
            .unwrap_or_else(constants::default_panel_material);
        text_base.alpha_mode = AlphaMode::Blend;
        text_base.double_sided = true;
        text_base.cull_mode = None;
        if !is_geometry {
            text_base.unlit = true;
        }

        let text_depth_bias = if is_geometry {
            max_command_index.saturating_add(1).to_f32() * constants::LAYER_DEPTH_BIAS
        } else {
            0.0
        };
        let text_oit_offset = if is_geometry {
            max_command_index.saturating_add(1).to_f32() * constants::OIT_DEPTH_STEP
        } else {
            0.0
        };

        let mut spawn_context = BatchSpawnContext {
            panel_entity,
            hue,
            atlas: &atlas,
            meshes: &mut meshes,
            materials: &mut materials,
            shared_mats: &mut shared_mats,
            content_layer,
            scene_layer,
            text_base,
            text_depth_bias,
            text_oit_offset,
            commands: &mut commands,
        };
        spawn_batch_meshes(&batches, &mut spawn_context);
    }

    perf.panel_text.mesh_build_ms =
        mesh_build_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

fn collect_panel_batches(
    panel_entity: Entity,
    panel_children: &Query<(Entity, &PanelTextQuads, &PanelTextChild, &ChildOf)>,
    resolved_alphas: &Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: &CascadeDefaults,
) -> (
    HashMap<TextBatchKey, (AlphaMode, Vec<GlyphQuadData>)>,
    usize,
) {
    let mut batches: HashMap<TextBatchKey, (AlphaMode, Vec<GlyphQuadData>)> = HashMap::new();
    let mut max_command_index = 0_usize;
    for (child_entity, panel_text_quads, panel_text_child, child_of) in panel_children {
        if child_of.parent() != panel_entity {
            continue;
        }
        max_command_index = max_command_index.max(panel_text_child.command_index);
        let clip_rect = panel_clip_rect_local(
            panel_text_child.clip_rect,
            panel_text_child.scale_x,
            panel_text_child.scale_y,
            panel_text_child.anchor_x,
            panel_text_child.anchor_y,
        );
        let resolved_alpha = resolved_alphas.get(child_entity).map_or_else(
            |_| PanelTextAlpha::global_default(defaults).0,
            |resolved| resolved.0.0,
        );
        let key_clip_rect = clip_rect_bits(clip_rect);
        let alpha_bits = alpha_mode_bits(resolved_alpha);
        for (page_index, quad) in &panel_text_quads.quads {
            let key = TextBatchKey {
                render_mode:     panel_text_quads.render_mode,
                shadow_mode:     panel_text_quads.shadow_mode,
                page_index:      *page_index,
                clip_rect:       key_clip_rect,
                alpha_mode_bits: alpha_bits,
            };
            batches
                .entry(key)
                .or_insert_with(|| (resolved_alpha, Vec::new()))
                .1
                .push(*quad);
        }
    }
    (batches, max_command_index)
}

struct BatchSpawnContext<'a, 'w, 's> {
    panel_entity:    Entity,
    hue:             f32,
    atlas:           &'a MsdfAtlas,
    meshes:          &'a mut Assets<Mesh>,
    materials:       &'a mut Assets<MsdfTextMaterial>,
    shared_mats:     &'a mut SharedMsdfMaterials,
    content_layer:   RenderLayers,
    scene_layer:     RenderLayers,
    text_base:       StandardMaterial,
    text_depth_bias: f32,
    text_oit_offset: f32,
    commands:        &'a mut Commands<'w, 's>,
}

fn spawn_batch_meshes(
    batches: &HashMap<TextBatchKey, (AlphaMode, Vec<GlyphQuadData>)>,
    context: &mut BatchSpawnContext<'_, '_, '_>,
) {
    for (key, (alpha_mode, quads)) in batches {
        if quads.is_empty() {
            continue;
        }

        let Some(page_image) = context.atlas.image_handle(key.page_index).cloned() else {
            continue;
        };

        let mesh_handle = context.meshes.add(glyph_quad::build_glyph_mesh(quads));
        let is_invisible = key.render_mode == GlyphRenderMode::Invisible;
        let needs_proxy = if is_invisible {
            key.shadow_mode != GlyphShadowMode::None
        } else {
            matches!(
                key.shadow_mode,
                GlyphShadowMode::Text | GlyphShadowMode::PunchOut
            )
        };
        let suppress_shadow =
            is_invisible || needs_proxy || key.shadow_mode == GlyphShadowMode::None;

        if !is_invisible {
            let mut material_context = VisibleMaterialContext {
                hue:             context.hue,
                atlas:           context.atlas,
                materials:       context.materials,
                shared_mats:     context.shared_mats,
                batch_base:      {
                    let mut batch_base = context.text_base.clone();
                    batch_base.depth_bias = context.text_depth_bias;
                    batch_base
                },
                text_oit_offset: context.text_oit_offset,
            };
            let material_handle =
                resolve_visible_material(key, *alpha_mode, &page_image, &mut material_context);
            spawn_visible_mesh(
                context.panel_entity,
                mesh_handle.clone(),
                material_handle,
                suppress_shadow,
                &context.content_layer,
                context.commands,
            );
        }

        if needs_proxy {
            let mut shadow_context = ShadowProxyContext {
                panel_entity:    context.panel_entity,
                hue:             context.hue,
                atlas:           context.atlas,
                text_base:       context.text_base.clone(),
                text_depth_bias: context.text_depth_bias,
                text_oit_offset: context.text_oit_offset,
                scene_layer:     context.scene_layer.clone(),
                materials:       context.materials,
                commands:        context.commands,
            };
            spawn_shadow_proxy(key, mesh_handle, page_image, &mut shadow_context);
        }
    }
}

struct VisibleMaterialContext<'a> {
    hue:             f32,
    atlas:           &'a MsdfAtlas,
    materials:       &'a mut Assets<MsdfTextMaterial>,
    shared_mats:     &'a mut SharedMsdfMaterials,
    batch_base:      StandardMaterial,
    text_oit_offset: f32,
}

fn resolve_visible_material(
    key: &TextBatchKey,
    alpha_mode: AlphaMode,
    page_image: &Handle<Image>,
    context: &mut VisibleMaterialContext<'_>,
) -> Handle<MsdfTextMaterial> {
    let clip_rect = clip_rect_from_bits(key.clip_rect);
    if context.hue.abs() < f32::EPSILON && key.render_mode == GlyphRenderMode::Text {
        context
            .shared_mats
            .handles
            .entry(SharedMsdfMaterialKey {
                page_index:      key.page_index,
                clip_rect:       key.clip_rect,
                depth_bias_bits: context.batch_base.depth_bias.to_bits(),
                alpha_mode_bits: key.alpha_mode_bits,
            })
            .or_insert_with(|| {
                context
                    .materials
                    .add(msdf_material::msdf_text_material(MsdfTextMaterialInput {
                        base: context.batch_base.clone(),
                        sdf_range: MsdfAtlas::sdf_range().to_f32(),
                        atlas_dimensions: UVec2::new(context.atlas.width(), context.atlas.height()),
                        atlas_texture: page_image.clone(),
                        hue_offset: 0.0,
                        render_mode: glyph_render_mode_uniform(GlyphRenderMode::Text),
                        clip_rect,
                        oit_depth_offset: context.text_oit_offset,
                        alpha_mode,
                    }))
            })
            .clone()
    } else {
        context
            .materials
            .add(msdf_material::msdf_text_material(MsdfTextMaterialInput {
                base: context.batch_base.clone(),
                sdf_range: MsdfAtlas::sdf_range().to_f32(),
                atlas_dimensions: UVec2::new(context.atlas.width(), context.atlas.height()),
                atlas_texture: page_image.clone(),
                hue_offset: context.hue,
                render_mode: glyph_render_mode_uniform(key.render_mode),
                clip_rect,
                oit_depth_offset: context.text_oit_offset,
                alpha_mode,
            }))
    }
}

fn spawn_visible_mesh(
    panel_entity: Entity,
    mesh_handle: Handle<Mesh>,
    material_handle: Handle<MsdfTextMaterial>,
    suppress_shadow: bool,
    content_layer: &RenderLayers,
    commands: &mut Commands,
) {
    let transform = Transform::from_xyz(0.0, 0.0, 0.0);
    if suppress_shadow {
        commands.entity(panel_entity).with_child((
            DiegeticTextMesh,
            NotShadowCaster,
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            transform,
            content_layer.clone(),
        ));
    } else {
        commands.entity(panel_entity).with_child((
            DiegeticTextMesh,
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            transform,
            content_layer.clone(),
        ));
    }
}

struct ShadowProxyContext<'a, 'w, 's> {
    panel_entity:    Entity,
    hue:             f32,
    atlas:           &'a MsdfAtlas,
    text_base:       StandardMaterial,
    text_depth_bias: f32,
    text_oit_offset: f32,
    scene_layer:     RenderLayers,
    materials:       &'a mut Assets<MsdfTextMaterial>,
    commands:        &'a mut Commands<'w, 's>,
}

fn spawn_shadow_proxy(
    key: &TextBatchKey,
    mesh_handle: Handle<Mesh>,
    page_image: Handle<Image>,
    context: &mut ShadowProxyContext<'_, '_, '_>,
) {
    let clip_rect = clip_rect_from_bits(key.clip_rect);
    let shadow_render_mode = match key.shadow_mode {
        GlyphShadowMode::SolidQuad => glyph_render_mode_uniform(GlyphRenderMode::SolidQuad),
        GlyphShadowMode::PunchOut => glyph_render_mode_uniform(GlyphRenderMode::PunchOut),
        GlyphShadowMode::None | GlyphShadowMode::Text => {
            glyph_render_mode_uniform(GlyphRenderMode::Text)
        },
    };

    let mut proxy_base = context.text_base.clone();
    proxy_base.depth_bias = context.text_depth_bias - constants::LAYER_DEPTH_BIAS;
    let proxy_material = context
        .materials
        .add(msdf_material::msdf_shadow_proxy_material(
            MsdfShadowProxyMaterialInput {
                base: proxy_base,
                sdf_range: MsdfAtlas::sdf_range().to_f32(),
                atlas_dimensions: UVec2::new(context.atlas.width(), context.atlas.height()),
                atlas_texture: page_image,
                hue_offset: context.hue,
                render_mode: shadow_render_mode,
                clip_rect,
                oit_depth_offset: context.text_oit_offset,
            },
        ));

    context.commands.entity(context.panel_entity).with_child((
        DiegeticShadowProxy,
        Mesh3d(mesh_handle),
        MeshMaterial3d(proxy_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
        context.scene_layer.clone(),
    ));
}

/// Syncs [`HueOffset`] to text materials on child meshes.
pub(super) fn sync_panel_hue_offset(
    panels: Query<(Entity, &HueOffset), Changed<HueOffset>>,
    mut children: Query<(&ChildOf, &mut MeshMaterial3d<MsdfTextMaterial>)>,
    shared_mats: Res<SharedMsdfMaterials>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
) {
    for (panel_entity, hue_offset) in &panels {
        for (child_of, mut material_handle) in &mut children {
            if child_of.parent() != panel_entity {
                continue;
            }

            let is_shared = shared_mats
                .handles
                .values()
                .any(|handle| *handle == material_handle.0);

            if is_shared {
                if let Some(base) = materials.get(&material_handle.0) {
                    let mut private = base.clone();
                    private.extension.uniforms.hue_offset = hue_offset.0;
                    material_handle.0 = materials.add(private);
                }
            } else if let Some(material) = materials.get_mut(&material_handle.0) {
                material.extension.uniforms.hue_offset = hue_offset.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;
    use crate::render::msdf_material;
    use crate::render::msdf_material::MsdfTextMaterialInput;

    #[test]
    fn material_sharing_by_hue_offset() {
        let mut materials = Assets::<MsdfTextMaterial>::default();
        let mut images = Assets::<Image>::default();
        let atlas_image = images.add(Image::default());
        let base = StandardMaterial::default();
        let shared_handle =
            materials.add(msdf_material::msdf_text_material(MsdfTextMaterialInput {
                base:             base.clone(),
                sdf_range:        4.0,
                atlas_dimensions: UVec2::new(256, 256),
                atlas_texture:    atlas_image.clone(),
                hue_offset:       0.0,
                render_mode:      0,
                clip_rect:        constants::UNCLIPPED_TEXT_CLIP_RECT,
                oit_depth_offset: 0.0,
                alpha_mode:       AlphaMode::AlphaToCoverage,
            }));

        let mut decide = |hue: f32| -> Handle<MsdfTextMaterial> {
            if hue.abs() < f32::EPSILON {
                shared_handle.clone()
            } else {
                materials.add(msdf_material::msdf_text_material(MsdfTextMaterialInput {
                    base:             base.clone(),
                    sdf_range:        4.0,
                    atlas_dimensions: UVec2::new(256, 256),
                    atlas_texture:    atlas_image.clone(),
                    hue_offset:       hue,
                    render_mode:      0,
                    clip_rect:        constants::UNCLIPPED_TEXT_CLIP_RECT,
                    oit_depth_offset: 0.0,
                    alpha_mode:       AlphaMode::AlphaToCoverage,
                }))
            }
        };

        let a = decide(0.0);
        let b = decide(0.0);
        assert_eq!(a.id(), b.id());

        let c = decide(0.5);
        assert_ne!(a.id(), c.id());

        let d = decide(0.5);
        assert_ne!(c.id(), d.id());
    }
}

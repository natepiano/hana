//! Panel geometry rendering — SDF rounded rectangles from layout render commands.
//!
//! Each element with a background or border produces one SDF quad entity.
//! The SDF fragment shader renders both fill and border in a single pass
//! with pixel-perfect anti-aliased edges and per-corner radii.

use std::collections::HashMap;

use bevy::asset::load_internal_asset;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::picking::mesh_picking::ray_cast::RayCastBackfaces;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::clip;
use super::constants;
use super::constants::SDF_STROKE_SHADER_HANDLE;
use super::panel_rtt;
use super::panel_rtt::PanelRttRegistry;
use super::sdf_material;
use super::sdf_material::SdfPanelMaterial;
use super::sdf_material::SdfPanelMaterialInput;
use crate::layout::BoundingBox;
use crate::layout::RectangleSource;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::RenderMode;
use crate::panel::SurfaceShadow;

/// Marker for SDF panel mesh entities.
#[derive(Component)]
struct PanelSdfMesh;

/// Marker for the invisible full-panel interaction mesh (Geometry mode only).
#[derive(Component)]
struct PanelInteractionMesh;

/// Whether the panel renders as 3D geometry (lit by PBR) or as an unlit texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderStyle {
    Geometry,
    Texture,
}

/// Whether shadow casting is enabled or suppressed for panel meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShadowMode {
    Enabled,
    Suppressed,
}

/// Plugin that adds panel geometry rendering (backgrounds and borders).
pub(super) struct PanelGeometryPlugin;

impl Plugin for PanelGeometryPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SDF_STROKE_SHADER_HANDLE,
            "sdf_stroke.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<SdfPanelMaterial>::default());
        app.add_systems(
            PostUpdate,
            build_panel_geometry.after(panel_rtt::setup_panel_rtt),
        );
    }
}

/// Gathered fill + border data for a single element.
struct ElementSurface {
    /// Element index in the layout tree.
    element_idx:   usize,
    /// Bounding box from the render command.
    bounds:        BoundingBox,
    /// Fill color (from Rectangle command), if any.
    fill_color:    Option<Color>,
    /// Border widths [top, right, bottom, left] in layout points.
    border_widths: [f32; 4],
    /// Border color.
    border_color:  Option<Color>,
    /// Index of the first render command for this element (for layer ordering).
    command_index: usize,
    /// Active clip rect in layout coordinates when this surface was
    /// gathered. `None` means unclipped.
    clip_rect:     Option<BoundingBox>,
}

/// Extracts render commands, gathers fill + border per element, and
/// spawns one SDF quad per element.
fn build_panel_geometry(
    changed_panels: Query<
        (
            Entity,
            &DiegeticPanel,
            &ComputedDiegeticPanel,
            Option<&RenderLayers>,
        ),
        Changed<ComputedDiegeticPanel>,
    >,
    old_sdf: Query<(Entity, &ChildOf), With<PanelSdfMesh>>,
    old_interaction: Query<(Entity, &ChildOf), With<PanelInteractionMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut sdf_materials: ResMut<Assets<SdfPanelMaterial>>,
    rtt_registry: Res<PanelRttRegistry>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed, panel_layers) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        let (anchor_x, anchor_y) = panel.anchor_offsets();
        let render_style = if panel.render_mode() == RenderMode::Geometry {
            RenderStyle::Geometry
        } else {
            RenderStyle::Texture
        };
        let shadow_mode = if panel.surface_shadow() == SurfaceShadow::Off {
            ShadowMode::Suppressed
        } else {
            ShadowMode::Enabled
        };
        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
        let layer = rtt_registry
            .get_layer(panel_entity)
            .map_or(scene_layer, RenderLayers::layer);

        // ── Despawn old geometry ────────────────────────────────────
        despawn_children_of(&old_sdf, panel_entity, &mut commands);
        despawn_children_of(&old_interaction, panel_entity, &mut commands);

        // ── Gather fill + border per element ────────────────────────
        let gathered = gather_surfaces(&result.commands);

        // ── Spawn SDF quads ─────────────────────────────────────────
        {
            let mut spawn_context = SdfElementSpawnContext {
                panel,
                render_style,
                shadow_mode,
                points_to_world,
                anchor_x,
                anchor_y,
                layer: &layer,
                meshes: &mut meshes,
                sdf_materials: &mut sdf_materials,
                commands: &mut commands,
                panel_entity,
            };
            for surface in gathered.surfaces.values() {
                spawn_sdf_element(surface, &mut spawn_context);
            }

            // ── Spawn between-children dividers as SDF elements ─────────
            for &(cmd_index, bounds, color, clip) in &gathered.dividers {
                let divider_surface = ElementSurface {
                    element_idx: usize::MAX,
                    bounds,
                    fill_color: Some(color),
                    border_widths: [0.0; 4],
                    border_color: None,
                    command_index: cmd_index,
                    clip_rect: clip,
                };
                spawn_sdf_element(&divider_surface, &mut spawn_context);
            }
        }

        // ── Interaction mesh (Geometry mode only) ───────────────────
        if render_style == RenderStyle::Geometry {
            let world_w = panel.world_width();
            let world_h = panel.world_height();
            let center_x = world_w.mul_add(0.5, -anchor_x);
            let center_y = world_h.mul_add(-0.5, anchor_y);

            let interact_mat = standard_materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                depth_bias: -constants::LAYER_DEPTH_BIAS,
                ..default()
            });
            commands.entity(panel_entity).with_child((
                PanelInteractionMesh,
                RayCastBackfaces,
                NotShadowCaster,
                Mesh3d(meshes.add(Rectangle::new(world_w, world_h))),
                MeshMaterial3d(interact_mat),
                Transform::from_xyz(center_x, center_y, 0.0),
                layer,
            ));
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

struct GatheredCommands {
    surfaces: HashMap<usize, ElementSurface>,
    dividers: Vec<(usize, BoundingBox, Color, Option<BoundingBox>)>,
}

/// Gathers fill + border data per element from render commands.
///
/// Computes the active clip rect for each command via
/// [`clip::compute_clip_rects`] and stores it on each surface and divider.
fn gather_surfaces(commands: &[RenderCommand]) -> GatheredCommands {
    let clip_rects = clip::compute_clip_rects(commands);
    let mut surfaces: HashMap<usize, ElementSurface> = HashMap::new();
    let mut dividers: Vec<(usize, BoundingBox, Color, Option<BoundingBox>)> = Vec::new();

    for (cmd_index, cmd) in commands.iter().enumerate() {
        let clip = clip_rects[cmd_index];
        match &cmd.kind {
            RenderCommandKind::Rectangle { color, source } => {
                // Skip surfaces entirely outside their clip rect.
                if clip.is_some_and(|c| cmd.bounds.intersect(&c).is_none()) {
                    continue;
                }
                if *source == RectangleSource::BetweenChildrenBorder {
                    dividers.push((cmd_index, cmd.bounds, *color, clip));
                } else {
                    surfaces
                        .entry(cmd.element_idx)
                        .or_insert_with(|| ElementSurface {
                            element_idx:   cmd.element_idx,
                            bounds:        cmd.bounds,
                            fill_color:    None,
                            border_widths: [0.0; 4],
                            border_color:  None,
                            command_index: cmd_index,
                            clip_rect:     clip,
                        })
                        .fill_color = Some(*color);
                }
            },
            RenderCommandKind::Border { border } => {
                let surface = surfaces
                    .entry(cmd.element_idx)
                    .or_insert_with(|| ElementSurface {
                        element_idx:   cmd.element_idx,
                        bounds:        cmd.bounds,
                        fill_color:    None,
                        border_widths: [0.0; 4],
                        border_color:  None,
                        command_index: cmd_index,
                        clip_rect:     clip,
                    });
                surface.border_widths = [
                    border.top.value,
                    border.right.value,
                    border.bottom.value,
                    border.left.value,
                ];
                surface.border_color = Some(border.color);
            },
            _ => {},
        }
    }

    GatheredCommands { surfaces, dividers }
}

struct SdfElementSpawnContext<'a, 'w, 's> {
    panel:           &'a DiegeticPanel,
    render_style:    RenderStyle,
    shadow_mode:     ShadowMode,
    points_to_world: f32,
    anchor_x:        f32,
    anchor_y:        f32,
    layer:           &'a RenderLayers,
    meshes:          &'a mut Assets<Mesh>,
    sdf_materials:   &'a mut Assets<SdfPanelMaterial>,
    commands:        &'a mut Commands<'w, 's>,
    panel_entity:    Entity,
}

fn spawn_sdf_element(surface: &ElementSurface, context: &mut SdfElementSpawnContext<'_, '_, '_>) {
    let element_mat = context.panel.tree().element_material(surface.element_idx);
    let corner_radius = context
        .panel
        .tree()
        .element_corner_radius(surface.element_idx);

    // Fill color from .background() or element .material() — never panel material.
    let effective_color = surface.fill_color.or_else(|| {
        if element_mat.is_some() {
            None
        } else {
            Some(Color::NONE)
        }
    });
    let mut base =
        constants::resolve_material(element_mat, context.panel.material(), effective_color);
    if context.render_style == RenderStyle::Texture {
        base.unlit = true;
    } else {
        base.depth_bias = surface.command_index.to_f32() * constants::LAYER_DEPTH_BIAS;
    }

    let world_w = surface.bounds.width * context.points_to_world;
    let world_h = surface.bounds.height * context.points_to_world;
    let world_radii = corner_radius
        .to_array()
        .map(|radius| radius * context.points_to_world);
    let world_borders = surface
        .border_widths
        .map(|width| width * context.points_to_world);

    let half_w = world_w * 0.5;
    let half_h = world_h * 0.5;

    // Mesh is slightly larger than the SDF shape so the exterior
    // anti-aliasing ramp has fragments to render on.
    let pad = constants::SDF_AA_PADDING;
    let mesh_half_w = half_w + pad;
    let mesh_half_h = half_h + pad;

    // Convert layout-space clip rect to local quad coords (centered, Y-up).
    let clip_rect = surface.clip_rect.map_or_else(
        || bevy::math::Vec4::new(-mesh_half_w, -mesh_half_h, mesh_half_w, mesh_half_h),
        |clip_rect| {
            let (cx, cy) = surface.bounds.center();
            let left = (clip_rect.x - cx) * context.points_to_world;
            let right = (clip_rect.x + clip_rect.width - cx) * context.points_to_world;
            // Layout Y-down → local Y-up.
            let top = -(clip_rect.y - cy) * context.points_to_world;
            let bottom = -(clip_rect.y + clip_rect.height - cy) * context.points_to_world;
            bevy::math::Vec4::new(left, bottom.min(top), right, bottom.max(top))
        },
    );

    let sdf_mat = sdf_material::sdf_panel_material(
        base,
        SdfPanelMaterialInput {
            half_size: Vec2::new(half_w, half_h),
            mesh_half_size: Vec2::new(mesh_half_w, mesh_half_h),
            corner_radii: world_radii,
            border_widths: world_borders,
            border_color: surface.border_color,
            clip_rect,
            oit_depth_offset: surface.command_index.to_f32() * constants::OIT_DEPTH_STEP,
        },
    );

    let world_rect = bounds_to_world_rect(
        &surface.bounds,
        context.points_to_world,
        context.anchor_x,
        context.anchor_y,
    );

    let mesh = context
        .meshes
        .add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0));
    let mat_handle = context.sdf_materials.add(sdf_mat);
    let base_components = (
        PanelSdfMesh,
        Mesh3d(mesh),
        MeshMaterial3d(mat_handle),
        Transform::from_xyz(world_rect.center_x, world_rect.center_y, 0.0),
        context.layer.clone(),
    );
    match context.shadow_mode {
        ShadowMode::Suppressed => {
            context
                .commands
                .entity(context.panel_entity)
                .with_child((base_components, NotShadowCaster));
        },
        ShadowMode::Enabled => {
            context
                .commands
                .entity(context.panel_entity)
                .with_child(base_components);
        },
    }
}

fn despawn_children_of<C: Component>(
    query: &Query<(Entity, &ChildOf), With<C>>,
    parent: Entity,
    commands: &mut Commands,
) {
    for (entity, child_of) in query {
        if child_of.parent() == parent {
            commands.entity(entity).despawn();
        }
    }
}

struct WorldRect {
    center_x: f32,
    center_y: f32,
}

fn bounds_to_world_rect(
    bounds: &BoundingBox,
    points_to_world: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> WorldRect {
    let width = bounds.width * points_to_world;
    let height = bounds.height * points_to_world;
    let left = bounds.x.mul_add(points_to_world, -anchor_x);
    let top = -(bounds.y.mul_add(points_to_world, -anchor_y));

    WorldRect {
        center_x: width.mul_add(0.5, left),
        center_y: height.mul_add(-0.5, top),
    }
}

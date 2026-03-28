//! Panel geometry rendering — rectangles and borders from layout render commands.
//!
//! Extracts `Rectangle` and `Border` render commands from computed panel layouts,
//! batches them into vertex-colored quad meshes, and spawns them as child entities.
//! Uses a shared unlit `StandardMaterial` with vertex colors for minimal draw calls.

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::picking::mesh_picking::ray_cast::RayCastBackfaces;
use bevy::prelude::*;

use super::panel_rtt::PanelRttRegistry;
use crate::layout::BoundingBox;
use crate::layout::RenderCommandKind;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::RenderMode;
use crate::plugin::SurfaceShadow;
use crate::plugin::UnitConfig;

/// Sort-key bias for the interaction mesh in Geometry mode.
/// Largest positive = furthest back = draws first, behind everything.
const INTERACTION_DEPTH_BIAS: f32 = 4.0;

/// Sort-key bias for background rectangles in Geometry mode.
/// Positive = farther from camera = draws first (behind everything).
const BACKGROUND_DEPTH_BIAS: f32 = 2.0;

/// Sort-key bias for border edges in Geometry mode.
/// Negative = closer to camera = draws last (on top of everything).
const BORDER_DEPTH_BIAS: f32 = -2.0;

/// Z offset for background rectangles (at the panel plane, behind text).
const RECTANGLE_Z_OFFSET: f32 = 0.0;

/// Z offset for border edges. Zero because layer ordering is handled by
/// `depth_bias` (Geometry mode) or is irrelevant (Texture mode composites flat).
const BORDER_Z_OFFSET: f32 = 0.0;

/// Marker for rectangle mesh entities spawned by the panel geometry renderer.
#[derive(Component)]
struct PanelRectMesh;

/// Marker for border mesh entities spawned by the panel geometry renderer.
#[derive(Component)]
struct PanelBorderMesh;

/// Marker for the invisible full-panel interaction mesh (Geometry mode only).
/// Enables picking across the entire panel area including transparent gaps.
#[derive(Component)]
struct PanelInteractionMesh;

/// Cached shared materials for vertex-colored panel geometry.
#[derive(Resource, Default)]
struct SharedPanelMaterials {
    /// Unlit material for RTT mode (all layers share one material).
    rtt:                 Option<Handle<StandardMaterial>>,
    /// Lit material with background depth bias for Geometry mode.
    geometry_background: Option<Handle<StandardMaterial>>,
    /// Lit material with border depth bias for Geometry mode.
    geometry_border:     Option<Handle<StandardMaterial>>,
    /// Fully transparent material for the interaction mesh (Geometry mode).
    interaction:         Option<Handle<StandardMaterial>>,
}

/// Per-quad data for building vertex-colored meshes.
struct ColoredQuad {
    /// Top-left corner in panel-local world space.
    position: [f32; 3],
    /// Width and height in world units.
    size:     [f32; 2],
    /// Linear RGBA color.
    color:    [f32; 4],
}

/// Plugin that adds panel geometry rendering (backgrounds and borders).
pub struct PanelGeometryPlugin;

impl Plugin for PanelGeometryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SharedPanelMaterials>();
        app.add_systems(
            PostUpdate,
            build_panel_geometry.after(super::panel_rtt::setup_panel_rtt),
        );
    }
}

/// Returns the shared RTT material (unlit, no depth bias).
fn rtt_material(
    shared: &mut SharedPanelMaterials,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    shared
        .rtt
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                unlit: true,
                ..default()
            })
        })
        .clone()
}

/// Returns the Geometry-mode background material (lit, positive depth bias).
fn geometry_background_material(
    shared: &mut SharedPanelMaterials,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    shared
        .geometry_background
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                depth_bias: BACKGROUND_DEPTH_BIAS,
                ..default()
            })
        })
        .clone()
}

/// Returns the Geometry-mode border material (lit, negative depth bias).
fn geometry_border_material(
    shared: &mut SharedPanelMaterials,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    shared
        .geometry_border
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                depth_bias: BORDER_DEPTH_BIAS,
                ..default()
            })
        })
        .clone()
}

/// Returns the fully transparent interaction material (Geometry mode).
fn interaction_material(
    shared: &mut SharedPanelMaterials,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    shared
        .interaction
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                depth_bias: INTERACTION_DEPTH_BIAS,
                ..default()
            })
        })
        .clone()
}

/// Extracts `Rectangle` and `Border` render commands from changed panels,
/// builds batched vertex-colored quad meshes, and spawns them as children.
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
    old_rects: Query<(Entity, &ChildOf), With<PanelRectMesh>>,
    old_borders: Query<(Entity, &ChildOf), With<PanelBorderMesh>>,
    old_interaction: Query<(Entity, &ChildOf), With<PanelInteractionMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shared_mats: ResMut<SharedPanelMaterials>,
    unit_config: Res<UnitConfig>,
    rtt_registry: Res<PanelRttRegistry>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed, panel_layers) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let pts_mpu = panel.points_to_world(&unit_config);
        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);

        // ── Collect rectangle quads ─────────────────────────────────
        let mut rect_quads: Vec<ColoredQuad> = Vec::new();

        for cmd in &result.commands {
            if let RenderCommandKind::Rectangle { color, .. } = &cmd.kind {
                let linear: LinearRgba = (*color).into();
                let color_array = [linear.red, linear.green, linear.blue, linear.alpha];
                rect_quads.push(bounds_to_quad(
                    &cmd.bounds,
                    pts_mpu,
                    anchor_x,
                    anchor_y,
                    RECTANGLE_Z_OFFSET,
                    color_array,
                ));
            }
        }

        // ── Collect border quads ────────────────────────────────────
        let mut border_quads: Vec<ColoredQuad> = Vec::new();

        for cmd in &result.commands {
            if let RenderCommandKind::Border { border } = &cmd.kind {
                let linear: LinearRgba = border.color.into();
                let color_array = [linear.red, linear.green, linear.blue, linear.alpha];
                let bounds = &cmd.bounds;

                emit_border_edge_quads(
                    &mut border_quads,
                    bounds,
                    border.top.value,
                    border.right.value,
                    border.bottom.value,
                    border.left.value,
                    pts_mpu,
                    anchor_x,
                    anchor_y,
                    color_array,
                );
            }
        }

        // ── Despawn old geometry ────────────────────────────────────
        for (entity, child_of) in &old_rects {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }
        for (entity, child_of) in &old_borders {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }
        for (entity, child_of) in &old_interaction {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }

        // ── Spawn new geometry ──────────────────────────────────────
        let is_geometry = panel.render_mode == RenderMode::Geometry;
        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
        let layer = rtt_registry
            .get_layer(panel_entity)
            .map_or(scene_layer.clone(), RenderLayers::layer);
        let suppress_surface_shadow = panel.surface_shadow == SurfaceShadow::Off;

        if !rect_quads.is_empty() {
            let rect_material = if is_geometry {
                geometry_background_material(&mut shared_mats, &mut materials)
            } else {
                rtt_material(&mut shared_mats, &mut materials)
            };
            let mesh = build_colored_quad_mesh(&rect_quads);
            let rect_base = (
                PanelRectMesh,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(rect_material),
                Transform::IDENTITY,
                layer.clone(),
            );
            if suppress_surface_shadow {
                commands
                    .entity(panel_entity)
                    .with_child((rect_base, NotShadowCaster));
            } else {
                commands.entity(panel_entity).with_child(rect_base);
            }
        }

        if !border_quads.is_empty() {
            let border_material = if is_geometry {
                geometry_border_material(&mut shared_mats, &mut materials)
            } else {
                rtt_material(&mut shared_mats, &mut materials)
            };
            let mesh = build_colored_quad_mesh(&border_quads);
            let border_base = (
                PanelBorderMesh,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(border_material),
                Transform::IDENTITY,
                layer.clone(),
            );
            if suppress_surface_shadow {
                commands
                    .entity(panel_entity)
                    .with_child((border_base, NotShadowCaster));
            } else {
                commands.entity(panel_entity).with_child(border_base);
            }
        }

        // ── Interaction mesh (Geometry mode only) ───────────────────
        // A full-panel transparent rectangle for picking. In RTT mode
        // the display quad already covers the full area.
        if is_geometry {
            let world_w = panel.world_width(&unit_config);
            let world_h = panel.world_height(&unit_config);
            let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);
            let center_x = world_w * 0.5 - anchor_x;
            let center_y = anchor_y - world_h * 0.5;

            let interact_mat = interaction_material(&mut shared_mats, &mut materials);
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

/// Emits up to four edge quads for a border (top, right, bottom, left).
///
/// Each edge with a non-zero width produces one thin rectangle. Corners
/// are shared: top and bottom edges span the full width; left and right
/// edges fit between them.
fn emit_border_edge_quads(
    quads: &mut Vec<ColoredQuad>,
    bounds: &BoundingBox,
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
    pts_mpu: f32,
    anchor_x: f32,
    anchor_y: f32,
    color: [f32; 4],
) {
    // Top edge — full width.
    if top > 0.0 {
        quads.push(bounds_to_quad(
            &BoundingBox {
                x:      bounds.x,
                y:      bounds.y,
                width:  bounds.width,
                height: top,
            },
            pts_mpu,
            anchor_x,
            anchor_y,
            BORDER_Z_OFFSET,
            color,
        ));
    }

    // Bottom edge — full width.
    if bottom > 0.0 {
        quads.push(bounds_to_quad(
            &BoundingBox {
                x:      bounds.x,
                y:      bounds.y + bounds.height - bottom,
                width:  bounds.width,
                height: bottom,
            },
            pts_mpu,
            anchor_x,
            anchor_y,
            BORDER_Z_OFFSET,
            color,
        ));
    }

    // Left edge — between top and bottom.
    if left > 0.0 {
        quads.push(bounds_to_quad(
            &BoundingBox {
                x:      bounds.x,
                y:      bounds.y + top,
                width:  left,
                height: bounds.height - top - bottom,
            },
            pts_mpu,
            anchor_x,
            anchor_y,
            BORDER_Z_OFFSET,
            color,
        ));
    }

    // Right edge — between top and bottom.
    if right > 0.0 {
        quads.push(bounds_to_quad(
            &BoundingBox {
                x:      bounds.x + bounds.width - right,
                y:      bounds.y + top,
                width:  right,
                height: bounds.height - top - bottom,
            },
            pts_mpu,
            anchor_x,
            anchor_y,
            BORDER_Z_OFFSET,
            color,
        ));
    }
}

/// Converts layout-space bounds to a world-space colored quad.
fn bounds_to_quad(
    bounds: &BoundingBox,
    pts_mpu: f32,
    anchor_x: f32,
    anchor_y: f32,
    z_offset: f32,
    color: [f32; 4],
) -> ColoredQuad {
    let world_w = bounds.width * pts_mpu;
    let world_h = bounds.height * pts_mpu;
    let world_x = bounds.x.mul_add(pts_mpu, -anchor_x);
    let world_y = -(bounds.y.mul_add(pts_mpu, -anchor_y));

    ColoredQuad {
        position: [world_x, world_y, z_offset],
        size: [world_w, world_h],
        color,
    }
}

/// Builds a `Mesh` from a list of vertex-colored quads.
///
/// Each quad produces 4 vertices and 6 indices (two triangles).
/// Vertex colors are in linear RGBA. UVs span [0,1] per quad
/// (required by `StandardMaterial` even without a texture).
fn build_colored_quad_mesh(quads: &[ColoredQuad]) -> Mesh {
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    let vertex_count = quads.len() * 4;
    let index_count = quads.len() * 6;

    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);
    let mut indices = Vec::with_capacity(index_count);

    for (idx, quad) in quads.iter().enumerate() {
        let [qx, qy, qz] = quad.position;
        let [qw, qh] = quad.size;

        #[allow(clippy::cast_possible_truncation)]
        let base = (idx * 4) as u32;

        // Quad vertices: TL, TR, BR, BL (Y-up coordinate system).
        positions.push([qx, qy, qz]); // TL
        positions.push([qx + qw, qy, qz]); // TR
        positions.push([qx + qw, qy - qh, qz]); // BR
        positions.push([qx, qy - qh, qz]); // BL

        // All normals point toward camera (+Z).
        normals.push([0.0, 0.0, 1.0]);
        normals.push([0.0, 0.0, 1.0]);
        normals.push([0.0, 0.0, 1.0]);
        normals.push([0.0, 0.0, 1.0]);

        // Unit UVs — `StandardMaterial` requires UV_0 even without a texture.
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([1.0, 1.0]);
        uvs.push([0.0, 1.0]);

        // Per-quad vertex color (linear RGBA).
        colors.push(quad.color);
        colors.push(quad.color);
        colors.push(quad.color);
        colors.push(quad.color);

        // Two triangles (CCW winding for front-face toward +Z):
        // TL-BL-BR and TL-BR-TR.
        indices.push(base);
        indices.push(base + 3);
        indices.push(base + 2);
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 1);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

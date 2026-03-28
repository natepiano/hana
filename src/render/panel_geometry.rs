//! Panel geometry rendering — rectangles and borders from layout render commands.
//!
//! Extracts `Rectangle` and `Border` render commands from computed panel layouts,
//! batches them into vertex-colored quad meshes, and spawns them as child entities.
//! Uses a shared unlit `StandardMaterial` with vertex colors for minimal draw calls.

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::panel_rtt::PanelRttRegistry;
use crate::layout::BoundingBox;
use crate::layout::RenderCommandKind;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::UnitConfig;

/// Z offset for background rectangles (at the panel plane, behind text).
const RECTANGLE_Z_OFFSET: f32 = 0.0;

/// Z offset for border edges (in front of text at 0.001).
const BORDER_Z_OFFSET: f32 = 0.002;

/// Marker for rectangle mesh entities spawned by the panel geometry renderer.
#[derive(Component)]
struct PanelRectMesh;

/// Marker for border mesh entities spawned by the panel geometry renderer.
#[derive(Component)]
struct PanelBorderMesh;

/// Cached shared material for vertex-colored panel geometry.
#[derive(Resource, Default)]
struct SharedPanelMaterial {
    handle: Option<Handle<StandardMaterial>>,
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
        app.init_resource::<SharedPanelMaterial>();
        app.add_systems(
            PostUpdate,
            build_panel_geometry.after(super::panel_rtt::setup_panel_rtt),
        );
    }
}

/// Returns the shared vertex-colored panel material, creating it on first use.
fn get_or_create_material(
    shared: &mut SharedPanelMaterial,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    shared
        .handle
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

/// Extracts `Rectangle` and `Border` render commands from changed panels,
/// builds batched vertex-colored quad meshes, and spawns them as children.
fn build_panel_geometry(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    old_rects: Query<(Entity, &ChildOf), With<PanelRectMesh>>,
    old_borders: Query<(Entity, &ChildOf), With<PanelBorderMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shared_mat: ResMut<SharedPanelMaterial>,
    unit_config: Res<UnitConfig>,
    rtt_registry: Res<PanelRttRegistry>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed) in &changed_panels {
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
                    border.top,
                    border.right,
                    border.bottom,
                    border.left,
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

        // ── Spawn new geometry ──────────────────────────────────────
        let material = get_or_create_material(&mut shared_mat, &mut materials);
        let layer = rtt_registry
            .get_layer(panel_entity)
            .map_or(RenderLayers::layer(0), RenderLayers::layer);

        if !rect_quads.is_empty() {
            let mesh = build_colored_quad_mesh(&rect_quads);
            commands.entity(panel_entity).with_child((
                PanelRectMesh,
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::IDENTITY,
                layer.clone(),
            ));
        }

        if !border_quads.is_empty() {
            let mesh = build_colored_quad_mesh(&border_quads);
            commands.entity(panel_entity).with_child((
                PanelBorderMesh,
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::IDENTITY,
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

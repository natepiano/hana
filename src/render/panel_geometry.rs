//! Panel geometry rendering — rectangles and borders from layout render commands.
//!
//! Each rectangle and border render command produces its own mesh entity,
//! enabling per-element picking and per-element material overrides. Materials
//! are resolved via the chain: element → panel → library default. Bevy's
//! automatic GPU batching merges entities that share the same material handle
//! into single draw calls.

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::picking::mesh_picking::ray_cast::RayCastBackfaces;
use bevy::prelude::*;

use super::constants::LAYER_Z_STEP;
use super::constants::resolve_material;
use super::panel_rtt::PanelRttRegistry;
use crate::layout::BoundingBox;
use crate::layout::RenderCommandKind;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::RenderMode;
use crate::plugin::SurfaceShadow;
use crate::plugin::UnitConfig;

/// Marker for rectangle mesh entities spawned by the panel geometry renderer.
#[derive(Component)]
struct PanelRectMesh;

/// Marker for border mesh entities spawned by the panel geometry renderer.
#[derive(Component)]
struct PanelBorderMesh;

/// Marker for the invisible full-panel interaction mesh (Geometry mode only).
#[derive(Component)]
struct PanelInteractionMesh;

/// Plugin that adds panel geometry rendering (backgrounds and borders).
pub struct PanelGeometryPlugin;

impl Plugin for PanelGeometryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            build_panel_geometry.after(super::panel_rtt::setup_panel_rtt),
        );
    }
}

/// Extracts `Rectangle` and `Border` render commands from changed panels
/// and spawns one mesh entity per element with its resolved material.
#[allow(clippy::too_many_arguments)]
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
        let is_geometry = panel.render_mode == RenderMode::Geometry;
        let suppress_shadow = panel.surface_shadow == SurfaceShadow::Off;
        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
        let layer = rtt_registry
            .get_layer(panel_entity)
            .map_or(scene_layer, RenderLayers::layer);

        // ── Despawn old geometry ────────────────────────────────────
        despawn_children_of(&old_rects, panel_entity, &mut commands);
        despawn_children_of(&old_borders, panel_entity, &mut commands);
        despawn_children_of(&old_interaction, panel_entity, &mut commands);

        // ── Spawn per-element geometry ──────────────────────────────
        // Commands are emitted back-to-front: parent backgrounds first,
        // then children, then borders. Each command gets a micro Z offset
        // toward the camera so later commands render on top.
        for (cmd_index, cmd) in result.commands.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let z_offset = if is_geometry {
                cmd_index as f32 * LAYER_Z_STEP
            } else {
                0.0
            };

            match &cmd.kind {
                RenderCommandKind::Rectangle { color, .. } => {
                    let element_mat = panel.tree.element_material(cmd.element_idx);
                    let mut resolved =
                        resolve_material(element_mat, panel.material.as_ref(), Some(*color));
                    configure_surface_material(&mut resolved, is_geometry);

                    let quad = bounds_to_world_rect(&cmd.bounds, pts_mpu, anchor_x, anchor_y);
                    spawn_surface_entity(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        panel_entity,
                        PanelRectMesh,
                        quad,
                        z_offset,
                        resolved,
                        &layer,
                        suppress_shadow,
                    );
                },
                RenderCommandKind::Border { border } => {
                    let element_mat = panel.tree.element_material(cmd.element_idx);
                    let mut resolved =
                        resolve_material(element_mat, panel.material.as_ref(), Some(border.color));
                    configure_surface_material(&mut resolved, is_geometry);

                    let edge_rects = border_edge_rects(
                        &cmd.bounds,
                        border.top.value,
                        border.right.value,
                        border.bottom.value,
                        border.left.value,
                    );
                    let mat_handle = materials.add(resolved);

                    for edge_bounds in edge_rects {
                        let quad = bounds_to_world_rect(&edge_bounds, pts_mpu, anchor_x, anchor_y);
                        let mesh = meshes.add(Rectangle::new(quad.width, quad.height));
                        let base = (
                            PanelBorderMesh,
                            Mesh3d(mesh),
                            MeshMaterial3d(mat_handle.clone()),
                            Transform::from_xyz(quad.center_x, quad.center_y, z_offset),
                            layer.clone(),
                        );
                        if suppress_shadow {
                            commands
                                .entity(panel_entity)
                                .with_child((base, NotShadowCaster));
                        } else {
                            commands.entity(panel_entity).with_child(base);
                        }
                    }
                },
                _ => {},
            }
        }

        // ── Interaction mesh (Geometry mode only) ───────────────────
        if is_geometry {
            let world_w = panel.world_width(&unit_config);
            let world_h = panel.world_height(&unit_config);
            let center_x = world_w * 0.5 - anchor_x;
            let center_y = anchor_y - world_h * 0.5;

            let interact_mat = StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            };
            commands.entity(panel_entity).with_child((
                PanelInteractionMesh,
                RayCastBackfaces,
                NotShadowCaster,
                Mesh3d(meshes.add(Rectangle::new(world_w, world_h))),
                MeshMaterial3d(materials.add(interact_mat)),
                Transform::from_xyz(center_x, center_y, -LAYER_Z_STEP),
                layer,
            ));
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Despawns all entities in `query` that are children of `parent`.
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

/// Configures a resolved material for panel surface rendering.
///
/// In Geometry mode: sets `depth_bias` for sort ordering, chooses
/// `AlphaMode::Opaque` or `Blend` based on the resolved `base_color` alpha.
/// In RTT mode: sets `unlit: true` and always uses `Blend` (compositing
/// onto a transparent background).
fn configure_surface_material(material: &mut StandardMaterial, is_geometry: bool) {
    material.double_sided = true;
    material.cull_mode = None;

    if is_geometry {
        // Opaque elements go through the opaque phase with correct PBR.
        // Only actually transparent elements use Blend.
        let alpha = material.base_color.alpha();
        material.alpha_mode = if alpha < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };
    } else {
        material.unlit = true;
        material.alpha_mode = AlphaMode::Blend;
    }
}

/// World-space rectangle computed from layout bounds.
struct WorldRect {
    center_x: f32,
    center_y: f32,
    width:    f32,
    height:   f32,
}

/// Converts layout-space bounds to a world-space rectangle.
fn bounds_to_world_rect(
    bounds: &BoundingBox,
    pts_mpu: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> WorldRect {
    let width = bounds.width * pts_mpu;
    let height = bounds.height * pts_mpu;
    let left = bounds.x.mul_add(pts_mpu, -anchor_x);
    let top = -(bounds.y.mul_add(pts_mpu, -anchor_y));

    WorldRect {
        center_x: left + width * 0.5,
        center_y: top - height * 0.5,
        width,
        height,
    }
}

/// Spawns a surface mesh entity (rectangle or single border edge) as a
/// child of the panel.
fn spawn_surface_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    panel_entity: Entity,
    marker: impl Component,
    quad: WorldRect,
    z_offset: f32,
    material: StandardMaterial,
    layer: &RenderLayers,
    suppress_shadow: bool,
) {
    let mesh = meshes.add(Rectangle::new(quad.width, quad.height));
    let mat_handle = materials.add(material);
    let base = (
        marker,
        Mesh3d(mesh),
        MeshMaterial3d(mat_handle),
        Transform::from_xyz(quad.center_x, quad.center_y, z_offset),
        layer.clone(),
    );
    if suppress_shadow {
        commands
            .entity(panel_entity)
            .with_child((base, NotShadowCaster));
    } else {
        commands.entity(panel_entity).with_child(base);
    }
}

/// Returns up to 4 edge bounding boxes for a border.
fn border_edge_rects(
    bounds: &BoundingBox,
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
) -> Vec<BoundingBox> {
    let mut edges = Vec::with_capacity(4);

    if top > 0.0 {
        edges.push(BoundingBox {
            x:      bounds.x,
            y:      bounds.y,
            width:  bounds.width,
            height: top,
        });
    }
    if bottom > 0.0 {
        edges.push(BoundingBox {
            x:      bounds.x,
            y:      bounds.y + bounds.height - bottom,
            width:  bounds.width,
            height: bottom,
        });
    }
    if left > 0.0 {
        edges.push(BoundingBox {
            x:      bounds.x,
            y:      bounds.y + top,
            width:  left,
            height: bounds.height - top - bottom,
        });
    }
    if right > 0.0 {
        edges.push(BoundingBox {
            x:      bounds.x + bounds.width - right,
            y:      bounds.y + top,
            width:  right,
            height: bounds.height - top - bottom,
        });
    }

    edges
}

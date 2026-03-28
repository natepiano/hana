//! Panel geometry rendering — SDF rounded rectangles from layout render commands.
//!
//! Each element with a background or border produces one SDF quad entity.
//! The SDF fragment shader renders both fill and border in a single pass
//! with pixel-perfect anti-aliased edges and per-corner radii.

use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::picking::mesh_picking::ray_cast::RayCastBackfaces;
use bevy::prelude::*;

use super::constants::LAYER_Z_STEP;
use super::constants::resolve_material;
use super::panel_rtt::PanelRttRegistry;
use super::sdf_material::SdfPanelMaterial;
use crate::layout::BoundingBox;
use crate::layout::RectangleSource;
use crate::layout::RenderCommandKind;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::RenderMode;
use crate::plugin::SurfaceShadow;
use crate::plugin::UnitConfig;

/// Marker for SDF panel mesh entities.
#[derive(Component)]
struct PanelSdfMesh;

/// Marker for between-children divider mesh entities.
#[derive(Component)]
struct PanelDividerMesh;

/// Marker for the invisible full-panel interaction mesh (Geometry mode only).
#[derive(Component)]
struct PanelInteractionMesh;

/// Plugin that adds panel geometry rendering (backgrounds and borders).
pub struct PanelGeometryPlugin;

impl Plugin for PanelGeometryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SdfPanelMaterial>::default());
        app.add_systems(
            PostUpdate,
            build_panel_geometry.after(super::panel_rtt::setup_panel_rtt),
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
    /// Index of the first render command for this element (for Z ordering).
    command_index: usize,
}

/// Extracts render commands, gathers fill + border per element, and
/// spawns one SDF quad per element.
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
    old_sdf: Query<(Entity, &ChildOf), With<PanelSdfMesh>>,
    old_dividers: Query<(Entity, &ChildOf), With<PanelDividerMesh>>,
    old_interaction: Query<(Entity, &ChildOf), With<PanelInteractionMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut sdf_materials: ResMut<Assets<SdfPanelMaterial>>,
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
        despawn_children_of(&old_sdf, panel_entity, &mut commands);
        despawn_children_of(&old_dividers, panel_entity, &mut commands);
        despawn_children_of(&old_interaction, panel_entity, &mut commands);

        // ── Gather fill + border per element ────────────────────────
        let mut surfaces: HashMap<usize, ElementSurface> = HashMap::new();
        let mut divider_commands: Vec<(usize, BoundingBox, Color)> = Vec::new();

        for (cmd_index, cmd) in result.commands.iter().enumerate() {
            match &cmd.kind {
                RenderCommandKind::Rectangle { color, source } => {
                    if *source == RectangleSource::BetweenChildrenBorder {
                        // Between-children dividers are simple quads, not SDF.
                        divider_commands.push((cmd_index, cmd.bounds, *color));
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
                            })
                            .fill_color = Some(*color);
                    }
                },
                RenderCommandKind::Border { border } => {
                    let surface =
                        surfaces
                            .entry(cmd.element_idx)
                            .or_insert_with(|| ElementSurface {
                                element_idx:   cmd.element_idx,
                                bounds:        cmd.bounds,
                                fill_color:    None,
                                border_widths: [0.0; 4],
                                border_color:  None,
                                command_index: cmd_index,
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

        // ── Spawn SDF quads ─────────────────────────────────────────
        for surface in surfaces.values() {
            let element_mat = panel.tree.element_material(surface.element_idx);
            let corner_radius = panel.tree.element_corner_radius(surface.element_idx);

            // Resolve the base StandardMaterial. If there's no fill color
            // and no custom material, use transparent so border-only
            // elements don't render a white fill.
            let effective_color = surface.fill_color.or_else(|| {
                if element_mat.is_some() || panel.material.is_some() {
                    None // let resolve_material use the material's base_color
                } else {
                    Some(Color::NONE) // no fill, no material → transparent
                }
            });
            let mut base = resolve_material(element_mat, panel.material.as_ref(), effective_color);
            if !is_geometry {
                base.unlit = true;
            }

            // Convert layout bounds to world dimensions.
            let world_w = surface.bounds.width * pts_mpu;
            let world_h = surface.bounds.height * pts_mpu;
            let half_w = world_w * 0.5;
            let half_h = world_h * 0.5;

            // Convert corner radii directly to world meters from their
            // original units (Mm, Pt, etc.). Bare f32 values use the
            // panel's layout unit conversion factor.
            let layout_mpu = panel
                .layout_unit
                .map_or(unit_config.layout, |u| u)
                .meters_per_unit();
            let world_radii = corner_radius.to_meters_array(layout_mpu);

            // Convert border widths to world units.
            let world_borders = [
                surface.border_widths[0] * pts_mpu,
                surface.border_widths[1] * pts_mpu,
                surface.border_widths[2] * pts_mpu,
                surface.border_widths[3] * pts_mpu,
            ];

            let sdf_mat = super::sdf_material::sdf_panel_material(
                base,
                half_w,
                half_h,
                world_radii,
                world_borders,
                surface.border_color,
            );

            // World-space position.
            let world_rect = bounds_to_world_rect(&surface.bounds, pts_mpu, anchor_x, anchor_y);

            #[allow(clippy::cast_precision_loss)]
            let z_offset = if is_geometry {
                surface.command_index as f32 * LAYER_Z_STEP
            } else {
                0.0
            };

            let mesh = meshes.add(Rectangle::new(world_w, world_h));
            let mat_handle = sdf_materials.add(sdf_mat);

            let base_components = (
                PanelSdfMesh,
                Mesh3d(mesh),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(world_rect.center_x, world_rect.center_y, z_offset),
                layer.clone(),
            );
            if suppress_shadow {
                commands
                    .entity(panel_entity)
                    .with_child((base_components, NotShadowCaster));
            } else {
                commands.entity(panel_entity).with_child(base_components);
            }
        }

        // ── Spawn between-children dividers (simple quads) ──────────
        for (cmd_index, bounds, color) in &divider_commands {
            let element_mat_option: Option<&StandardMaterial> = None;
            let mut base =
                resolve_material(element_mat_option, panel.material.as_ref(), Some(*color));
            base.alpha_mode = AlphaMode::Blend;
            base.double_sided = true;
            base.cull_mode = None;
            if !is_geometry {
                base.unlit = true;
            }

            let world_rect = bounds_to_world_rect(bounds, pts_mpu, anchor_x, anchor_y);

            #[allow(clippy::cast_precision_loss)]
            let z_offset = if is_geometry {
                *cmd_index as f32 * LAYER_Z_STEP
            } else {
                0.0
            };

            let mesh = meshes.add(Rectangle::new(world_rect.width, world_rect.height));
            let mat_handle = standard_materials.add(base);

            let base_components = (
                PanelDividerMesh,
                Mesh3d(mesh),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(world_rect.center_x, world_rect.center_y, z_offset),
                layer.clone(),
            );
            if suppress_shadow {
                commands
                    .entity(panel_entity)
                    .with_child((base_components, NotShadowCaster));
            } else {
                commands.entity(panel_entity).with_child(base_components);
            }
        }

        // ── Interaction mesh (Geometry mode only) ───────────────────
        if is_geometry {
            let world_w = panel.world_width(&unit_config);
            let world_h = panel.world_height(&unit_config);
            let center_x = world_w * 0.5 - anchor_x;
            let center_y = anchor_y - world_h * 0.5;

            let interact_mat = standard_materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            commands.entity(panel_entity).with_child((
                PanelInteractionMesh,
                RayCastBackfaces,
                NotShadowCaster,
                Mesh3d(meshes.add(Rectangle::new(world_w, world_h))),
                MeshMaterial3d(interact_mat),
                Transform::from_xyz(center_x, center_y, -LAYER_Z_STEP),
                layer,
            ));
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

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
    width:    f32,
    height:   f32,
}

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

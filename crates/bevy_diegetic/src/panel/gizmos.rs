//! Panel debug gizmo rendering — layout and text-bounds overlays.

use std::collections::HashMap;

use bevy::prelude::*;

use super::diegetic_panel::ComputedDiegeticPanel;
use super::diegetic_panel::DiegeticPanel;
use super::panel_mode::RenderMode;
use crate::layout::Border;
use crate::layout::BoundingBox;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;

/// Gizmo group for diegetic panel debug wireframes.
///
/// Enable or disable via Bevy's [`GizmoConfigStore`].
///
/// **Note:** This API is provisional. Once panels render real geometry
/// (Phase 4), debug visualization will likely move to a per-panel debug
/// mode rather than a separate gizmo group.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct DiegeticPanelGizmoGroup;

/// Controls whether debug gizmos (text bounding boxes, element outlines)
/// are drawn. Toggle at runtime to debug layout measurement and positioning.
#[derive(Resource, Default)]
pub enum ShowTextGizmos {
    /// Debug gizmos are not drawn (default).
    #[default]
    Hidden,
    /// Debug gizmos are drawn.
    Shown,
}

/// Marker on gizmo entities spawned by the layout gizmo renderer.
#[derive(Component)]
pub(super) struct PanelGizmoChild;

/// Marker on gizmo entities spawned by the debug gizmo renderer.
#[derive(Component)]
pub(super) struct DebugGizmoChild;

/// Enables perspective-scaled line widths on panel debug gizmos.
pub(super) fn configure_panel_gizmos(mut config_store: ResMut<GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<DiegeticPanelGizmoGroup>();
    config.line.perspective = true;
}

/// Approximate pixels-per-meter from the first camera's projection.
fn pixels_per_meter(cameras: &Query<(&Camera, &Projection)>) -> f32 {
    cameras
        .iter()
        .next()
        .and_then(|(cam, proj)| {
            let vp_height = cam.logical_viewport_size()?.y;
            match proj {
                Projection::Perspective(p) => Some(vp_height / (2.0 * (p.fov / 2.0).tan())),
                Projection::Orthographic(o) => Some(vp_height / o.scale),
                Projection::Custom(_) => None,
            }
        })
        .unwrap_or(1000.0)
}

enum GizmoChildMarker {
    Layout,
    Debug,
}

struct GizmoRect<'a> {
    bounds:          &'a BoundingBox,
    points_to_world: f32,
    anchor_x:        f32,
    anchor_y:        f32,
    color:           Color,
    line_width:      f32,
    marker:          GizmoChildMarker,
}

fn spawn_rect_gizmo(
    commands: &mut Commands,
    panel_entity: Entity,
    gizmo_assets: &mut Assets<GizmoAsset>,
    rect: &GizmoRect<'_>,
) {
    let mut asset = GizmoAsset::default();
    add_rect_to_gizmo(
        &mut asset,
        rect.bounds,
        rect.points_to_world,
        rect.anchor_x,
        rect.anchor_y,
        rect.color,
    );
    let gizmo = Gizmo {
        handle: gizmo_assets.add(asset),
        line_config: GizmoLineConfig {
            width: rect.line_width,
            perspective: false,
            joints: GizmoLineJoint::Round(8),
            ..default()
        },
        ..default()
    };
    match rect.marker {
        GizmoChildMarker::Layout => {
            commands
                .entity(panel_entity)
                .with_child((PanelGizmoChild, gizmo, Transform::IDENTITY));
        },
        GizmoChildMarker::Debug => {
            commands
                .entity(panel_entity)
                .with_child((DebugGizmoChild, gizmo, Transform::IDENTITY));
        },
    }
}

/// Renders layout visuals (backgrounds, borders, between-children
/// dividers) as retained gizmos. This is the production rendering
/// path for panel layout geometry — always active.
pub(super) fn render_layout_gizmos(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_gizmos: Query<(Entity, &ChildOf), With<PanelGizmoChild>>,
    cameras: Query<(&Camera, &Projection)>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    if changed_panels.is_empty() {
        return;
    }

    let screen_pixels_per_meter = pixels_per_meter(&cameras);

    for (panel_entity, panel, computed) in &changed_panels {
        let is_screen_space = panel.mode().is_screen();
        if panel.render_mode() == RenderMode::Geometry {
            continue;
        }

        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        despawn_gizmo_children(&mut commands, &existing_gizmos, panel_entity);
        let (anchor_x, anchor_y) = panel.anchor_offsets();

        let border_by_idx = collect_borders_by_index(&result.commands);

        for cmd in &result.commands {
            match &cmd.kind {
                RenderCommandKind::Rectangle { color, .. } => {
                    let border = border_by_idx.get(&cmd.element_idx);
                    let (inset_left, inset_right, inset_top, inset_bottom) = border
                        .map_or((0.0, 0.0, 0.0, 0.0), |b| {
                            (b.left.value, b.right.value, b.top.value, b.bottom.value)
                        });
                    let inset_bounds = BoundingBox {
                        x:      cmd.bounds.x + inset_left,
                        y:      cmd.bounds.y + inset_top,
                        width:  (cmd.bounds.width - inset_left - inset_right).max(0.0),
                        height: (cmd.bounds.height - inset_top - inset_bottom).max(0.0),
                    };
                    spawn_rect_gizmo(
                        &mut commands,
                        panel_entity,
                        &mut gizmo_assets,
                        &GizmoRect {
                            bounds: &inset_bounds,
                            points_to_world,
                            anchor_x,
                            anchor_y,
                            color: *color,
                            line_width: 1.0,
                            marker: GizmoChildMarker::Layout,
                        },
                    );
                },
                RenderCommandKind::Border { border } => {
                    let half_left = border.left.value * 0.5;
                    let half_right = border.right.value * 0.5;
                    let half_top = border.top.value * 0.5;
                    let half_bottom = border.bottom.value * 0.5;
                    let has_sides = border.left.value > 0.0
                        || border.right.value > 0.0
                        || border.top.value > 0.0
                        || border.bottom.value > 0.0;
                    if has_sides {
                        let inset_bounds = BoundingBox {
                            x:      cmd.bounds.x + half_left,
                            y:      cmd.bounds.y + half_top,
                            width:  (cmd.bounds.width - half_left - half_right).max(0.0),
                            height: (cmd.bounds.height - half_top - half_bottom).max(0.0),
                        };
                        let avg_border_pts = (border.left.value
                            + border.right.value
                            + border.top.value
                            + border.bottom.value)
                            / 4.0;
                        let border_px = if is_screen_space {
                            avg_border_pts.max(1.0)
                        } else {
                            (avg_border_pts * points_to_world * screen_pixels_per_meter).max(1.0)
                        };
                        spawn_rect_gizmo(
                            &mut commands,
                            panel_entity,
                            &mut gizmo_assets,
                            &GizmoRect {
                                bounds: &inset_bounds,
                                points_to_world,
                                anchor_x,
                                anchor_y,
                                color: border.color,
                                line_width: border_px,
                                marker: GizmoChildMarker::Layout,
                            },
                        );
                    }
                },
                _ => {},
            }
        }
    }
}

/// Renders debug overlays (text bounding boxes, element outlines) as
/// retained gizmos. Only active when [`ShowTextGizmos`] is enabled.
/// Separate from layout gizmos so debug can be toggled independently.
pub(super) fn render_debug_gizmos(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_gizmos: Query<(Entity, &ChildOf), With<DebugGizmoChild>>,
    show_text: Res<ShowTextGizmos>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    if !matches!(*show_text, ShowTextGizmos::Shown) || changed_panels.is_empty() {
        return;
    }

    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        despawn_gizmo_children(&mut commands, &existing_gizmos, panel_entity);
        let (anchor_x, anchor_y) = panel.anchor_offsets();

        for cmd in &result.commands {
            if matches!(cmd.kind, RenderCommandKind::Text { .. }) {
                spawn_rect_gizmo(
                    &mut commands,
                    panel_entity,
                    &mut gizmo_assets,
                    &GizmoRect {
                        bounds: &cmd.bounds,
                        points_to_world,
                        anchor_x,
                        anchor_y,
                        color: Color::srgba(0.9, 0.9, 0.2, 0.2),
                        line_width: 1.0,
                        marker: GizmoChildMarker::Debug,
                    },
                );
            }
        }
    }
}

fn despawn_gizmo_children<T: Component>(
    commands: &mut Commands,
    existing_gizmos: &Query<(Entity, &ChildOf), With<T>>,
    panel_entity: Entity,
) {
    for (entity, child_of) in existing_gizmos {
        if child_of.parent() == panel_entity {
            commands.entity(entity).despawn();
        }
    }
}

fn collect_borders_by_index(commands: &[RenderCommand]) -> HashMap<usize, &Border> {
    let mut border_by_index = HashMap::new();
    for command in commands {
        if let RenderCommandKind::Border { ref border } = command.kind {
            border_by_index.insert(command.element_idx, border);
        }
    }
    border_by_index
}

/// Adds a rectangle outline to a `GizmoAsset` in panel-local coordinates.
fn add_rect_to_gizmo(
    asset: &mut GizmoAsset,
    bounds: &BoundingBox,
    scale: f32,
    anchor_x: f32,
    anchor_y: f32,
    color: Color,
) {
    let left = bounds.x.mul_add(scale, -anchor_x);
    let right = (bounds.x + bounds.width).mul_add(scale, -anchor_x);
    let top = (-bounds.y).mul_add(scale, anchor_y);
    let bottom = (-(bounds.y + bounds.height)).mul_add(scale, anchor_y);

    let tl = Vec3::new(left, top, 0.0);
    let tr = Vec3::new(right, top, 0.0);
    let br = Vec3::new(right, bottom, 0.0);
    let bl = Vec3::new(left, bottom, 0.0);

    asset.line(tl, tr, color);
    asset.line(tr, br, color);
    asset.line(br, bl, color);
    asset.line(bl, tl, color);
}

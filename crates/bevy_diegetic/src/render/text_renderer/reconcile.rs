use std::collections::HashMap;
use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::batching::SharedMsdfMaterials;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::RenderCommandKind;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::panel::RenderMode;
use crate::render::clip;
use crate::render::constants;
use crate::render::constants::TEXT_Z_OFFSET;
use crate::render::panel_rtt::PanelRttRegistry;
use crate::render::world_text::PanelTextChild;
use crate::render::world_text::WorldText;
use crate::text::MsdfAtlas;

/// Polls completed async glyph rasterizations, inserts them into the
/// atlas, and syncs to GPU. Entities with `PendingGlyphs` will be
/// re-checked by `shape_panel_text_children` and `render_world_text`.
pub(super) fn poll_atlas_glyphs(
    mut atlas: ResMut<MsdfAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut shared_mats: ResMut<SharedMsdfMaterials>,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    let poll_start = Instant::now();
    let poll_stats = atlas.poll_async_glyphs_stats();
    let poll_ms = poll_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    let dirty_pages = atlas.dirty_page_count();
    let mut sync_ms = 0.0;

    if poll_stats.inserted > 0 || poll_stats.invisible > 0 {
        let sync_start = Instant::now();
        atlas.sync_to_gpu(&mut images);
        sync_ms = sync_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
        shared_mats.clear();
    }

    perf.atlas.poll_ms = poll_ms;
    perf.atlas.sync_ms = sync_ms;
    perf.atlas.completed_glyphs = poll_stats.completed;
    perf.atlas.inserted_glyphs = poll_stats.inserted;
    perf.atlas.invisible_glyphs = poll_stats.invisible;
    perf.atlas.pages_added = poll_stats.pages_added;
    perf.atlas.dirty_pages = dirty_pages;
    perf.atlas.in_flight_glyphs = atlas.in_flight_count();
    perf.atlas.active_jobs = atlas.active_job_count();
    perf.atlas.peak_active_jobs = atlas.peak_active_job_count();
    perf.atlas.worker_threads = poll_stats.worker_threads;
    perf.atlas.avg_raster_ms = poll_stats.avg_raster_ms;
    perf.atlas.max_raster_ms = poll_stats.max_raster_ms;
    perf.atlas.batch_max_active_jobs = poll_stats.max_active_jobs;
    perf.atlas.total_glyphs = atlas.glyph_count();

    if poll_stats.completed > 0 || sync_ms > 0.0 {
        bevy::log::debug!(
            "poll_atlas_glyphs: poll={poll_ms:.2}ms sync={sync_ms:.2}ms completed={} inserted={} invisible={} pages_added={} dirty_pages={} in_flight={} active_jobs={} peak_active={} workers={} avg_raster={:.2}ms max_raster={:.2}ms batch_max_active={} total_glyphs={}",
            poll_stats.completed,
            poll_stats.inserted,
            poll_stats.invisible,
            poll_stats.pages_added,
            dirty_pages,
            atlas.in_flight_count(),
            atlas.active_job_count(),
            atlas.peak_active_job_count(),
            poll_stats.worker_threads,
            poll_stats.avg_raster_ms,
            poll_stats.max_raster_ms,
            poll_stats.max_active_jobs,
            atlas.glyph_count(),
        );
    }
}

/// Reconciles [`WorldText`] children for each changed [`ComputedDiegeticPanel`].
pub(super) fn reconcile_panel_text_children(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_children: Query<(Entity, &PanelTextChild, &ChildOf)>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        let scale_x = points_to_world;
        let scale_y = points_to_world;
        let (anchor_x, anchor_y) = panel.anchor_offsets();

        let clip_rects = clip::compute_clip_rects(&result.commands);
        let text_commands: Vec<_> = result
            .commands
            .iter()
            .enumerate()
            .filter_map(|(cmd_index, cmd)| match &cmd.kind {
                RenderCommandKind::Text { text, config } => Some((
                    cmd.element_idx,
                    cmd_index,
                    text.clone(),
                    config.clone(),
                    cmd.bounds,
                    clip_rects[cmd_index],
                )),
                _ => None,
            })
            .collect();

        let mut existing_by_key: HashMap<(usize, usize), Entity> = HashMap::new();
        for (entity, panel_text_child, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_key.insert(
                    (panel_text_child.element_idx, panel_text_child.command_index),
                    entity,
                );
            }
        }

        let mut visited_keys: Vec<(usize, usize)> = Vec::new();
        for (element_idx, cmd_index, text, config, bounds, clip) in &text_commands {
            let style = config.as_standalone();
            let panel_text_child = PanelTextChild {
                element_idx: *element_idx,
                command_index: *cmd_index,
                bounds: *bounds,
                scale_x,
                scale_y,
                anchor_x,
                anchor_y,
                clip_rect: *clip,
            };

            let key = (*element_idx, *cmd_index);
            visited_keys.push(key);

            if let Some(&child_entity) = existing_by_key.get(&key) {
                commands.entity(child_entity).insert((
                    WorldText(text.clone()),
                    style,
                    panel_text_child,
                ));
            } else {
                commands.entity(panel_entity).with_child((
                    WorldText(text.clone()),
                    style,
                    panel_text_child,
                ));
            }
        }

        for (entity, panel_text_child, child_of) in &existing_children {
            if child_of.parent() == panel_entity
                && !visited_keys
                    .contains(&(panel_text_child.element_idx, panel_text_child.command_index))
            {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Marker on image child entities spawned by the panel image reconciler.
#[derive(Component, Clone, Debug)]
pub(super) struct PanelImageChild {
    /// Index of the source element in the layout tree.
    pub element_idx: usize,
}

/// Reconciles image children for each changed [`ComputedDiegeticPanel`].
pub(super) fn reconcile_panel_image_children(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_children: Query<(Entity, &PanelImageChild, &ChildOf)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    rtt_registry: Res<PanelRttRegistry>,
) {
    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        let (anchor_x, anchor_y) = panel.anchor_offsets();
        let layer = rtt_registry
            .get_layer(panel_entity)
            .map_or(RenderLayers::layer(0), RenderLayers::layer);
        let is_geometry = panel.render_mode() == RenderMode::Geometry;

        let clip_rects = clip::compute_clip_rects(&result.commands);
        let image_commands: Vec<_> = result
            .commands
            .iter()
            .enumerate()
            .filter_map(|(cmd_index, cmd)| match &cmd.kind {
                RenderCommandKind::Image { handle, tint } => {
                    let clip = clip_rects[cmd_index];
                    if clip.is_some_and(|active_clip| cmd.bounds.intersect(&active_clip).is_none())
                    {
                        None
                    } else {
                        Some((
                            cmd_index,
                            cmd.element_idx,
                            handle.clone(),
                            *tint,
                            cmd.bounds,
                        ))
                    }
                },
                _ => None,
            })
            .collect();

        let mut existing_by_idx: HashMap<usize, Entity> = HashMap::new();
        for (entity, panel_image_child, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_idx.insert(panel_image_child.element_idx, entity);
            }
        }

        let mut visited_indices: Vec<usize> = Vec::new();
        for (cmd_index, element_idx, handle, tint, bounds) in &image_commands {
            visited_indices.push(*element_idx);

            let world_w = bounds.width * points_to_world;
            let world_h = bounds.height * points_to_world;
            let world_x = bounds.x.mul_add(points_to_world, world_w * 0.5) - anchor_x;
            let world_y = -(bounds.y.mul_add(points_to_world, world_h * 0.5) - anchor_y);

            let mesh_handle = meshes.add(Rectangle::new(world_w, world_h));
            let material_handle = materials.add(StandardMaterial {
                base_color: *tint,
                base_color_texture: Some(handle.clone()),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Blend,
                depth_bias: if is_geometry {
                    cmd_index.to_f32() * constants::LAYER_DEPTH_BIAS
                } else {
                    0.0
                },
                ..default()
            });

            let transform = Transform::from_xyz(world_x, world_y, TEXT_Z_OFFSET);
            let panel_image_child = PanelImageChild {
                element_idx: *element_idx,
            };

            if let Some(&child_entity) = existing_by_idx.get(element_idx) {
                commands.entity(child_entity).insert((
                    panel_image_child,
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                    transform,
                    layer.clone(),
                ));
            } else {
                commands.entity(panel_entity).with_child((
                    panel_image_child,
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                    transform,
                    layer.clone(),
                ));
            }
        }

        for (entity, panel_image_child, child_of) in &existing_children {
            if child_of.parent() == panel_entity
                && !visited_indices.contains(&panel_image_child.element_idx)
            {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::HashMap;

    use bevy::prelude::*;
    use bevy_kana::ToF32;

    use crate::layout::BoundingBox;
    use crate::render::world_text::PanelTextChild;

    #[test]
    fn reconcile_keys_by_element_and_command_index() {
        let existing: Vec<(Entity, PanelTextChild)> = (0..3)
            .map(|cmd| {
                let panel_text_child = PanelTextChild {
                    element_idx:   7,
                    command_index: cmd,
                    bounds:        BoundingBox {
                        x:      0.0,
                        y:      cmd.to_f32() * 10.0,
                        width:  100.0,
                        height: 10.0,
                    },
                    scale_x:       1.0,
                    scale_y:       1.0,
                    anchor_x:      0.0,
                    anchor_y:      0.0,
                    clip_rect:     None,
                };
                (
                    Entity::from_raw_u32(cmd.try_into().expect("small")).expect("valid"),
                    panel_text_child,
                )
            })
            .collect();

        let mut by_key: HashMap<(usize, usize), Entity> = HashMap::new();
        for (entity, panel_text_child) in &existing {
            by_key.insert(
                (panel_text_child.element_idx, panel_text_child.command_index),
                *entity,
            );
        }
        assert_eq!(by_key.len(), 3);

        let mut by_element_only: HashMap<usize, Entity> = HashMap::new();
        for (entity, panel_text_child) in &existing {
            by_element_only.insert(panel_text_child.element_idx, *entity);
        }
        assert_eq!(by_element_only.len(), 1);
    }
}

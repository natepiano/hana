//! Batched-records text geometry (`docs/bevy_diegetic/glyph_instancing.md`):
//! routes every panel-text run into a per-[`BatchKey`] batch entity whose
//! vertex shader pulls per-glyph and per-run records from GPU tables.
//!
//! Frame flow on the plan's schedule anchors: [`update_panel_text_batches`]
//! writes glyph/run records before `TransformSystems::Propagate`;
//! [`write_batch_run_transforms`] copies propagated label transforms into run
//! records after it; [`update_batch_bounds`] hand-writes each batch entity's
//! `Aabb` and sort translation between `CalculateBounds` and
//! `CheckVisibility`; [`commit_batch_buffers`] uploads dirty record buffers
//! last, so extraction sees this frame's data.

use std::time::Instant;

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::system::SystemParam;
use bevy::light::NotShadowCaster;
use bevy::math::Vec3A;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::PanelTextLayout;
use super::PreparedPanelText;
use crate::cascade::CascadeDefault;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::cascade::TextDrawLayer;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphLighting;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render;
use crate::render::BatchGpu;
use crate::render::BatchKey;
use crate::render::BatchRenderLayers;
use crate::render::BatchTextMaterialInput;
use crate::render::GlyphAtlasHandles;
use crate::render::GlyphInstanceRecord;
use crate::render::RenderMode;
use crate::render::RunRecord;
use crate::render::TextAntiAlias;
use crate::render::TextMaterial;
use crate::render::constants;
use crate::render::constants::DrawOrdinal;
use crate::render::world_text::TextContent;
use crate::text;
use crate::text::GlyphCache;
use crate::text::PreparedTextRun;
use crate::text::RunStorageKey;

/// Marker on every batch render entity, BRP-inspectable.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct DiegeticTextBatch;

/// Builds changed runs' glyph records, routes them through the batch store,
/// and reconciles batch entities and GPU assets to the store's state (spawn
/// on a key's first run, despawn on its last, mesh growth on a capacity
/// crossing — created, written, and swapped in the same frame, D4).
///
/// Cascade inputs feeding [`BatchKey`] fields: each run's resolved
/// alpha / lighting / sidedness / draw layer (with the global defaults for
/// runs the cascade has not seeded) plus the changed-this-frame run set that
/// triggers re-routing.
#[derive(SystemParam)]
pub(super) struct BatchKeyCascades<'w, 's> {
    alphas:             Query<'w, 's, &'static Resolved<TextAlpha>, With<TextContent>>,
    lightings:          Query<'w, 's, &'static Resolved<TextLighting>, With<TextContent>>,
    sidednesses:        Query<'w, 's, &'static Resolved<TextSidedness>, With<TextContent>>,
    anti_aliases:       Query<'w, 's, &'static Resolved<TextAntiAlias>, With<TextContent>>,
    draw_layers:        Query<'w, 's, &'static Resolved<TextDrawLayer>, With<TextContent>>,
    alpha_default:      Res<'w, CascadeDefault<TextAlpha>>,
    lighting_default:   Res<'w, CascadeDefault<TextLighting>>,
    sidedness_default:  Res<'w, CascadeDefault<TextSidedness>>,
    anti_alias_default: Res<'w, CascadeDefault<TextAntiAlias>>,
    draw_layer_default: Res<'w, CascadeDefault<TextDrawLayer>>,
    changed: Query<
        'w,
        's,
        Entity,
        (
            With<TextContent>,
            With<PreparedPanelText>,
            Or<(
                Changed<Resolved<TextAlpha>>,
                Changed<Resolved<TextLighting>>,
                Changed<Resolved<TextSidedness>>,
                Changed<Resolved<TextAntiAlias>>,
                Changed<Resolved<TextDrawLayer>>,
            )>,
        ),
    >,
}

impl BatchKeyCascades<'_, '_> {
    /// Runs whose resolved cascade value transitioned this frame. The
    /// propagation pass is inequality-guarded, so membership means a real
    /// transition.
    fn changed_set(&self) -> EntityHashSet { self.changed.iter().collect() }

    fn alpha(&self, label: Entity) -> AlphaMode {
        self.alphas
            .get(label)
            .map_or(self.alpha_default.0.0, |resolved| resolved.0.0)
    }

    fn lighting(&self, label: Entity) -> GlyphLighting {
        self.lightings
            .get(label)
            .map_or(self.lighting_default.0.0, |resolved| resolved.0.0)
    }

    fn sidedness(&self, label: Entity) -> GlyphSidedness {
        self.sidednesses
            .get(label)
            .map_or(self.sidedness_default.0.0, |resolved| resolved.0.0)
    }

    fn anti_alias(&self, label: Entity) -> TextAntiAlias {
        self.anti_aliases
            .get(label)
            .map_or(self.anti_alias_default.0, |resolved| resolved.0)
    }

    fn draw_layer(&self, label: Entity) -> TextDrawLayer {
        self.draw_layers
            .get(label)
            .map_or(self.draw_layer_default.0, |resolved| resolved.0)
    }
}

/// The query walks every run but touches only those whose text changed, whose
/// resolved cascade value changed (alpha / lighting / sidedness / draw layer
/// are batch-key fields, so the run re-routes through `upsert_run` and moves
/// batches when the key differs), or that are not yet routed, so the system is
/// self-healing: a skipped frame (e.g. a glyph not yet packed) re-routes on
/// the next pass.
pub(super) fn update_panel_text_batches(
    runs: Query<
        (
            Entity,
            Ref<PreparedPanelText>,
            &PanelTextLayout,
            &ChildOf,
            &GlobalTransform,
            Option<&Visibility>,
        ),
        With<TextContent>,
    >,
    mut emptied_runs: RemovedComponents<PreparedPanelText>,
    panels: Query<(&DiegeticPanel, Option<&RenderLayers>, Option<&Visibility>)>,
    cascades: BatchKeyCascades,
    anti_alias: Res<TextAntiAlias>,
    mut backend: ResMut<GlyphCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let mesh_build_start = Instant::now();

    // R10 analogue: an emptied or despawned run leaves its batch; the rebuild
    // re-derives the survivors' ranges.
    for label_entity in emptied_runs.read() {
        backend
            .batch_store_mut()
            .remove_run(RunStorageKey::from(label_entity));
    }

    // Cascade changes are inequality-guarded at the propagation pass, so this
    // set holds only real transitions; membership re-routes the run below.
    let cascade_changed = cascades.changed_set();

    // Upload the shared glyph atlas once before the run loop — records only
    // index the atlas.
    let any_work = !cascade_changed.is_empty()
        || runs.iter().any(|(label_entity, prepared, ..)| {
            prepared.is_changed()
                || !backend
                    .batch_store()
                    .is_routed(RunStorageKey::from(label_entity))
        });
    let atlas = if any_work {
        backend.commit_glyph_atlas(&mut storage_buffers, &mut materials)
    } else {
        None
    };

    for (label_entity, prepared, panel_text_child, child_of, label_transform, label_visibility) in
        &runs
    {
        let storage_key = RunStorageKey::from(label_entity);
        let Ok((panel, panel_layers, panel_visibility)) = panels.get(child_of.parent()) else {
            backend.batch_store_mut().remove_run(storage_key);
            continue;
        };
        if is_hidden(label_visibility) || is_hidden(panel_visibility) {
            backend.batch_store_mut().remove_run(storage_key);
            continue;
        }
        if !prepared.is_changed()
            && !cascade_changed.contains(&label_entity)
            && backend.batch_store().is_routed(storage_key)
        {
            continue;
        }

        // A glyph missing from the atlas means shaping hasn't packed it yet;
        // the run stays unrouted and self-heals next frame.
        let Some(glyphs) = build_glyph_records(&backend, &prepared.prepared, prepared.clip_rect)
        else {
            continue;
        };
        if glyphs.is_empty() {
            // Clipping removed every quad: drop the run so nothing renders.
            backend.batch_store_mut().remove_run(storage_key);
            continue;
        }

        let base = panel
            .text_material()
            .cloned()
            .unwrap_or_else(constants::default_panel_material);
        let base_material = backend.batch_store_mut().intern_base_material(&base);
        let draw_layer = cascades.draw_layer(label_entity);
        let draw_ordinal = DrawOrdinal::from(draw_layer);
        let key = BatchKey {
            base_material,
            alpha: cascades.alpha(label_entity).into(),
            lighting: cascades.lighting(label_entity),
            sidedness: cascades.sidedness(label_entity),
            layer: draw_layer.0,
            shadow: prepared.shadow_mode,
            layers: BatchRenderLayers(panel_layers.cloned().unwrap_or(RenderLayers::layer(0))),
        };

        let fill_color = LinearRgba::from(prepared.fill_color);
        let record = RunRecord {
            // Pre-propagation snapshot; write_batch_run_transforms corrects it
            // after TransformSystems::Propagate the same frame.
            transform:        label_transform.to_matrix(),
            fill_color:       Vec4::new(
                fill_color.red,
                fill_color.green,
                fill_color.blue,
                fill_color.alpha,
            ),
            render_mode:      u32::from(RenderMode::from(prepared.render_mode)),
            // The recorded slot is the one the next geometry command occupies,
            // so the run already sits one layer above everything emitted
            // before it.
            depth_nudge:      panel_text_child.draw_slot.to_f32() * constants::LAYER_DEPTH_BIAS,
            oit_depth_offset: draw_ordinal.oit_depth_offset(),
            aa_flags:         cascades.anti_alias(label_entity).aa_flags(),
        };
        backend
            .batch_store_mut()
            .upsert_run(key, storage_key, glyphs, record);
    }

    reconcile_batch_entities(ReconcileBatchEntities {
        atlas:           atlas.as_ref(),
        anti_alias:      *anti_alias,
        backend:         &mut backend,
        meshes:          &mut meshes,
        materials:       &mut materials,
        storage_buffers: &mut storage_buffers,
        commands:        &mut commands,
    });

    perf.panel_text.mesh_build_ms =
        mesh_build_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

const fn is_hidden(visibility: Option<&Visibility>) -> bool {
    matches!(visibility, Some(Visibility::Hidden))
}

/// Inputs for [`reconcile_batch_entities`].
struct ReconcileBatchEntities<'a, 'w, 's> {
    atlas:           Option<&'a GlyphAtlasHandles>,
    anti_alias:      TextAntiAlias,
    backend:         &'a mut GlyphCache,
    meshes:          &'a mut Assets<Mesh>,
    materials:       &'a mut Assets<TextMaterial>,
    storage_buffers: &'a mut Assets<ShaderBuffer>,
    commands:        &'a mut Commands<'w, 's>,
}

/// Reconciles batch entities and GPU assets to the store's state: despawns
/// emptied batches, spawns entities + assets for new keys, and re-creates the
/// inert mesh on a capacity crossing.
fn reconcile_batch_entities(input: ReconcileBatchEntities<'_, '_, '_>) {
    let ReconcileBatchEntities {
        atlas,
        anti_alias,
        backend,
        meshes,
        materials,
        storage_buffers,
        commands,
    } = input;
    for entity in backend.batch_store_mut().take_empty_batches() {
        commands.entity(entity).despawn();
    }
    let mut to_create = Vec::new();
    let mut to_grow = Vec::new();
    for (key, batch) in backend.batch_store().batches() {
        match &batch.gpu {
            None => to_create.push(key.clone()),
            Some(gpu)
                if batch.glyph_record_count() > gpu.capacity
                    || batch.run_count().to_u32() > gpu.run_capacity =>
            {
                to_grow.push(key.clone());
            },
            Some(_) => {},
        }
    }
    if let Some(atlas) = atlas {
        for key in to_create {
            spawn_batch_entity(SpawnBatchEntity {
                key: &key,
                atlas,
                anti_alias,
                backend: &mut *backend,
                meshes: &mut *meshes,
                materials: &mut *materials,
                storage_buffers: &mut *storage_buffers,
                commands: &mut *commands,
            });
        }
    }
    for key in to_grow {
        grow_batch_assets(&key, backend, meshes, materials, storage_buffers, commands);
    }
}

/// Copies each routed label's propagated `GlobalTransform` into its
/// `RunRecord` slot. The store dirties the run table only when the matrix
/// actually changed, so a static frame uploads nothing.
pub(super) fn write_batch_run_transforms(
    labels: Query<(Entity, Ref<GlobalTransform>), (With<TextContent>, With<PreparedPanelText>)>,
    mut backend: ResMut<GlyphCache>,
) {
    for (label_entity, transform) in &labels {
        if !transform.is_changed() {
            continue;
        }
        backend
            .batch_store_mut()
            .update_run_transform(RunStorageKey::from(label_entity), transform.to_matrix());
    }
}

/// Hand-writes each dirty batch's bounds: the entity's translation moves to
/// the union's world center (the `Transparent3d` sort distance) and its
/// local-space `Aabb` gets the union's half-extents. The batch entity carries
/// `NoAutoAabb` — `CalculateBounds` would otherwise install a zero-extent box
/// computed from the inert mesh's zeroed positions on every growth frame.
pub(super) fn update_batch_bounds(
    mut backend: ResMut<GlyphCache>,
    mut batch_entities: Query<
        (&mut Transform, &mut GlobalTransform, &mut Aabb),
        With<DiegeticTextBatch>,
    >,
) {
    for (_, batch) in backend.batch_store_mut().batches_mut() {
        if !batch.bounds_dirty {
            continue;
        }
        let Some(entity) = batch.entity else {
            continue;
        };
        let Ok((mut transform, mut global, mut aabb)) = batch_entities.get_mut(entity) else {
            continue;
        };
        let Some((min, max)) = batch.world_bounds() else {
            continue;
        };
        let center = (min + max) * 0.5;
        // The batch entity has no parent and propagation already ran, so the
        // pair is written directly; the vertex shader ignores it (placement
        // comes from run records) — this is sort/culling metadata only.
        *transform = Transform::from_translation(center);
        *global = GlobalTransform::from(*transform);
        *aabb = Aabb {
            center:       Vec3A::ZERO,
            half_extents: Vec3A::from((max - min) * 0.5),
        };
        batch.bounds_dirty = false;
    }
}

/// Uploads each batch's dirty record buffers — one `set_data` per dirty
/// buffer per batch — and publishes the batch counters. The split dirty flags
/// keep the uploads minimal: a transform-only frame uploads only the run
/// table, a same-count text edit only the instance buffer, an unchanged frame
/// nothing (the Phase D property).
///
/// Every payload is padded to the buffer's capacity so its byte length never
/// changes between growths — a constant-length `set_data` writes the existing
/// wgpu buffer in place, which the material's bind group observes without a
/// re-prepare (see [`BatchGpu`](crate::render::BatchGpu)).
pub(super) fn commit_batch_buffers(
    mut backend: ResMut<GlyphCache>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    let mut batches = 0_usize;
    let mut runs = 0_usize;
    let mut glyph_records = 0_usize;
    let mut instance_uploads = 0_usize;
    let mut run_table_uploads = 0_usize;
    for (_, batch) in backend.batch_store_mut().batches_mut() {
        batches += 1;
        runs += batch.run_count();
        glyph_records += batch.glyph_record_count().to_usize();
        if batch.gpu.is_none() {
            continue;
        }
        let instances_payload = batch.instances_dirty.then(|| {
            let capacity = batch.gpu.as_ref().map_or(0, |gpu| gpu.capacity);
            padded_glyph_records(batch.glyph_records(), capacity)
        });
        let run_table_payload = batch.run_table_dirty.then(|| {
            let run_capacity = batch.gpu.as_ref().map_or(0, |gpu| gpu.run_capacity);
            padded_run_records(batch.run_records(), run_capacity)
        });
        batch.instances_dirty = false;
        batch.run_table_dirty = false;
        let Some(gpu) = &batch.gpu else {
            continue;
        };
        if let Some(data) = instances_payload
            && let Some(mut buffer) = storage_buffers.get_mut(&gpu.instances)
        {
            buffer.set_data(data);
            instance_uploads += 1;
        }
        if let Some(data) = run_table_payload
            && let Some(mut buffer) = storage_buffers.get_mut(&gpu.run_table)
        {
            buffer.set_data(data);
            run_table_uploads += 1;
        }
    }
    perf.batch.batches = batches;
    perf.batch.runs = runs;
    perf.batch.glyph_records = glyph_records;
    perf.batch.instance_uploads = instance_uploads;
    perf.batch.run_table_uploads = run_table_uploads;
}

/// Glyph records padded to `capacity` with zero-size quads: every corner of a
/// padding quad coincides, so it rasterizes nothing, and the buffer's byte
/// length stays constant between growths.
fn padded_glyph_records(
    records: &[GlyphInstanceRecord],
    capacity: u32,
) -> Vec<GlyphInstanceRecord> {
    let mut padded = Vec::with_capacity(capacity.to_usize());
    padded.extend_from_slice(records);
    padded.resize(
        capacity.to_usize().max(records.len()),
        GlyphInstanceRecord {
            rect_min:    Vec2::ZERO,
            rect_size:   Vec2::ZERO,
            uv_min:      Vec2::ZERO,
            uv_size:     Vec2::ZERO,
            atlas_index: 0,
            run_index:   0,
        },
    );
    padded
}

/// Run records padded to `run_capacity`. Padding slots are never referenced —
/// no live glyph record carries their index, and zero-size padding quads
/// produce no fragments — so every field can be zero (`render_mode` 0 is
/// deliberately neither `Text` nor `PunchOut`).
fn padded_run_records(records: &[RunRecord], run_capacity: u32) -> Vec<RunRecord> {
    let mut padded = Vec::with_capacity(run_capacity.to_usize());
    padded.extend_from_slice(records);
    padded.resize(
        run_capacity.to_usize().max(records.len()),
        RunRecord {
            transform:        Mat4::ZERO,
            fill_color:       Vec4::ZERO,
            render_mode:      0,
            depth_nudge:      0.0,
            oit_depth_offset: 0.0,
            aa_flags:         0,
        },
    );
    padded
}

/// Builds one run's glyph instance records against the shared atlas, with
/// each quad's padded rect and UVs clipped by `glyph_quad_extents`.
/// `run_index` is `0` on every record — the batch store stamps it at rebuild.
/// Returns `None` when a glyph is not yet packed.
pub(crate) fn build_glyph_records(
    cache: &GlyphCache,
    prepared: &PreparedTextRun,
    clip_rect: Option<[f32; 4]>,
) -> Option<Vec<GlyphInstanceRecord>> {
    let mut records = Vec::with_capacity(prepared.glyph_count());
    for glyph in prepared.glyphs() {
        let atlas_index = cache.atlas_index(glyph.key())?;
        let Some(extents) = text::glyph_quad_extents(*glyph, 1.0, clip_rect) else {
            continue;
        };
        records.push(GlyphInstanceRecord {
            rect_min: Vec2::new(extents.left, extents.bottom),
            rect_size: Vec2::new(extents.right - extents.left, extents.top - extents.bottom),
            uv_min: Vec2::new(extents.uv_left, extents.uv_top),
            uv_size: Vec2::new(
                extents.uv_right - extents.uv_left,
                extents.uv_bottom - extents.uv_top,
            ),
            atlas_index,
            run_index: 0,
        });
    }
    Some(records)
}

/// Inert capacity-sized batch mesh: zeroed `POSITION` / `UV_0` / `UV_1`
/// (values unread — the layout switches the `VERTEX_UVS_A/B` pipeline defs
/// on) plus the static per-quad `U32` index pattern winding each quad
/// `base, base+3, base+2, base, base+2, base+1`.
pub(crate) fn inert_batch_mesh(capacity: u32) -> Mesh {
    let vertex_count = capacity.to_usize() * 4;
    let mut indices = Vec::with_capacity(capacity.to_usize() * 6);
    for quad in 0..capacity {
        let base = quad * 4;
        indices.extend([base, base + 3, base + 2, base, base + 2, base + 1]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; vertex_count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; vertex_count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0_f32; 2]; vertex_count]);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Inputs for [`spawn_batch_entity`].
struct SpawnBatchEntity<'a, 'w, 's> {
    key:             &'a BatchKey,
    atlas:           &'a GlyphAtlasHandles,
    anti_alias:      TextAntiAlias,
    backend:         &'a mut GlyphCache,
    meshes:          &'a mut Assets<Mesh>,
    materials:       &'a mut Assets<TextMaterial>,
    storage_buffers: &'a mut Assets<ShaderBuffer>,
    commands:        &'a mut Commands<'w, 's>,
}

/// Spawns one batch's render entity with its GPU assets: record buffers
/// created from the current store contents, an inert mesh at the rounded-up
/// capacity, and the per-batch vertex-pulling material.
fn spawn_batch_entity(input: SpawnBatchEntity<'_, '_, '_>) {
    let SpawnBatchEntity {
        key,
        atlas,
        anti_alias,
        backend,
        meshes,
        materials,
        storage_buffers,
        commands,
    } = input;
    let Some(batch) = backend.batch_store().get(key) else {
        return;
    };
    let capacity = batch.glyph_record_count().next_power_of_two();
    let run_capacity = batch.run_count().to_u32().next_power_of_two();
    let glyph_records = padded_glyph_records(batch.glyph_records(), capacity);
    let run_records = padded_run_records(batch.run_records(), run_capacity);
    let base = backend
        .batch_store()
        .base_material(key.base_material)
        .clone();

    let instances = storage_buffers.add(ShaderBuffer::from(glyph_records));
    let run_table = storage_buffers.add(ShaderBuffer::from(run_records));
    let mesh = meshes.add(inert_batch_mesh(capacity));
    let material = materials.add(batch_material(BatchMaterialInput {
        base,
        key,
        atlas,
        instances: instances.clone(),
        run_table: run_table.clone(),
        anti_alias,
    }));

    let mut batch_entity = commands.spawn((
        DiegeticTextBatch,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        // The union system owns this Aabb; CalculateBounds must not replace
        // it with a zero-extent box from the inert mesh's zeroed positions.
        NoAutoAabb,
        Aabb::default(),
        key.layers.0.clone(),
    ));
    if key.shadow == GlyphShadowMode::None {
        batch_entity.insert(NotShadowCaster);
    }
    let entity = batch_entity.id();

    if let Some(batch) = backend.batch_store_mut().get_mut(key) {
        batch.entity = Some(entity);
        batch.gpu = Some(BatchGpu {
            instances,
            run_table,
            mesh,
            material,
            capacity,
            run_capacity,
        });
        // The buffers were created from the current records; only later
        // writes (e.g. this frame's post-propagation transform pass) need an
        // upload. bounds_dirty stays set so the union system places the
        // entity this frame.
        batch.instances_dirty = false;
        batch.run_table_dirty = false;
    }
}

/// Grows a batch past its capacity: new padded record buffers and a new inert
/// mesh at the doubled capacities, the mesh swapped onto the entity and the
/// material's buffer handles rewritten in place. The mesh draws the same
/// frame (D4); the rewritten material re-prepares against the new buffers —
/// at worst one frame later (a missing render asset retries), during which
/// the old buffers keep drawing the pre-growth content. No blink either way.
fn grow_batch_assets(
    key: &BatchKey,
    backend: &mut GlyphCache,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TextMaterial>,
    storage_buffers: &mut Assets<ShaderBuffer>,
    commands: &mut Commands,
) {
    let Some(batch) = backend.batch_store_mut().get_mut(key) else {
        return;
    };
    let Some(entity) = batch.entity else {
        return;
    };
    let required = batch.glyph_record_count();
    let run_required = batch.run_count().to_u32();
    let (Some(current_capacity), Some(current_run_capacity)) = (
        batch.gpu.as_ref().map(|gpu| gpu.capacity),
        batch.gpu.as_ref().map(|gpu| gpu.run_capacity),
    ) else {
        return;
    };
    let mut capacity = current_capacity.max(1);
    while capacity < required {
        capacity *= 2;
    }
    let mut run_capacity = current_run_capacity.max(1);
    while run_capacity < run_required {
        run_capacity *= 2;
    }

    let instances = storage_buffers.add(ShaderBuffer::from(padded_glyph_records(
        batch.glyph_records(),
        capacity,
    )));
    let run_table = storage_buffers.add(ShaderBuffer::from(padded_run_records(
        batch.run_records(),
        run_capacity,
    )));
    let mesh = meshes.add(inert_batch_mesh(capacity));
    commands.entity(entity).insert(Mesh3d(mesh.clone()));

    let Some(gpu) = &mut batch.gpu else {
        return;
    };
    if let Some(mut material) = materials.get_mut(&gpu.material) {
        render::set_batch_text_material_buffers(
            &mut material,
            instances.clone(),
            run_table.clone(),
        );
    }
    gpu.instances = instances;
    gpu.run_table = run_table;
    gpu.mesh = mesh;
    gpu.capacity = capacity;
    gpu.run_capacity = run_capacity;
    // The new buffers were created from the current records.
    batch.instances_dirty = false;
    batch.run_table_dirty = false;
}

/// Inputs for [`batch_material`].
struct BatchMaterialInput<'a> {
    base:       StandardMaterial,
    key:        &'a BatchKey,
    atlas:      &'a GlyphAtlasHandles,
    instances:  Handle<ShaderBuffer>,
    run_table:  Handle<ShaderBuffer>,
    anti_alias: TextAntiAlias,
}

/// Builds one batch's material: the interned base with the key's cascade
/// values applied, the shared atlas buffers, the batch's record buffers, and
/// the vertex-pulling route switched on. `fill_color` / `render_mode` in the
/// uniform are placeholders — the fragment reads them per run from the run
/// table under `GLYPH_VERTEX_PULL`.
fn batch_material(input: BatchMaterialInput<'_>) -> TextMaterial {
    let BatchMaterialInput {
        mut base,
        key,
        atlas,
        instances,
        run_table,
        anti_alias,
    } = input;
    base.alpha_mode = batch_gpu_alpha_mode(key.alpha.into());
    base.unlit = matches!(key.lighting, GlyphLighting::Unlit);
    constants::apply_glyph_sidedness(&mut base, key.sidedness);
    // One bias/offset pair for the whole batch, derived from the key's draw
    // layer: the bias orders the batch among backing commands on sorted
    // (non-OIT) views, the OIT offset among them on OIT views (clamped at
    // `0.0` from the default layer up, so opaque world geometry keeps depth
    // authority). Per-run order inside the batch comes from the per-record
    // depth nudge, which a per-material bias cannot express.
    let text_ordinal = DrawOrdinal::from(TextDrawLayer(key.layer));
    base.depth_bias = text_ordinal.depth_bias();
    render::batch_text_material(BatchTextMaterialInput {
        base,
        fill_color: Vec4::ONE,
        render_mode: RenderMode::Text,
        oit_depth_offset: text_ordinal.oit_depth_offset(),
        supersample: anti_alias.supersamples(),
        aa_band: anti_alias.anisotropic(),
        curves: atlas.curves.clone(),
        bands: atlas.bands.clone(),
        glyphs: atlas.glyphs.clone(),
        instances,
        run_records: run_table,
        debug_glyph_index: false,
    })
}

/// Maps a batch's authored alpha mode to the one written on the GPU material.
/// Invariant: [`BatchKey::alpha`] always keeps the user's authored mode — only
/// the material differs, and only where bevy's pipeline routing requires it.
///
/// `Opaque` becomes `Mask(0.0)`: opaque casters take bevy's depth-only shadow
/// pipelines, which strip the material bind group from the pipeline layout —
/// and the vertex-pull stage reads bindings 104/105 from that group, so
/// pipeline creation fails wgpu validation. `Mask(0.0)` renders the same
/// pixels (cutoff 0 never discards by alpha; the coverage discards cut the
/// glyph outlines; depth writes, nothing blends) and its `MAY_DISCARD`
/// pipelines keep the material bind group.
const fn batch_gpu_alpha_mode(authored: AlphaMode) -> AlphaMode {
    match authored {
        AlphaMode::Opaque => AlphaMode::Mask(0.0),
        other => other,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should fail loudly when fixture batches are missing"
)]
mod tests {
    use std::sync::Arc;

    use bevy::asset::AssetPlugin;
    use bevy::prelude::*;

    use super::*;
    use crate::Mm;
    use crate::cascade::CascadeEntityCommandsExt;
    use crate::cascade::CascadePlugin;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::panel_text::alpha;
    use crate::render::panel_text::glyph_cascade;
    use crate::render::panel_text::reconcile;
    use crate::render::panel_text::shaping;
    use crate::render::text_shaping::TextShapingContext;
    use crate::text::DiegeticTextMeasurer;
    use crate::text::FontRegistry;

    fn monospace_measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|text: &str, measure: &TextMeasure| {
                let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
                let width = text
                    .lines()
                    .map(|line| line.chars().count().to_f32() * char_width)
                    .fold(0.0_f32, f32::max);
                let line_count = text.lines().count().max(1).to_f32();
                TextDimensions {
                    width,
                    height: measure.size * line_count,
                    line_height: measure.size,
                }
            }),
        }
    }

    /// App with the full panel-text pipeline — the production registration's
    /// ordering, minus the visibility-set edges (headless: no visibility
    /// systems run).
    fn pipeline_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .add_plugins(TransformPlugin)
            .insert_resource(monospace_measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_plugins(CascadePlugin::<TextLighting>::default())
            .add_plugins(CascadePlugin::<TextSidedness>::default())
            .add_plugins(CascadePlugin::<TextDrawLayer>::default())
            .insert_resource(FontRegistry::new().expect("embedded font should parse"))
            .init_resource::<TextShapingContext>()
            .init_resource::<GlyphCache>()
            .init_resource::<TextAntiAlias>()
            .init_asset::<Mesh>()
            .init_asset::<ShaderBuffer>()
            .init_asset::<TextMaterial>()
            .add_observer(alpha::seed_panel_text_child_alpha)
            .add_observer(glyph_cascade::seed_panel_text_child_glyph)
            .add_systems(
                PostUpdate,
                (
                    reconcile::reconcile_panel_text_children,
                    shaping::shape_panel_text_children
                        .after(reconcile::reconcile_panel_text_children),
                    update_panel_text_batches
                        .after(shaping::shape_panel_text_children)
                        .before(TransformSystems::Propagate),
                    write_batch_run_transforms.after(TransformSystems::Propagate),
                    update_batch_bounds.after(write_batch_run_transforms),
                    commit_batch_buffers
                        .after(update_panel_text_batches)
                        .after(write_batch_run_transforms),
                ),
            );
        app
    }

    fn two_text_tree() -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text("Alpha", TextStyle::new(10.0));
        builder.text("Beta", TextStyle::new(10.0));
        builder.build()
    }

    fn spawn_panel(app: &mut App, tree: LayoutTree) -> Entity {
        app.world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(tree)
                    .build()
                    .expect("panel should build"),
            )
            .id()
    }

    fn settle(app: &mut App) {
        for _ in 0..4 {
            app.update();
        }
    }

    fn batch_entities(app: &mut App) -> Vec<Entity> {
        let mut state = app
            .world_mut()
            .query_filtered::<Entity, With<DiegeticTextBatch>>();
        state.iter(app.world()).collect()
    }

    fn label_entities(app: &mut App) -> Vec<Entity> {
        let mut state = app
            .world_mut()
            .query_filtered::<Entity, With<TextContent>>();
        state.iter(app.world()).collect()
    }

    fn store_stats(app: &App) -> (usize, usize, usize) {
        let store = app.world().resource::<GlyphCache>().batch_store();
        let batches = store.batches().count();
        let runs: usize = store.batches().map(|(_, batch)| batch.run_count()).sum();
        let glyphs: usize = store
            .batches()
            .map(|(_, batch)| batch.glyph_record_count().to_usize())
            .sum();
        (batches, runs, glyphs)
    }

    #[test]
    fn runs_route_into_one_batch_entity() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, two_text_tree());
        settle(&mut app);

        let (batches, runs, glyphs) = store_stats(&app);
        assert_eq!(batches, 1, "two same-key runs share one batch");
        assert_eq!(runs, 2);
        assert_eq!(glyphs, 9, "'Alpha' + 'Beta' visible glyphs");
        assert_eq!(batch_entities(&mut app).len(), 1);
    }

    #[test]
    fn batch_entity_gets_real_bounds_and_sort_translation() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, two_text_tree());
        settle(&mut app);

        let entity = batch_entities(&mut app)[0];
        let aabb = *app
            .world()
            .get::<Aabb>(entity)
            .expect("batch entity should carry a hand-written Aabb");
        assert!(
            aabb.half_extents.length_squared() > 0.0,
            "the union Aabb has real extents, not the inert mesh's zeros"
        );
        assert_eq!(aabb.center, Vec3A::ZERO, "the Aabb is local-space");
        assert!(
            app.world().get::<NoAutoAabb>(entity).is_some(),
            "CalculateBounds must not replace the union Aabb"
        );
    }

    #[test]
    fn run_record_transform_matches_propagated_label_transform() {
        let mut app = pipeline_app();
        let panel = app
            .world_mut()
            .spawn((
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(two_text_tree())
                    .build()
                    .expect("panel should build"),
                Transform::from_xyz(3.0, -2.0, 1.0),
            ))
            .id();
        settle(&mut app);

        let panel_translation = app
            .world()
            .get::<GlobalTransform>(panel)
            .expect("panel should have a GlobalTransform")
            .translation();
        assert_eq!(panel_translation, Vec3::new(3.0, -2.0, 1.0));
        let store = app.world().resource::<GlyphCache>().batch_store();
        let (_, batch) = store.batches().next().expect("one batch should exist");
        for record in batch.run_records() {
            // Labels sit at panel-local offsets, so only the propagated panel
            // translation is asserted exactly: the records' w-axis carries it.
            assert!(
                (record.transform.w_axis.z - 1.0).abs() < f32::EPSILON,
                "run record transform should carry the propagated panel z"
            );
        }
    }

    #[test]
    fn despawning_the_panel_despawns_the_emptied_batch() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree());
        settle(&mut app);
        assert_eq!(batch_entities(&mut app).len(), 1);

        app.world_mut().entity_mut(panel).despawn();
        settle(&mut app);

        assert_eq!(store_stats(&app), (0, 0, 0));
        assert_eq!(
            batch_entities(&mut app).len(),
            0,
            "the last run leaving despawns the batch entity"
        );
    }

    #[test]
    fn hidden_panel_routes_no_batched_runs_until_visible() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree());
        app.world_mut().entity_mut(panel).insert(Visibility::Hidden);
        settle(&mut app);

        assert_eq!(
            store_stats(&app),
            (0, 0, 0),
            "a hidden panel's text should not enter the batch store"
        );
        assert!(batch_entities(&mut app).is_empty());

        app.world_mut()
            .entity_mut(panel)
            .insert(Visibility::Inherited);
        settle(&mut app);
        assert_eq!(
            store_stats(&app),
            (1, 2, 9),
            "restoring inherited visibility routes the existing text runs"
        );
        assert_eq!(batch_entities(&mut app).len(), 1);

        app.world_mut().entity_mut(panel).insert(Visibility::Hidden);
        settle(&mut app);
        assert_eq!(
            store_stats(&app),
            (0, 0, 0),
            "hiding the panel again removes its text from the batch store"
        );
        assert!(batch_entities(&mut app).is_empty());
    }

    #[test]
    fn text_edit_rewrites_records_in_the_same_batch() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree());
        settle(&mut app);
        let entity_before = batch_entities(&mut app)[0];
        let (_, _, glyphs_before) = store_stats(&app);

        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text("Alphas", TextStyle::new(10.0));
        builder.text("Beta", TextStyle::new(10.0));
        app.world_mut().commands().set_tree(panel, builder.build());
        settle(&mut app);

        let (batches, runs, glyphs) = store_stats(&app);
        assert_eq!(batches, 1);
        assert_eq!(runs, 2);
        assert_eq!(glyphs, glyphs_before + 1, "'Alphas' grew by one glyph");
        assert_eq!(
            batch_entities(&mut app)[0],
            entity_before,
            "a text edit reuses the batch entity"
        );
    }

    #[test]
    fn alpha_cascade_change_moves_the_run_to_the_new_keys_batch() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, two_text_tree());
        settle(&mut app);
        assert_eq!(store_stats(&app), (1, 2, 9));

        let label = label_entities(&mut app)[0];
        app.world_mut()
            .commands()
            .entity(label)
            .override_text_alpha(AlphaMode::Opaque);
        settle(&mut app);

        let (batches, runs, glyphs) = store_stats(&app);
        assert_eq!(
            batches, 2,
            "the alpha-changed run re-keys into its own batch"
        );
        assert_eq!(runs, 2);
        assert_eq!(glyphs, 9, "no records were lost in the move");
        assert_eq!(batch_entities(&mut app).len(), 2);
        let store = app.world().resource::<GlyphCache>().batch_store();
        let opaque_runs: usize = store
            .batches()
            .filter(|(key, _)| key.alpha == AlphaMode::Opaque.into())
            .map(|(_, batch)| batch.run_count())
            .sum();
        assert_eq!(
            opaque_runs, 1,
            "exactly the overridden run is keyed under the new key"
        );
    }

    /// Per-batch (`depth_bias`, OIT depth offset) read off the single live
    /// batch's material asset.
    fn batch_material_values(app: &App) -> (f32, f32) {
        let store = app.world().resource::<GlyphCache>().batch_store();
        let (_, batch) = store.batches().next().expect("one batch should exist");
        let gpu = batch.gpu.as_ref().expect("batch should have GPU assets");
        let material = app
            .world()
            .resource::<Assets<TextMaterial>>()
            .get(&gpu.material)
            .expect("batch material asset should exist");
        (
            material.base.depth_bias,
            text::text_material_oit_depth_offset(material),
        )
    }

    fn batch_layers(app: &App) -> Vec<i8> {
        let store = app.world().resource::<GlyphCache>().batch_store();
        let mut layers: Vec<i8> = store.batches().map(|(key, _)| key.layer).collect();
        layers.sort_unstable();
        layers
    }

    #[test]
    fn runs_with_distinct_draw_layers_route_to_distinct_batches() {
        let mut app = pipeline_app();
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            "Alpha",
            TextStyle::new(10.0).with_draw_layer(TextDrawLayer(10)),
        );
        builder.text(
            "Beta",
            TextStyle::new(10.0).with_draw_layer(TextDrawLayer(10)),
        );
        builder.text(
            "Gamma",
            TextStyle::new(10.0).with_draw_layer(TextDrawLayer(-3)),
        );
        builder.text("Delta", TextStyle::new(10.0));
        spawn_panel(&mut app, builder.build());
        settle(&mut app);

        let (batches, runs, _) = store_stats(&app);
        assert_eq!(batches, 3, "three distinct layers split three batches");
        assert_eq!(runs, 4);
        assert_eq!(batch_layers(&app), vec![-3, 10, 64]);
        assert_eq!(batch_entities(&mut app).len(), 3);
        let store = app.world().resource::<GlyphCache>().batch_store();
        let shared_layer_runs: usize = store
            .batches()
            .filter(|(key, _)| key.layer == 10)
            .map(|(_, batch)| batch.run_count())
            .sum();
        assert_eq!(shared_layer_runs, 2, "same-layer runs share one batch");
    }

    #[test]
    fn label_spawned_with_a_draw_layer_override_routes_to_the_override_batch() {
        let mut app = pipeline_app();
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            "Alpha",
            TextStyle::new(10.0).with_draw_layer(TextDrawLayer(10)),
        );
        spawn_panel(&mut app, builder.build());
        settle(&mut app);

        // The run was never routed before, so it took the unrouted-run path —
        // no `Changed<Resolved<TextDrawLayer>>` membership involved.
        assert_eq!(store_stats(&app), (1, 1, 5));
        assert_eq!(batch_layers(&app), vec![10]);
    }

    #[test]
    fn draw_layer_cascade_change_moves_the_run_and_reconciles_batch_entities() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, two_text_tree());
        settle(&mut app);
        assert_eq!(store_stats(&app), (1, 2, 9));
        let default_entity = batch_entities(&mut app)[0];

        let label = label_entities(&mut app)[0];
        app.world_mut()
            .commands()
            .entity(label)
            .override_text_draw_layer(TextDrawLayer(10));
        settle(&mut app);

        let (batches, runs, glyphs) = store_stats(&app);
        assert_eq!(
            batches, 2,
            "the layer-changed run re-keys into its own batch"
        );
        assert_eq!(runs, 2);
        assert_eq!(glyphs, 9, "no records were lost in the move");
        assert_eq!(batch_layers(&app), vec![10, 64]);
        let entities = batch_entities(&mut app);
        assert_eq!(entities.len(), 2, "the new key's batch entity spawned");
        assert!(entities.contains(&default_entity));

        app.world_mut()
            .commands()
            .entity(label)
            .inherit_text_draw_layer();
        settle(&mut app);

        assert_eq!(store_stats(&app), (1, 2, 9));
        assert_eq!(batch_layers(&app), vec![64]);
        assert_eq!(
            batch_entities(&mut app),
            vec![default_entity],
            "the emptied layer batch entity despawned; the default batch entity survived"
        );
    }

    #[test]
    fn default_layer_batch_material_reproduces_previous_values() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, two_text_tree());
        settle(&mut app);

        // Pre-DrawOrdinal constants: BATCH_TEXT_DEPTH_BIAS = 64.0 ×
        // LAYER_DEPTH_BIAS and a hard-coded 0.0 OIT offset.
        let (depth_bias, oit_depth_offset) = batch_material_values(&app);
        assert_eq!(depth_bias.to_bits(), 64.0f32.to_bits());
        assert_eq!(oit_depth_offset.to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn layer_below_a_backing_sorts_between_neighboring_commands() {
        let mut app = pipeline_app();
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            "Alpha",
            TextStyle::new(10.0).with_draw_layer(TextDrawLayer(5)),
        );
        spawn_panel(&mut app, builder.build());
        settle(&mut app);

        let (depth_bias, oit_depth_offset) = batch_material_values(&app);
        let lower_backing = DrawOrdinal::from_draw_slot(3);
        let higher_backing = DrawOrdinal::from_draw_slot(7);
        assert!(
            lower_backing.depth_bias() < depth_bias && depth_bias < higher_backing.depth_bias(),
            "the layer-5 batch sorts between slots 3 and 7 on the sorted axis"
        );
        assert!(
            lower_backing.oit_depth_offset() < oit_depth_offset
                && oit_depth_offset < higher_backing.oit_depth_offset(),
            "the layer-5 batch sorts between slots 3 and 7 on the OIT axis"
        );
    }

    /// Mirrors `text_alpha`'s `apply_state_and_rebuild_hud`: one frame both
    /// changes the global alpha default and despawns + respawns a panel whose
    /// alpha is pinned by a panel-level override. The respawned runs must key
    /// by the pin, not by the default that was active while they routed.
    #[test]
    fn respawned_pinned_panel_keys_by_its_override_not_the_default() {
        fn spawn_pinned_panel(app: &mut App) -> Entity {
            app.world_mut()
                .spawn(
                    DiegeticPanel::world()
                        .size(Mm(100.0), Mm(50.0))
                        .text_alpha_mode(AlphaMode::Blend)
                        .with_tree(two_text_tree())
                        .build()
                        .expect("panel should build"),
                )
                .id()
        }

        let mut app = pipeline_app();
        let panel = spawn_pinned_panel(&mut app);
        settle(&mut app);
        assert_eq!(store_stats(&app), (1, 2, 9));

        // The live sequence: default change + rebuild in the same frame.
        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Multiply);
        app.world_mut().entity_mut(panel).despawn();
        spawn_pinned_panel(&mut app);
        settle(&mut app);

        let (batches, runs, glyphs) = store_stats(&app);
        assert_eq!(runs, 2, "the respawned panel's runs are routed");
        assert_eq!(glyphs, 9);
        let store = app.world().resource::<GlyphCache>().batch_store();
        let keys: Vec<AlphaMode> = store.batches().map(|(key, _)| key.alpha.into()).collect();
        assert_eq!(
            (batches, keys.as_slice()),
            (1, &[AlphaMode::Blend][..]),
            "pinned runs key by the panel override, not the changed default"
        );
    }

    #[test]
    fn fill_color_edit_stays_in_batch_as_a_record_write() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree());
        settle(&mut app);
        let entity_before = batch_entities(&mut app)[0];

        // Same text, new color: a value-only run-record write, not a re-key.
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            "Alpha",
            TextStyle::new(10.0).with_color(Color::srgb(1.0, 0.0, 0.0)),
        );
        builder.text("Beta", TextStyle::new(10.0));
        app.world_mut().commands().set_tree(panel, builder.build());
        settle(&mut app);

        assert_eq!(
            store_stats(&app),
            (1, 2, 9),
            "one batch survives the recolor"
        );
        assert_eq!(
            batch_entities(&mut app)[0],
            entity_before,
            "a fill-color edit reuses the batch entity"
        );
        let store = app.world().resource::<GlyphCache>().batch_store();
        let (_, batch) = store.batches().next().expect("one batch should exist");
        assert!(
            batch
                .run_records()
                .iter()
                .any(|record| record.fill_color == Vec4::new(1.0, 0.0, 0.0, 1.0)),
            "the recolored run's record carries the new fill color"
        );
    }

    #[test]
    fn shadow_mode_change_rekeys_into_a_not_shadow_caster_batch() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree());
        settle(&mut app);
        assert_eq!(store_stats(&app).0, 1);

        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            "Alpha",
            TextStyle::new(10.0).with_shadow_mode(GlyphShadowMode::None),
        );
        builder.text("Beta", TextStyle::new(10.0));
        app.world_mut().commands().set_tree(panel, builder.build());
        settle(&mut app);

        let (batches, runs, _) = store_stats(&app);
        assert_eq!(batches, 2, "the shadow-mode run re-keys into its own batch");
        assert_eq!(runs, 2);
        let entities = batch_entities(&mut app);
        let non_casters = entities
            .iter()
            .filter(|&&entity| app.world().get::<NotShadowCaster>(entity).is_some())
            .count();
        assert_eq!(
            (non_casters, entities.len() - non_casters),
            (1, 1),
            "the GlyphShadowMode::None batch entity opts out of shadow casting"
        );
    }

    #[test]
    fn same_frame_cascade_move_and_despawn_leave_the_store_consistent() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree());
        settle(&mut app);
        let label = label_entities(&mut app)[0];

        // Same frame: the run's key changes AND its panel despawns. The
        // routing system must observe one consistent end state (decision-4/9
        // order independence).
        app.world_mut()
            .commands()
            .entity(label)
            .override_text_alpha(AlphaMode::Opaque);
        app.world_mut().commands().entity(panel).despawn();
        settle(&mut app);

        assert_eq!(store_stats(&app), (0, 0, 0));
        assert!(batch_entities(&mut app).is_empty());
        assert!(
            !app.world()
                .resource::<GlyphCache>()
                .batch_store()
                .is_routed(RunStorageKey::from(label)),
            "the despawned run left no membership behind"
        );
    }

    /// The `BatchGpu` doc contract: every commit payload is padded to the
    /// buffer's capacity, so its byte length never changes between growths —
    /// a constant-length `set_data` writes the existing wgpu buffer in place
    /// and live material bind groups keep reading it. A refactor that drops
    /// the padding fails here instead of at a parity screenshot.
    #[test]
    fn commit_payloads_keep_a_constant_length_between_growths() {
        let glyph = GlyphInstanceRecord {
            rect_min:    Vec2::ZERO,
            rect_size:   Vec2::ONE,
            uv_min:      Vec2::ZERO,
            uv_size:     Vec2::ONE,
            atlas_index: 0,
            run_index:   0,
        };
        let record = RunRecord {
            transform:        Mat4::IDENTITY,
            fill_color:       Vec4::ONE,
            render_mode:      1,
            depth_nudge:      0.0,
            oit_depth_offset: 0.0,
            aa_flags:         TextAntiAlias::Both.aa_flags(),
        };

        for count in 0..=8_usize {
            assert_eq!(
                padded_glyph_records(&vec![glyph; count], 8).len(),
                8,
                "glyph payload length must equal capacity at {count} records"
            );
            assert_eq!(
                padded_run_records(&vec![record; count], 8).len(),
                8,
                "run payload length must equal capacity at {count} records"
            );
        }
    }
}

//! Batched analytic-path rendering for panel-owned line primitives.
//!
//! Every visible resolved line primitive becomes one analytic path instance and
//! one run record. Compatible records from any number of panels share one batch
//! render entity, one inert quad mesh, one `TextMaterial`, and one path atlas.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::math::Vec2;
use bevy::math::Vec3;
use bevy::math::Vec3A;
use bevy::math::Vec4;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::path;
use super::path::PanelLinePathContext;
use super::primitive::PanelLineRenderKey;
use crate::layout::BoundingBox;
use crate::layout::PanelLinePaintOrder;
use crate::layout::PanelLineSourceKey;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::layout::ResolvedPanelLine;
use crate::layout::ResolvedPanelLinePrimitive;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render;
use crate::render::BaseMaterialId;
use crate::render::BatchAlphaMode;
use crate::render::BatchRenderLayers;
use crate::render::BatchTextMaterialInput;
use crate::render::GlyphAtlasHandles;
use crate::render::GlyphInstanceRecord;
use crate::render::PathAtlas;
use crate::render::PathOutline;
use crate::render::RenderMode;
use crate::render::RunRecord;
use crate::render::TextAntiAlias;
use crate::render::TextMaterial;
use crate::render::VisualBatchKey;
use crate::render::VisualLighting;
use crate::render::VisualMaterialInterner;
use crate::render::VisualShadow;
use crate::render::VisualSidedness;
use crate::render::constants;

/// Panel-line paths pack with a single band per axis. Bands shrink the
/// per-fragment curve loop for glyphs with hundreds of curves, but a line
/// rectangle has four and a cap at most a dozen — and at line scale (a tick
/// packs ~100 design units tall) 96 bands are ~1 unit wide, so the banded
/// distance scan cannot see an edge beyond ~2 units. The signed distance then
/// saturates inside the silhouette and the AA ramp collapses to a hard step.
/// One band keeps the distance exact at every zoom.
const PANEL_LINE_BAND_COUNT: usize = 1;

const PANEL_LINE_LINE_DEPTH_BIAS_STEP: f32 = 0.001;
const PANEL_LINE_PART_DEPTH_BIAS_STEP: f32 = 0.000_001;
const PANEL_LINE_LINE_OIT_DEPTH_STEP: f32 = 0.000_000_1;
const PANEL_LINE_PART_OIT_DEPTH_STEP: f32 = 0.000_000_001;

/// Marker on every panel-line batch render entity.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(super) struct DiegeticPanelLineBatch;

/// Coarse paint lane used as a batch split.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LinePaintLane {
    Normal,
    Overlay,
}

impl From<PanelLinePaintOrder> for LinePaintLane {
    fn from(order: PanelLinePaintOrder) -> Self {
        match order {
            PanelLinePaintOrder::Normal { .. } => Self::Normal,
            PanelLinePaintOrder::Overlay { .. } => Self::Overlay,
        }
    }
}

/// Cross-panel compatibility key for analytic panel-line path instances.
///
/// Per-primitive color, render mode, transform, sorted depth nudge, and OIT
/// offset live in `RunRecord`s. The material depth bias is only the coarse
/// normal/overlay lane so compatible marks can batch across panels and command
/// indices.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LineBatchKey {
    visual:              VisualBatchKey,
    paint_lane:          LinePaintLane,
    material_depth_bias: u32,
}

impl LineBatchKey {
    const fn new(
        visual: VisualBatchKey,
        paint_lane: LinePaintLane,
        material_depth_bias: f32,
    ) -> Self {
        Self {
            visual,
            paint_lane,
            material_depth_bias: material_depth_bias.to_bits(),
        }
    }

    const fn depth_bias(&self) -> f32 { f32::from_bits(self.material_depth_bias) }
}

/// GPU-side handles for one line path batch.
#[derive(Debug)]
struct LineBatchGpu {
    instances:    Handle<ShaderBuffer>,
    run_table:    Handle<ShaderBuffer>,
    mesh:         Handle<Mesh>,
    material:     Handle<TextMaterial>,
    capacity:     u32,
    run_capacity: u32,
}

/// One member primitive in a batch.
#[derive(Debug)]
struct LineBatchRecord {
    key:      PanelLineRenderKey,
    outline:  PathOutline,
    instance: GlyphInstanceRecord,
    run:      RunRecord,
}

/// One render entity + material + mesh per [`LineBatchKey`].
#[derive(Debug, Default)]
struct LineBatch {
    entity:        Option<Entity>,
    gpu:           Option<LineBatchGpu>,
    records_dirty: bool,
    bounds_dirty:  bool,
    records:       Vec<LineBatchRecord>,
}

impl LineBatch {
    const fn is_empty(&self) -> bool { self.records.is_empty() }

    fn record_count(&self) -> u32 { self.records.len().to_u32() }

    fn run_count(&self) -> u32 { self.records.len().to_u32() }

    fn instances(&self) -> Vec<GlyphInstanceRecord> {
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| GlyphInstanceRecord {
                run_index: index.to_u32(),
                ..record.instance
            })
            .collect()
    }

    fn run_records(&self) -> Vec<RunRecord> {
        self.records.iter().map(|record| record.run).collect()
    }

    fn push_record(&mut self, record: LineBatchRecord) {
        self.records.push(record);
        self.records_dirty = true;
        self.bounds_dirty = true;
    }

    fn remove_record(&mut self, key: PanelLineRenderKey) {
        if let Some(index) = self.records.iter().position(|record| record.key == key) {
            self.records.remove(index);
            self.records_dirty = true;
            self.bounds_dirty = true;
        }
    }

    fn world_bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::MAX;
        let mut max = Vec3::MIN;
        let mut any = false;
        for record in &self.records {
            for (corner_x, corner_y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let local = record.instance.rect_min
                    + Vec2::new(corner_x, corner_y) * record.instance.rect_size;
                let world = record
                    .run
                    .transform
                    .transform_point3(Vec3::new(local.x, local.y, 0.0));
                min = min.min(world);
                max = max.max(world);
                any = true;
            }
        }
        any.then_some((min, max))
    }
}

/// Routes panel-line primitives into compatible cross-panel batches.
#[derive(Debug, Default, Resource)]
pub(super) struct PanelLineBatchStore {
    batches:     HashMap<LineBatchKey, LineBatch>,
    panel_index: HashMap<Entity, Vec<(LineBatchKey, PanelLineRenderKey)>>,
    interner:    VisualMaterialInterner,
    atlas:       PathAtlas<PanelLineRenderKey>,
    atlas_dirty: bool,
}

impl PanelLineBatchStore {
    fn intern_base_material(&mut self, material: &StandardMaterial) -> BaseMaterialId {
        self.interner.intern_base_material(material)
    }

    fn base_material(&self, id: BaseMaterialId) -> &StandardMaterial {
        self.interner.base_material(id)
    }

    fn upsert_panel(&mut self, panel: Entity, records: Vec<(LineBatchKey, LineBatchRecord)>) {
        self.remove_panel(panel);
        if !records.is_empty() {
            self.atlas_dirty = true;
        }
        for (key, record) in records {
            let record_key = record.key;
            self.batches
                .entry(key.clone())
                .or_default()
                .push_record(record);
            self.panel_index
                .entry(panel)
                .or_default()
                .push((key, record_key));
        }
    }

    fn remove_panel(&mut self, panel: Entity) {
        let Some(records) = self.panel_index.remove(&panel) else {
            return;
        };
        self.atlas_dirty = true;
        for (key, record_key) in records {
            if let Some(batch) = self.batches.get_mut(&key) {
                batch.remove_record(record_key);
            }
        }
    }

    fn rebuild_path_atlas_if_dirty(&mut self) {
        if !self.atlas_dirty {
            return;
        }
        let paths: Vec<(PanelLineRenderKey, PathOutline)> = self
            .batches
            .values()
            .flat_map(|batch| {
                batch
                    .records
                    .iter()
                    .map(|record| (record.key, record.outline.clone()))
            })
            .collect();
        self.atlas.rebuild(paths, PANEL_LINE_BAND_COUNT);
        for batch in self.batches.values_mut() {
            for record in &mut batch.records {
                if let Some(atlas_index) = self.atlas.index(&record.key) {
                    record.instance.atlas_index = atlas_index;
                }
            }
            batch.records_dirty = true;
            batch.bounds_dirty = true;
        }
        self.atlas_dirty = false;
    }

    fn commit_path_atlas(
        &mut self,
        storage_buffers: &mut Assets<ShaderBuffer>,
        materials: &mut Assets<TextMaterial>,
    ) -> Option<GlyphAtlasHandles> {
        let (atlas, uploaded) = self.atlas.upload(storage_buffers)?;
        if uploaded {
            for batch in self.batches.values() {
                let Some(gpu) = &batch.gpu else {
                    continue;
                };
                if let Some(mut material) = materials.get_mut(&gpu.material) {
                    render::set_text_material_atlas(
                        &mut material,
                        atlas.curves.clone(),
                        atlas.bands.clone(),
                        atlas.glyphs.clone(),
                    );
                }
            }
        }
        Some(atlas)
    }

    fn take_empty_batches(&mut self) -> Vec<Entity> {
        let empty: Vec<LineBatchKey> = self
            .batches
            .iter()
            .filter(|(_, batch)| batch.is_empty())
            .map(|(key, _)| key.clone())
            .collect();
        let mut entities = Vec::new();
        for key in empty {
            if let Some(batch) = self.batches.remove(&key)
                && let Some(entity) = batch.entity
            {
                entities.push(entity);
            }
        }
        entities
    }

    fn batches(&self) -> impl Iterator<Item = (&LineBatchKey, &LineBatch)> { self.batches.iter() }

    fn batches_mut(&mut self) -> impl Iterator<Item = (&LineBatchKey, &mut LineBatch)> {
        self.batches.iter_mut()
    }

    fn get(&self, key: &LineBatchKey) -> Option<&LineBatch> { self.batches.get(key) }

    fn get_mut(&mut self, key: &LineBatchKey) -> Option<&mut LineBatch> {
        self.batches.get_mut(key)
    }
}

struct PanelLineReconcileContext<'a> {
    panel_entity:    Entity,
    panel:           &'a DiegeticPanel,
    panel_transform: Mat4,
    path_context:    PanelLinePathContext,
    shadow:          VisualShadow,
    layers:          BatchRenderLayers,
}

struct LinePrimitiveSource<'a> {
    element_index: usize,
    line:          &'a ResolvedPanelLine,
    primitive:     &'a ResolvedPanelLinePrimitive,
}

struct BuiltPanelLinePrimitive {
    batch_key: LineBatchKey,
    record:    LineBatchRecord,
}

pub(super) fn reconcile_panel_line_batches(
    changed_panels: Query<
        (
            Entity,
            &DiegeticPanel,
            &ComputedDiegeticPanel,
            &GlobalTransform,
            Option<&RenderLayers>,
            Option<&Visibility>,
        ),
        Or<(
            Changed<ComputedDiegeticPanel>,
            Changed<DiegeticPanel>,
            Changed<GlobalTransform>,
            Changed<RenderLayers>,
            Changed<Visibility>,
        )>,
    >,
    mut removed_computed: RemovedComponents<ComputedDiegeticPanel>,
    mut removed_panels: RemovedComponents<DiegeticPanel>,
    anti_alias: Res<TextAntiAlias>,
    mut store: ResMut<PanelLineBatchStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut commands: Commands,
) {
    for panel in removed_computed.read().chain(removed_panels.read()) {
        store.remove_panel(panel);
    }

    for (panel_entity, panel, computed, panel_transform, panel_layers, panel_visibility) in
        &changed_panels
    {
        let Some(result) = computed.result() else {
            store.remove_panel(panel_entity);
            continue;
        };
        if is_hidden(panel_visibility) {
            store.remove_panel(panel_entity);
            continue;
        }

        let (anchor_x, anchor_y) = panel.anchor_offsets();
        let context = PanelLineReconcileContext {
            panel_entity,
            panel,
            panel_transform: panel_transform.to_matrix(),
            path_context: PanelLinePathContext {
                points_to_world: panel.points_to_world(),
                anchor_x,
                anchor_y,
            },
            shadow: panel.surface_shadow().into(),
            layers: BatchRenderLayers(panel_layers.cloned().unwrap_or(RenderLayers::layer(0))),
        };
        let records = collect_panel_records(&context, &result.commands, &mut store);
        store.upsert_panel(panel_entity, records);
    }

    store.rebuild_path_atlas_if_dirty();
    let atlas = store.commit_path_atlas(&mut storage_buffers, &mut materials);
    reconcile_batch_entities(
        atlas.as_ref(),
        *anti_alias,
        &mut store,
        &mut meshes,
        &mut materials,
        &mut storage_buffers,
        &mut commands,
    );
}

const fn is_hidden(visibility: Option<&Visibility>) -> bool {
    matches!(visibility, Some(Visibility::Hidden))
}

fn collect_panel_records(
    context: &PanelLineReconcileContext<'_>,
    render_commands: &[RenderCommand],
    store: &mut PanelLineBatchStore,
) -> Vec<(LineBatchKey, LineBatchRecord)> {
    collect_line_primitives(render_commands)
        .into_iter()
        .filter_map(|source| build_panel_line_primitive(context, source, store))
        .map(|built| (built.batch_key, built.record))
        .collect()
}

fn collect_line_primitives(render_commands: &[RenderCommand]) -> Vec<LinePrimitiveSource<'_>> {
    let mut primitives = Vec::new();
    for command in render_commands {
        let RenderCommandKind::Lines { lines } = &command.kind else {
            continue;
        };
        for line in lines {
            for primitive in line.primitives() {
                primitives.push(LinePrimitiveSource {
                    element_index: command.element_idx,
                    line,
                    primitive,
                });
            }
        }
    }
    primitives
}

fn build_panel_line_primitive(
    context: &PanelLineReconcileContext<'_>,
    source: LinePrimitiveSource<'_>,
    store: &mut PanelLineBatchStore,
) -> Option<BuiltPanelLinePrimitive> {
    if clipped_out(source.primitive.bounds(), source.primitive.clip()) {
        return None;
    }

    let path = path::build_panel_line_path(source.primitive, &context.path_context)?;
    let depth_bias = primitive_depth_bias(source.line, source.primitive);
    let oit_depth_offset = primitive_oit_depth_offset(source.line, source.primitive);
    let base = constants::resolve_material(
        context.panel.tree().element_material(source.element_index),
        context.panel.material(),
        None,
    );
    let base_material = store.intern_base_material(&base);
    let visual = VisualBatchKey {
        base_material,
        alpha: BatchAlphaMode::Blend,
        lighting: VisualLighting::Unlit,
        sidedness: VisualSidedness::DoubleSided,
        shadow: context.shadow,
        layers: context.layers.clone(),
    };
    let paint_lane = LinePaintLane::from(source.line.paint_order());
    let batch_key = LineBatchKey::new(visual, paint_lane, batch_material_depth_bias(paint_lane));
    let key = PanelLineRenderKey {
        panel:  context.panel_entity,
        source: source.primitive.source_key(),
    };
    let linear = source.primitive.color().to_linear();
    let run = RunRecord {
        transform: context.panel_transform,
        fill_color: Vec4::new(linear.red, linear.green, linear.blue, linear.alpha),
        render_mode: u32::from(RenderMode::Text),
        depth_nudge: depth_bias,
        oit_depth_offset,
    };
    let instance = GlyphInstanceRecord {
        rect_min:    path.rect_min,
        rect_size:   path.rect_size,
        uv_min:      path.uv_min,
        uv_size:     path.uv_size,
        atlas_index: 0,
        run_index:   0,
    };
    Some(BuiltPanelLinePrimitive {
        batch_key,
        record: LineBatchRecord {
            key,
            outline: path.outline,
            instance,
            run,
        },
    })
}

fn primitive_depth_bias(line: &ResolvedPanelLine, primitive: &ResolvedPanelLinePrimitive) -> f32 {
    let line_depth = line_depth_order(line).to_f32().mul_add(
        PANEL_LINE_LINE_DEPTH_BIAS_STEP,
        line.layering().depth_bias(),
    );
    primitive
        .part_order()
        .to_f32()
        .mul_add(PANEL_LINE_PART_DEPTH_BIAS_STEP, line_depth)
}

const fn batch_material_depth_bias(paint_lane: LinePaintLane) -> f32 {
    match paint_lane {
        LinePaintLane::Normal => constants::BATCH_PANEL_LINE_DEPTH_BIAS,
        LinePaintLane::Overlay => constants::BATCH_PANEL_LINE_OVERLAY_DEPTH_BIAS,
    }
}

fn primitive_oit_depth_offset(
    line: &ResolvedPanelLine,
    primitive: &ResolvedPanelLinePrimitive,
) -> f32 {
    let line_depth = line_depth_order(line).to_f32().mul_add(
        PANEL_LINE_LINE_OIT_DEPTH_STEP,
        line.layering().oit_depth_offset(),
    );
    primitive
        .part_order()
        .to_f32()
        .mul_add(PANEL_LINE_PART_OIT_DEPTH_STEP, line_depth)
}

const fn line_depth_order(line: &ResolvedPanelLine) -> usize {
    match line.source_key() {
        PanelLineSourceKey::Element { line_ordinal, .. }
        | PanelLineSourceKey::External { line_ordinal, .. } => line_ordinal,
    }
}

fn clipped_out(bounds: BoundingBox, clip: Option<BoundingBox>) -> bool {
    clip.is_some_and(|clip| bounds.intersect(&clip).is_none())
}

fn reconcile_batch_entities(
    atlas: Option<&GlyphAtlasHandles>,
    anti_alias: TextAntiAlias,
    store: &mut PanelLineBatchStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TextMaterial>,
    storage_buffers: &mut Assets<ShaderBuffer>,
    commands: &mut Commands,
) {
    for entity in store.take_empty_batches() {
        commands.entity(entity).despawn();
    }
    let mut to_create = Vec::new();
    let mut to_grow = Vec::new();
    for (key, batch) in store.batches() {
        match &batch.gpu {
            None => to_create.push(key.clone()),
            Some(gpu)
                if batch.record_count() > gpu.capacity || batch.run_count() > gpu.run_capacity =>
            {
                to_grow.push(key.clone());
            },
            Some(_) => {},
        }
    }
    if let Some(atlas) = atlas {
        for key in to_create {
            spawn_batch_entity(
                &key,
                atlas,
                anti_alias,
                store,
                meshes,
                materials,
                storage_buffers,
                commands,
            );
        }
    }
    for key in to_grow {
        grow_batch_assets(&key, store, meshes, materials, storage_buffers, commands);
    }
}

fn spawn_batch_entity(
    key: &LineBatchKey,
    atlas: &GlyphAtlasHandles,
    anti_alias: TextAntiAlias,
    store: &mut PanelLineBatchStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TextMaterial>,
    storage_buffers: &mut Assets<ShaderBuffer>,
    commands: &mut Commands,
) {
    let Some(batch) = store.get(key) else {
        return;
    };
    let capacity = batch.record_count().max(1).next_power_of_two();
    let run_capacity = batch.run_count().max(1).next_power_of_two();
    let instances = storage_buffers.add(ShaderBuffer::from(padded_line_instances(
        &batch.instances(),
        capacity,
    )));
    let run_table = storage_buffers.add(ShaderBuffer::from(padded_line_runs(
        &batch.run_records(),
        run_capacity,
    )));
    let mesh = meshes.add(inert_line_batch_mesh(capacity));
    let material = materials.add(line_batch_material(LineBatchMaterialInput {
        base: store.base_material(key.visual.base_material).clone(),
        key,
        atlas,
        instances: instances.clone(),
        run_table: run_table.clone(),
        anti_alias,
    }));
    let mut batch_entity = commands.spawn((
        DiegeticPanelLineBatch,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        NoAutoAabb,
        Aabb::default(),
        key.visual.layers.0.clone(),
    ));
    if key.visual.shadow == VisualShadow::None {
        batch_entity.insert(NotShadowCaster);
    }
    let entity = batch_entity.id();

    if let Some(batch) = store.get_mut(key) {
        batch.entity = Some(entity);
        batch.gpu = Some(LineBatchGpu {
            instances,
            run_table,
            mesh,
            material,
            capacity,
            run_capacity,
        });
        batch.records_dirty = false;
    }
}

fn grow_batch_assets(
    key: &LineBatchKey,
    store: &mut PanelLineBatchStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TextMaterial>,
    storage_buffers: &mut Assets<ShaderBuffer>,
    commands: &mut Commands,
) {
    let Some(batch) = store.get_mut(key) else {
        return;
    };
    let Some(entity) = batch.entity else {
        return;
    };
    let required = batch.record_count();
    let run_required = batch.run_count();
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

    let instances = storage_buffers.add(ShaderBuffer::from(padded_line_instances(
        &batch.instances(),
        capacity,
    )));
    let run_table = storage_buffers.add(ShaderBuffer::from(padded_line_runs(
        &batch.run_records(),
        run_capacity,
    )));
    let mesh = meshes.add(inert_line_batch_mesh(capacity));
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
    batch.records_dirty = false;
}

pub(super) fn update_panel_line_batch_bounds(
    mut store: ResMut<PanelLineBatchStore>,
    mut batch_entities: Query<
        (&mut Transform, &mut GlobalTransform, &mut Aabb),
        With<DiegeticPanelLineBatch>,
    >,
) {
    for (_, batch) in store.batches_mut() {
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
        *transform = Transform::from_translation(center);
        *global = GlobalTransform::from(*transform);
        *aabb = Aabb {
            center:       Vec3A::ZERO,
            half_extents: Vec3A::from((max - min) * 0.5),
        };
        batch.bounds_dirty = false;
    }
}

pub(super) fn commit_panel_line_batch_buffers(
    mut store: ResMut<PanelLineBatchStore>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    let mut batches = 0_usize;
    let mut records = 0_usize;
    let mut uploads = 0_usize;
    for (_, batch) in store.batches_mut() {
        batches += 1;
        records += batch.record_count().to_usize();
        if batch.gpu.is_none() || !batch.records_dirty {
            continue;
        }
        let capacity = batch.gpu.as_ref().map_or(0, |gpu| gpu.capacity);
        let run_capacity = batch.gpu.as_ref().map_or(0, |gpu| gpu.run_capacity);
        let instances = padded_line_instances(&batch.instances(), capacity);
        let run_records = padded_line_runs(&batch.run_records(), run_capacity);
        batch.records_dirty = false;
        let Some(gpu) = &batch.gpu else {
            continue;
        };
        if let Some(mut buffer) = storage_buffers.get_mut(&gpu.instances) {
            buffer.set_data(instances);
            uploads += 1;
        }
        if let Some(mut buffer) = storage_buffers.get_mut(&gpu.run_table) {
            buffer.set_data(run_records);
            uploads += 1;
        }
    }
    perf.line_batch.batches = batches;
    perf.line_batch.records = records;
    perf.line_batch.uploads = uploads;
}

fn padded_line_instances(
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

fn padded_line_runs(records: &[RunRecord], run_capacity: u32) -> Vec<RunRecord> {
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
        },
    );
    padded
}

fn inert_line_batch_mesh(capacity: u32) -> Mesh {
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

struct LineBatchMaterialInput<'a> {
    base:       StandardMaterial,
    key:        &'a LineBatchKey,
    atlas:      &'a GlyphAtlasHandles,
    instances:  Handle<ShaderBuffer>,
    run_table:  Handle<ShaderBuffer>,
    anti_alias: TextAntiAlias,
}

fn line_batch_material(input: LineBatchMaterialInput<'_>) -> TextMaterial {
    let LineBatchMaterialInput {
        mut base,
        key,
        atlas,
        instances,
        run_table,
        anti_alias,
    } = input;
    base.alpha_mode = key.visual.alpha.into();
    base.unlit = matches!(key.visual.lighting, VisualLighting::Unlit);
    apply_visual_sidedness(&mut base, key.visual.sidedness);
    base.depth_bias = key.depth_bias();
    render::batch_text_material(BatchTextMaterialInput {
        base,
        fill_color: Vec4::ONE,
        render_mode: RenderMode::Text,
        oit_depth_offset: 0.0,
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

const fn apply_visual_sidedness(base: &mut StandardMaterial, sidedness: VisualSidedness) {
    match sidedness {
        VisualSidedness::DoubleSided => {
            base.double_sided = true;
            base.cull_mode = None;
        },
        VisualSidedness::OneSided => {
            base.double_sided = false;
            base.cull_mode = Some(Face::Back);
        },
    }
}

#[cfg(test)]
#[allow(clippy::panic, reason = "tests should panic on unexpected values")]
mod tests {
    use bevy::color::Color;

    use super::*;
    use crate::layout::PanelLineLayering;
    use crate::layout::PanelLinePrimitiveGeometry;
    use crate::layout::PanelLinePrimitiveKey;
    use crate::layout::PanelLinePrimitiveKind;
    use crate::render::Bounds;
    use crate::render::PathContour;
    use crate::render::QuadraticSegment;

    #[test]
    fn line_ordinal_contributes_to_depth_inside_grouped_command() {
        let first = test_line(0);
        let second = test_line(1);

        assert!(
            primitive_depth_bias(&second, &second.primitives()[0])
                > primitive_depth_bias(&first, &first.primitives()[0])
        );
        assert!(
            primitive_oit_depth_offset(&second, &second.primitives()[0])
                > primitive_oit_depth_offset(&first, &first.primitives()[0])
        );
    }

    #[test]
    fn batch_material_depth_uses_coarse_paint_lane() {
        assert_eq!(
            batch_material_depth_bias(LinePaintLane::Normal).to_bits(),
            constants::BATCH_PANEL_LINE_DEPTH_BIAS.to_bits(),
        );
        assert_eq!(
            batch_material_depth_bias(LinePaintLane::Overlay).to_bits(),
            constants::BATCH_PANEL_LINE_OVERLAY_DEPTH_BIAS.to_bits(),
        );
    }

    #[test]
    fn collect_line_primitives_preserves_resolved_overlay_clip() {
        let inherited_clip = BoundingBox {
            x:      0.0,
            y:      0.0,
            width:  100.0,
            height: 100.0,
        };
        let owner_clip = BoundingBox {
            x:      10.0,
            y:      10.0,
            width:  10.0,
            height: 10.0,
        };
        let source_key = PanelLineSourceKey::element(1, 0, 0);
        let primitive = ResolvedPanelLinePrimitive {
            source_key: PanelLinePrimitiveKey::new(source_key, 0),
            kind:       PanelLinePrimitiveKind::Segment,
            geometry:   PanelLinePrimitiveGeometry::Segment {
                start: Vec2::new(0.0, 0.0),
                end:   Vec2::new(50.0, 0.0),
                width: 2.0,
            },
            color:      Color::WHITE,
            bounds:     BoundingBox {
                x:      0.0,
                y:      -1.0,
                width:  50.0,
                height: 2.0,
            },
            clip:       Some(inherited_clip),
            part_order: 0,
        };
        let primitive_bounds = primitive.bounds();
        let line = ResolvedPanelLine {
            source_key,
            source_command_index: 0,
            owner_bounds: owner_clip,
            visual_bounds: primitive_bounds,
            clip: Some(inherited_clip),
            paint_order: PanelLinePaintOrder::Overlay { order: 0 },
            layering: PanelLineLayering {
                depth_bias:       0.0,
                oit_depth_offset: 0.0,
            },
            start: Vec2::ZERO,
            end: Vec2::new(50.0, 0.0),
            shaft_start: Vec2::ZERO,
            shaft_end: Vec2::new(50.0, 0.0),
            width: 2.0,
            color: Color::WHITE,
            primitives: vec![primitive],
        };
        let commands = vec![RenderCommand {
            bounds:      primitive_bounds,
            element_idx: 1,
            kind:        RenderCommandKind::Lines { lines: vec![line] },
        }];

        let collected = collect_line_primitives(&commands);

        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].primitive.clip(), Some(inherited_clip));
    }

    #[test]
    fn two_panels_with_same_key_share_one_line_batch() {
        let mut store = PanelLineBatchStore::default();
        let key = test_batch_key(&mut store);
        let first_panel = Entity::from_bits(1);
        let second_panel = Entity::from_bits(2);

        store.upsert_panel(
            first_panel,
            vec![(key.clone(), test_batch_record(first_panel, 0))],
        );
        store.upsert_panel(
            second_panel,
            vec![(key.clone(), test_batch_record(second_panel, 1))],
        );

        assert_eq!(store.batches().count(), 1);
        assert_eq!(store.get(&key).map(LineBatch::record_count), Some(2));
    }

    #[test]
    fn removing_a_panel_removes_only_its_line_records() {
        let mut store = PanelLineBatchStore::default();
        let key = test_batch_key(&mut store);
        let first_panel = Entity::from_bits(1);
        let second_panel = Entity::from_bits(2);

        store.upsert_panel(
            first_panel,
            vec![(key.clone(), test_batch_record(first_panel, 0))],
        );
        store.upsert_panel(
            second_panel,
            vec![(key.clone(), test_batch_record(second_panel, 1))],
        );

        store.remove_panel(first_panel);

        assert_eq!(store.get(&key).map(LineBatch::record_count), Some(1));
    }

    #[test]
    fn atlas_rebuild_compacts_surviving_panel_line_paths() {
        let mut store = PanelLineBatchStore::default();
        let key = test_batch_key(&mut store);
        let first_panel = Entity::from_bits(1);
        let second_panel = Entity::from_bits(2);
        store.upsert_panel(
            first_panel,
            vec![(key.clone(), test_batch_record(first_panel, 0))],
        );
        store.upsert_panel(
            second_panel,
            vec![(key.clone(), test_batch_record(second_panel, 1))],
        );
        store.rebuild_path_atlas_if_dirty();
        let Some(batch) = store.get(&key) else {
            panic!("batch should exist");
        };
        let first_indices: Vec<u32> = batch
            .instances()
            .iter()
            .map(|record| record.atlas_index)
            .collect();
        assert_eq!(first_indices, vec![0, 1]);

        store.remove_panel(first_panel);
        store.rebuild_path_atlas_if_dirty();

        let Some(batch) = store.get(&key) else {
            panic!("batch should still exist");
        };
        let second_indices: Vec<u32> = batch
            .instances()
            .iter()
            .map(|record| record.atlas_index)
            .collect();
        assert_eq!(second_indices, vec![0]);
    }

    #[test]
    fn line_batch_bounds_use_instance_rects_and_run_transforms() {
        let mut batch = LineBatch::default();
        let mut record = test_batch_record(Entity::from_bits(1), 0);
        record.instance.rect_min = Vec2::new(1.0, 2.0);
        record.instance.rect_size = Vec2::new(3.0, 4.0);
        record.run.transform = Mat4::from_translation(Vec3::new(10.0, 20.0, 0.0));
        batch.push_record(record);

        let bounds = batch.world_bounds();

        assert_eq!(
            bounds,
            Some((Vec3::new(11.0, 22.0, 0.0), Vec3::new(14.0, 26.0, 0.0))),
        );
    }

    fn test_batch_key(store: &mut PanelLineBatchStore) -> LineBatchKey {
        let base = StandardMaterial::default();
        let base_material = store.intern_base_material(&base);
        LineBatchKey::new(
            VisualBatchKey {
                base_material,
                alpha: BatchAlphaMode::Blend,
                lighting: VisualLighting::Lit,
                sidedness: VisualSidedness::DoubleSided,
                shadow: VisualShadow::Cast,
                layers: BatchRenderLayers(RenderLayers::layer(0)),
            },
            LinePaintLane::Normal,
            0.0,
        )
    }

    fn test_batch_record(panel: Entity, primitive_ordinal: usize) -> LineBatchRecord {
        LineBatchRecord {
            key:      PanelLineRenderKey {
                panel,
                source: PanelLinePrimitiveKey::new(
                    PanelLineSourceKey::element(primitive_ordinal, 0, 0),
                    0,
                ),
            },
            outline:  test_outline(),
            instance: GlyphInstanceRecord {
                rect_min:    Vec2::ZERO,
                rect_size:   Vec2::ONE,
                uv_min:      Vec2::ZERO,
                uv_size:     Vec2::ONE,
                atlas_index: 0,
                run_index:   0,
            },
            run:      RunRecord {
                transform:        Mat4::IDENTITY,
                fill_color:       Vec4::ONE,
                render_mode:      u32::from(RenderMode::Text),
                depth_nudge:      0.0,
                oit_depth_offset: 0.0,
            },
        }
    }

    fn test_outline() -> PathOutline {
        PathOutline {
            bounds:   Bounds {
                min: Vec2::ZERO,
                max: Vec2::ONE,
            },
            contours: vec![PathContour {
                segments: vec![
                    QuadraticSegment {
                        start:   Vec2::ZERO,
                        control: Vec2::new(0.5, 0.0),
                        end:     Vec2::X,
                    },
                    QuadraticSegment {
                        start:   Vec2::X,
                        control: Vec2::new(1.0, 0.5),
                        end:     Vec2::ONE,
                    },
                    QuadraticSegment {
                        start:   Vec2::ONE,
                        control: Vec2::new(0.5, 1.0),
                        end:     Vec2::Y,
                    },
                    QuadraticSegment {
                        start:   Vec2::Y,
                        control: Vec2::new(0.0, 0.5),
                        end:     Vec2::ZERO,
                    },
                ],
            }],
        }
    }

    fn test_line(line_ordinal: usize) -> ResolvedPanelLine {
        let source_key = PanelLineSourceKey::element(0, 0, line_ordinal);
        let primitive = ResolvedPanelLinePrimitive {
            source_key: PanelLinePrimitiveKey::new(source_key, 0),
            kind:       PanelLinePrimitiveKind::Segment,
            geometry:   PanelLinePrimitiveGeometry::Segment {
                start: Vec2::ZERO,
                end:   Vec2::X,
                width: 1.0,
            },
            color:      Color::WHITE,
            bounds:     BoundingBox {
                x:      0.0,
                y:      0.0,
                width:  1.0,
                height: 1.0,
            },
            clip:       None,
            part_order: 0,
        };
        ResolvedPanelLine {
            source_key,
            source_command_index: 0,
            owner_bounds: BoundingBox {
                x:      0.0,
                y:      0.0,
                width:  1.0,
                height: 1.0,
            },
            visual_bounds: primitive.bounds(),
            clip: None,
            paint_order: PanelLinePaintOrder::Normal { command_index: 0 },
            layering: PanelLineLayering {
                depth_bias:       0.0,
                oit_depth_offset: 0.0,
            },
            start: Vec2::ZERO,
            end: Vec2::X,
            shaft_start: Vec2::ZERO,
            shaft_end: Vec2::X,
            width: 1.0,
            color: Color::WHITE,
            primitives: vec![primitive],
        }
    }
}

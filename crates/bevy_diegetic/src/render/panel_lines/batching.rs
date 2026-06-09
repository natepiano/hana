//! Batched panel-line geometry.
//!
//! Every visible line primitive becomes one storage-buffer record. Compatible
//! records from any number of panels share one batch render entity, one inert
//! quad mesh, and one material; the vertex shader expands records into quads.

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
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::material;
use super::material::BatchLineMaterialInput;
use super::material::PanelLineBatchMaterial;
use super::primitive::LinePrimitiveClass;
use super::primitive::PanelLineGpuRecord;
use super::primitive::PanelLineRenderKey;
use crate::layout::BoundingBox;
use crate::layout::PanelLinePaintOrder;
use crate::layout::PanelLinePrimitiveGeometry;
use crate::layout::PanelLinePrimitiveKind;
use crate::layout::PanelLineSourceKey;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::layout::ResolvedPanelLine;
use crate::layout::ResolvedPanelLinePrimitive;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render::BaseMaterialId;
use crate::render::BatchAlphaMode;
use crate::render::BatchRenderLayers;
use crate::render::SDF_AA_PADDING;
use crate::render::SdfPrimitiveKind;
use crate::render::VisualBatchKey;
use crate::render::VisualLighting;
use crate::render::VisualMaterialInterner;
use crate::render::VisualShadow;
use crate::render::VisualSidedness;
use crate::render::constants;

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

/// Cross-panel compatibility key for line records.
///
/// Command-local depth stays in each [`PanelLineGpuRecord`]. The material
/// depth bias is only the coarse normal/overlay lane so compatible line
/// primitives can batch across panels and command indices.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LineBatchKey {
    visual:              VisualBatchKey,
    paint_lane:          LinePaintLane,
    primitive_class:     LinePrimitiveClass,
    material_depth_bias: u32,
}

impl LineBatchKey {
    const fn new(
        visual: VisualBatchKey,
        paint_lane: LinePaintLane,
        primitive_class: LinePrimitiveClass,
        material_depth_bias: f32,
    ) -> Self {
        Self {
            visual,
            paint_lane,
            primitive_class,
            material_depth_bias: material_depth_bias.to_bits(),
        }
    }

    const fn depth_bias(&self) -> f32 { f32::from_bits(self.material_depth_bias) }
}

/// GPU-side handles for one line batch.
#[derive(Debug)]
struct LineBatchGpu {
    records:  Handle<ShaderBuffer>,
    mesh:     Handle<Mesh>,
    material: Handle<PanelLineBatchMaterial>,
    capacity: u32,
}

/// One member primitive in a batch.
#[derive(Debug)]
struct LineBatchRecord {
    key:    PanelLineRenderKey,
    record: PanelLineGpuRecord,
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

    fn records(&self) -> Vec<PanelLineGpuRecord> {
        self.records.iter().map(|entry| entry.record).collect()
    }

    fn push_record(&mut self, key: PanelLineRenderKey, record: PanelLineGpuRecord) {
        self.records.push(LineBatchRecord { key, record });
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
        for entry in &self.records {
            let (record_min, record_max) = entry.record.world_bounds();
            min = min.min(record_min);
            max = max.max(record_max);
            any = true;
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
}

impl PanelLineBatchStore {
    fn intern_base_material(&mut self, material: &StandardMaterial) -> BaseMaterialId {
        self.interner.intern_base_material(material)
    }

    fn base_material(&self, id: BaseMaterialId) -> &StandardMaterial {
        self.interner.base_material(id)
    }

    fn upsert_panel(
        &mut self,
        panel: Entity,
        records: Vec<(LineBatchKey, PanelLineRenderKey, PanelLineGpuRecord)>,
    ) {
        self.remove_panel(panel);
        for (key, record_key, record) in records {
            self.batches
                .entry(key.clone())
                .or_default()
                .push_record(record_key, record);
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
        for (key, record_key) in records {
            if let Some(batch) = self.batches.get_mut(&key) {
                batch.remove_record(record_key);
            }
        }
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
    points_to_world: f32,
    anchor_x:        f32,
    anchor_y:        f32,
    shadow:          VisualShadow,
    layers:          BatchRenderLayers,
}

struct LinePrimitiveSource<'a> {
    element_index: usize,
    line:          &'a ResolvedPanelLine,
    primitive:     &'a ResolvedPanelLinePrimitive,
}

struct BuiltPanelLinePrimitive {
    key:        PanelLineRenderKey,
    batch_key:  LineBatchKey,
    gpu_record: PanelLineGpuRecord,
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
    mut store: ResMut<PanelLineBatchStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PanelLineBatchMaterial>>,
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
            points_to_world: panel.points_to_world(),
            anchor_x,
            anchor_y,
            shadow: panel.surface_shadow().into(),
            layers: BatchRenderLayers(panel_layers.cloned().unwrap_or(RenderLayers::layer(0))),
        };
        let records = collect_panel_records(&context, &result.commands, &mut store);
        store.upsert_panel(panel_entity, records);
    }

    reconcile_batch_entities(
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
) -> Vec<(LineBatchKey, PanelLineRenderKey, PanelLineGpuRecord)> {
    collect_line_primitives(render_commands)
        .into_iter()
        .filter_map(|source| build_panel_line_primitive(context, source, store))
        .map(|built| {
            let BuiltPanelLinePrimitive {
                key,
                batch_key,
                gpu_record,
                ..
            } = built;
            (batch_key, key, gpu_record)
        })
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

    let shape = primitive_shape(source.primitive, context)?;
    let clip_rect = primitive_clip_rect(
        source.primitive.clip(),
        shape.center,
        shape.mesh_half_size,
        context,
    );
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
    let batch_key = LineBatchKey::new(
        visual,
        paint_lane,
        LinePrimitiveClass::from(source.primitive.kind()),
        batch_material_depth_bias(paint_lane),
    );
    let gpu_record = PanelLineGpuRecord {
        transform: context.panel_transform
            * Mat4::from_translation(Vec3::new(shape.center.x, shape.center.y, 0.0)),
        mesh_half_kind_depth: Vec4::new(
            shape.mesh_half_size.x,
            shape.mesh_half_size.y,
            sdf_kind_u32(source.primitive.kind()).to_f32(),
            depth_bias,
        ),
        shape_oit: Vec4::new(
            shape.shape_half_size.x,
            shape.shape_half_size.y,
            oit_depth_offset,
            0.0,
        ),
        clip_rect,
        color: color_vec4(source.primitive.color()),
        params: primitive_sdf_params(source.primitive.kind(), &shape),
    };
    Some(BuiltPanelLinePrimitive {
        key: PanelLineRenderKey {
            panel:  context.panel_entity,
            source: source.primitive.source_key(),
        },
        batch_key,
        gpu_record,
    })
}

struct PrimitiveShape {
    center:          Vec2,
    axis:            Vec2,
    shape_half_size: Vec2,
    mesh_half_size:  Vec2,
}

fn primitive_shape(
    primitive: &ResolvedPanelLinePrimitive,
    context: &PanelLineReconcileContext<'_>,
) -> Option<PrimitiveShape> {
    match primitive.geometry() {
        PanelLinePrimitiveGeometry::Segment { start, end, width } => {
            let start = layout_point_to_panel(start, context);
            let end = layout_point_to_panel(end, context);
            let delta = end - start;
            let length = delta.length();
            if length <= f32::EPSILON || width <= 0.0 {
                return None;
            }
            let axis = delta / length;
            let shape_half_size = Vec2::new(length * 0.5, width * context.points_to_world * 0.5);
            Some(shape_from_center_axis(
                (start + end) * 0.5,
                axis,
                shape_half_size,
            ))
        },
        PanelLinePrimitiveGeometry::Form {
            center,
            axis,
            half_size,
        } => {
            let center = layout_point_to_panel(center, context);
            let axis = layout_axis_to_panel(axis)?;
            let shape_half_size = half_size * context.points_to_world;
            Some(shape_from_center_axis(center, axis, shape_half_size))
        },
    }
}

fn shape_from_center_axis(center: Vec2, axis: Vec2, shape_half_size: Vec2) -> PrimitiveShape {
    let mesh_half_size = oriented_mesh_half_size(axis, shape_half_size);
    PrimitiveShape {
        center,
        axis,
        shape_half_size,
        mesh_half_size,
    }
}

fn oriented_mesh_half_size(axis: Vec2, shape_half_size: Vec2) -> Vec2 {
    let perp = perp(axis);
    Vec2::new(
        axis.x
            .abs()
            .mul_add(shape_half_size.x, perp.x.abs() * shape_half_size.y),
        axis.y
            .abs()
            .mul_add(shape_half_size.x, perp.y.abs() * shape_half_size.y),
    ) + Vec2::splat(SDF_AA_PADDING)
}

fn primitive_clip_rect(
    clip: Option<BoundingBox>,
    center: Vec2,
    mesh_half_size: Vec2,
    context: &PanelLineReconcileContext<'_>,
) -> Vec4 {
    clip.map_or_else(
        || {
            Vec4::new(
                -mesh_half_size.x,
                -mesh_half_size.y,
                mesh_half_size.x,
                mesh_half_size.y,
            )
        },
        |clip| {
            let left = clip.x.mul_add(context.points_to_world, -context.anchor_x) - center.x;
            let right = (clip.x + clip.width).mul_add(context.points_to_world, -context.anchor_x)
                - center.x;
            let top = -(clip.y.mul_add(context.points_to_world, -context.anchor_y)) - center.y;
            let bottom = -((clip.y + clip.height)
                .mul_add(context.points_to_world, -context.anchor_y))
                - center.y;
            let pad = SDF_AA_PADDING;
            Vec4::new(
                left - pad,
                bottom.min(top) - pad,
                right + pad,
                bottom.max(top) + pad,
            )
        },
    )
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

const fn sdf_kind(kind: PanelLinePrimitiveKind) -> SdfPrimitiveKind {
    match kind {
        PanelLinePrimitiveKind::Segment => SdfPrimitiveKind::LineSegment,
        PanelLinePrimitiveKind::Triangle => SdfPrimitiveKind::OrientedTriangle,
        PanelLinePrimitiveKind::Circle => SdfPrimitiveKind::Circle,
        PanelLinePrimitiveKind::Square => SdfPrimitiveKind::OrientedSquare,
        PanelLinePrimitiveKind::Diamond => SdfPrimitiveKind::OrientedDiamond,
    }
}

fn sdf_kind_u32(kind: PanelLinePrimitiveKind) -> u32 { u32::from(sdf_kind(kind)) }

fn primitive_sdf_params(kind: PanelLinePrimitiveKind, shape: &PrimitiveShape) -> Vec4 {
    match kind {
        PanelLinePrimitiveKind::Segment => Vec4::new(shape.axis.x, shape.axis.y, 0.0, 0.0),
        PanelLinePrimitiveKind::Triangle => Vec4::new(
            shape.shape_half_size.x * 0.08,
            0.6,
            shape.axis.x,
            shape.axis.y,
        ),
        PanelLinePrimitiveKind::Circle => Vec4::ZERO,
        PanelLinePrimitiveKind::Square | PanelLinePrimitiveKind::Diamond => {
            Vec4::new(0.0, 0.0, shape.axis.x, shape.axis.y)
        },
    }
}

fn layout_point_to_panel(point: Vec2, context: &PanelLineReconcileContext<'_>) -> Vec2 {
    Vec2::new(
        point.x.mul_add(context.points_to_world, -context.anchor_x),
        -(point.y.mul_add(context.points_to_world, -context.anchor_y)),
    )
}

fn layout_axis_to_panel(axis: Vec2) -> Option<Vec2> { Vec2::new(axis.x, -axis.y).try_normalize() }

fn perp(axis: Vec2) -> Vec2 { Vec2::new(-axis.y, axis.x) }

fn color_vec4(color: Color) -> Vec4 {
    let linear = color.to_linear();
    Vec4::new(linear.red, linear.green, linear.blue, linear.alpha)
}

fn reconcile_batch_entities(
    store: &mut PanelLineBatchStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<PanelLineBatchMaterial>,
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
            Some(gpu) if batch.record_count() > gpu.capacity => to_grow.push(key.clone()),
            Some(_) => {},
        }
    }
    for key in to_create {
        spawn_batch_entity(&key, store, meshes, materials, storage_buffers, commands);
    }
    for key in to_grow {
        grow_batch_assets(&key, store, meshes, materials, storage_buffers, commands);
    }
}

fn spawn_batch_entity(
    key: &LineBatchKey,
    store: &mut PanelLineBatchStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<PanelLineBatchMaterial>,
    storage_buffers: &mut Assets<ShaderBuffer>,
    commands: &mut Commands,
) {
    let Some(batch) = store.get(key) else {
        return;
    };
    let capacity = batch.record_count().max(1).next_power_of_two();
    let records = padded_line_records(&batch.records(), capacity);
    let mut base = store.base_material(key.visual.base_material).clone();
    base.unlit = matches!(key.visual.lighting, VisualLighting::Unlit);
    let buffer = storage_buffers.add(ShaderBuffer::from(records));
    let mesh = meshes.add(inert_line_batch_mesh(capacity));
    let material = materials.add(material::batch_line_material(BatchLineMaterialInput {
        base,
        records: buffer.clone(),
        depth_bias: key.depth_bias(),
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
            records: buffer,
            mesh,
            material,
            capacity,
        });
        batch.records_dirty = false;
    }
}

fn grow_batch_assets(
    key: &LineBatchKey,
    store: &mut PanelLineBatchStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<PanelLineBatchMaterial>,
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
    let Some(current_capacity) = batch.gpu.as_ref().map(|gpu| gpu.capacity) else {
        return;
    };
    let mut capacity = current_capacity.max(1);
    while capacity < required {
        capacity *= 2;
    }

    let records = storage_buffers.add(ShaderBuffer::from(padded_line_records(
        &batch.records(),
        capacity,
    )));
    let mesh = meshes.add(inert_line_batch_mesh(capacity));
    commands.entity(entity).insert(Mesh3d(mesh.clone()));

    let Some(gpu) = &mut batch.gpu else {
        return;
    };
    if let Some(mut material) = materials.get_mut(&gpu.material) {
        material::set_batch_line_material_buffer(&mut material, records.clone());
    }
    gpu.records = records;
    gpu.mesh = mesh;
    gpu.capacity = capacity;
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
        let payload = padded_line_records(&batch.records(), capacity);
        batch.records_dirty = false;
        let Some(gpu) = &batch.gpu else {
            continue;
        };
        if let Some(mut buffer) = storage_buffers.get_mut(&gpu.records) {
            buffer.set_data(payload);
            uploads += 1;
        }
    }
    perf.line_batch.batches = batches;
    perf.line_batch.records = records;
    perf.line_batch.uploads = uploads;
}

fn padded_line_records(records: &[PanelLineGpuRecord], capacity: u32) -> Vec<PanelLineGpuRecord> {
    let mut padded = Vec::with_capacity(capacity.to_usize());
    padded.extend_from_slice(records);
    padded.resize(
        capacity.to_usize().max(records.len()),
        PanelLineGpuRecord::default(),
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

#[cfg(test)]
mod tests {
    use bevy::color::Color;
    use bevy::math::Vec4Swizzles;

    use super::*;
    use crate::layout::PanelLineLayering;
    use crate::layout::PanelLinePaintOrder;
    use crate::layout::PanelLinePrimitiveGeometry;
    use crate::layout::PanelLinePrimitiveKey;
    use crate::layout::PanelLineSourceKey;

    #[test]
    fn segment_primitives_use_butt_line_sdf_kind() {
        assert_eq!(
            sdf_kind(PanelLinePrimitiveKind::Segment),
            SdfPrimitiveKind::LineSegment
        );
    }

    #[test]
    fn triangle_params_keep_shape_controls_separate_from_axis() {
        let params = primitive_sdf_params(
            PanelLinePrimitiveKind::Triangle,
            &PrimitiveShape {
                center:          Vec2::ZERO,
                axis:            Vec2::new(0.0, 1.0),
                shape_half_size: Vec2::new(10.0, 4.0),
                mesh_half_size:  Vec2::new(10.0, 4.0),
            },
        );

        assert!((params.x - 0.8).abs() < f32::EPSILON);
        assert!((params.y - 0.6).abs() < f32::EPSILON);
        assert_eq!(params.zw(), Vec2::new(0.0, 1.0));
    }

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
        let base = StandardMaterial::default();
        let base_material = store.intern_base_material(&base);
        let key = LineBatchKey::new(
            VisualBatchKey {
                base_material,
                alpha: BatchAlphaMode::Blend,
                lighting: VisualLighting::Lit,
                sidedness: VisualSidedness::DoubleSided,
                shadow: VisualShadow::Cast,
                layers: BatchRenderLayers(RenderLayers::layer(0)),
            },
            LinePaintLane::Normal,
            LinePrimitiveClass::Segment,
            0.0,
        );
        let first_panel = Entity::from_bits(1);
        let second_panel = Entity::from_bits(2);
        let record = PanelLineGpuRecord::default();
        store.upsert_panel(
            first_panel,
            vec![(
                key.clone(),
                PanelLineRenderKey {
                    panel:  first_panel,
                    source: PanelLinePrimitiveKey::new(PanelLineSourceKey::element(0, 0, 0), 0),
                },
                record,
            )],
        );
        store.upsert_panel(
            second_panel,
            vec![(
                key.clone(),
                PanelLineRenderKey {
                    panel:  second_panel,
                    source: PanelLinePrimitiveKey::new(PanelLineSourceKey::element(1, 0, 0), 0),
                },
                record,
            )],
        );

        assert_eq!(store.batches().count(), 1);
        assert_eq!(store.get(&key).map(LineBatch::record_count), Some(2));
    }

    #[test]
    fn removing_a_panel_removes_only_its_line_records() {
        let mut store = PanelLineBatchStore::default();
        let base = StandardMaterial::default();
        let base_material = store.intern_base_material(&base);
        let key = LineBatchKey::new(
            VisualBatchKey {
                base_material,
                alpha: BatchAlphaMode::Blend,
                lighting: VisualLighting::Lit,
                sidedness: VisualSidedness::DoubleSided,
                shadow: VisualShadow::Cast,
                layers: BatchRenderLayers(RenderLayers::layer(0)),
            },
            LinePaintLane::Normal,
            LinePrimitiveClass::Segment,
            0.0,
        );
        let first_panel = Entity::from_bits(1);
        let second_panel = Entity::from_bits(2);
        let record = PanelLineGpuRecord::default();
        store.upsert_panel(
            first_panel,
            vec![(
                key.clone(),
                PanelLineRenderKey {
                    panel:  first_panel,
                    source: PanelLinePrimitiveKey::new(PanelLineSourceKey::element(0, 0, 0), 0),
                },
                record,
            )],
        );
        store.upsert_panel(
            second_panel,
            vec![(
                key.clone(),
                PanelLineRenderKey {
                    panel:  second_panel,
                    source: PanelLinePrimitiveKey::new(PanelLineSourceKey::element(1, 0, 0), 0),
                },
                record,
            )],
        );

        store.remove_panel(first_panel);

        assert_eq!(store.get(&key).map(LineBatch::record_count), Some(1));
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

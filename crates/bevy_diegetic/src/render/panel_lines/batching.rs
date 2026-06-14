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
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::path;
use super::path::PanelLineMember;
use super::path::PanelLinePathContext;
use super::primitive::PanelLineRenderKey;
use crate::cascade::CascadeDefault;
use crate::cascade::Resolved;
use crate::layout::BoundingBox;
use crate::layout::Lighting;
use crate::layout::PanelLineSourceKey;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::layout::ResolvedPanelLine;
use crate::layout::ResolvedPanelLinePrimitive;
use crate::layout::Sidedness;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render;
use crate::render::AntiAlias;
use crate::render::BaseMaterialId;
use crate::render::BatchAlphaMode;
use crate::render::BatchRenderLayers;
use crate::render::BatchTextMaterialInput;
use crate::render::GlyphAtlasHandles;
use crate::render::GlyphInstanceRecord;
use crate::render::HairlineFade;
use crate::render::PathAtlas;
use crate::render::PathOutline;
use crate::render::RenderMode;
use crate::render::RunRecord;
use crate::render::TextMaterial;
use crate::render::VisualBatchKey;
use crate::render::VisualMaterialInterner;
use crate::render::VisualShadow;
use crate::render::draw_order;
use crate::render::draw_order::DrawCommandDepth;
use crate::render::draw_order::DrawOrderProjection;
use crate::render::draw_order::ScreenDepthBias;

/// Target design-unit extent for one panel-line band (≈ 5.8mm at the
/// reference design scale). Bands shrink the per-fragment curve loop — a
/// merged ruler path carries hundreds of curves — but the banded distance
/// scan is blind past the band overlap (half this extent), so bands smaller
/// than the on-screen scan width collapse the AA ramp to a hard step. At
/// this size a single tick or arrowhead still packs one exact band, and
/// blindness only sets in once a whole millimeter ruler drops under ~200
/// screen pixels, where 1mm ticks are sub-pixel mush regardless.
const PANEL_LINE_BAND_TARGET_DESIGN_UNITS: f32 = 2048.0;

const PANEL_LINE_LINE_DEPTH_BIAS_STEP: f32 = 0.001;
const PANEL_LINE_PART_DEPTH_BIAS_STEP: f32 = 0.000_001;
const PANEL_LINE_LINE_OIT_DEPTH_STEP: f32 = 0.000_000_1;
const PANEL_LINE_PART_OIT_DEPTH_STEP: f32 = 0.000_000_001;

/// Marker on every panel-line batch render entity.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(super) struct DiegeticPanelLineBatch;

/// Cross-panel compatibility key for analytic panel-line path instances.
///
/// Per-primitive color, render mode, transform, sorted depth nudge, and OIT
/// offset live in `RunRecord`s. `LineBatchKey::z_level` selects the shared
/// panel-line lane for that authored level.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LineBatchKey {
    visual:  VisualBatchKey,
    z_level: i8,
}

impl LineBatchKey {
    const fn new(visual: VisualBatchKey, z_level: i8) -> Self { Self { visual, z_level } }

    fn depth_bias(&self) -> ScreenDepthBias { draw_order::line_batch_depth_bias(self.z_level) }
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
        self.atlas
            .rebuild(paths, PANEL_LINE_BAND_TARGET_DESIGN_UNITS);
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
    panel_entity:        Entity,
    panel:               &'a DiegeticPanel,
    panel_transform:     Mat4,
    path_context:        PanelLinePathContext,
    shadow:              VisualShadow,
    layers:              BatchRenderLayers,
    /// The panel entity's cascade-resolved lighting mode; every line on the
    /// panel renders with it, matching the panel's glyph runs.
    panel_lighting:      Lighting,
    /// The panel entity's cascade-resolved sidedness; every line on the panel
    /// renders with it, matching the panel's glyph runs.
    panel_sidedness:     Sidedness,
    /// The panel entity's cascade-resolved anti-alias mode; elements without
    /// their own override inherit it.
    panel_anti_alias:    AntiAlias,
    /// The panel entity's cascade-resolved hairline fade policy; elements
    /// without their own override inherit it.
    panel_hairline_fade: HairlineFade,
}

struct LinePrimitiveSource<'a> {
    element_index: usize,
    draw_depth:    DrawCommandDepth,
    line:          &'a ResolvedPanelLine,
    primitive:     &'a ResolvedPanelLinePrimitive,
}

struct BuiltPanelLinePrimitive {
    batch_key: LineBatchKey,
    record:    LineBatchRecord,
}

/// Same-silhouette grouping key: primitives that agree on everything that
/// must be uniform across one merged analytic path. Members of one group
/// render as a single multi-contour path, so abutting strokes (tick meets
/// spine) share one winding field and one anti-aliasing ramp.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LineMergeKey {
    element_index: usize,
    color:         [u32; 4],
    clip:          Option<[u32; 4]>,
    owner_bounds:  [u32; 4],
    layering:      [u32; 2],
}

impl From<&LinePrimitiveSource<'_>> for LineMergeKey {
    fn from(source: &LinePrimitiveSource<'_>) -> Self {
        let linear = source.primitive.color().to_linear();
        Self {
            element_index: source.element_index,
            color:         [
                linear.red.to_bits(),
                linear.green.to_bits(),
                linear.blue.to_bits(),
                linear.alpha.to_bits(),
            ],
            clip:          source.primitive.clip().map(bounding_box_bits),
            owner_bounds:  bounding_box_bits(source.line.owner_bounds()),
            layering:      [
                source.draw_depth.depth_bias().get().to_bits(),
                source.draw_depth.oit_depth_offset().get().to_bits(),
            ],
        }
    }
}

const fn bounding_box_bits(bounds: BoundingBox) -> [u32; 4] {
    [
        bounds.x.to_bits(),
        bounds.y.to_bits(),
        bounds.width.to_bits(),
        bounds.height.to_bits(),
    ]
}

/// Splits the primitive list into merge groups, preserving first-occurrence
/// order so group identities stay stable across reconciles.
fn group_line_primitives<'a>(
    sources: Vec<LinePrimitiveSource<'a>>,
) -> Vec<Vec<LinePrimitiveSource<'a>>> {
    let mut groups: Vec<Vec<LinePrimitiveSource<'a>>> = Vec::new();
    let mut group_indices: HashMap<LineMergeKey, usize> = HashMap::new();
    for source in sources {
        let key = LineMergeKey::from(&source);
        if let Some(&index) = group_indices.get(&key) {
            groups[index].push(source);
        } else {
            group_indices.insert(key, groups.len());
            groups.push(vec![source]);
        }
    }
    groups
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
            Option<&Resolved<Lighting>>,
            Option<&Resolved<Sidedness>>,
            Option<&Resolved<AntiAlias>>,
            Option<&Resolved<HairlineFade>>,
        ),
        Or<(
            Changed<ComputedDiegeticPanel>,
            Changed<DiegeticPanel>,
            Changed<GlobalTransform>,
            Changed<RenderLayers>,
            Changed<Visibility>,
            Changed<Resolved<Lighting>>,
            Changed<Resolved<Sidedness>>,
            Changed<Resolved<AntiAlias>>,
            Changed<Resolved<HairlineFade>>,
        )>,
    >,
    mut removed_computed: RemovedComponents<ComputedDiegeticPanel>,
    mut removed_panels: RemovedComponents<DiegeticPanel>,
    anti_alias: Res<AntiAlias>,
    lighting_default: Res<CascadeDefault<Lighting>>,
    sidedness_default: Res<CascadeDefault<Sidedness>>,
    anti_alias_default: Res<CascadeDefault<AntiAlias>>,
    hairline_fade_default: Res<CascadeDefault<HairlineFade>>,
    mut store: ResMut<PanelLineBatchStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut commands: Commands,
) {
    for panel in removed_computed.read().chain(removed_panels.read()) {
        store.remove_panel(panel);
    }

    for (
        panel_entity,
        panel,
        computed,
        panel_transform,
        panel_layers,
        panel_visibility,
        panel_lighting,
        panel_sidedness,
        panel_anti_alias,
        panel_hairline_fade,
    ) in &changed_panels
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
            panel_lighting: panel_lighting.map_or(lighting_default.0, |resolved| resolved.0),
            panel_sidedness: panel_sidedness.map_or(sidedness_default.0, |resolved| resolved.0),
            panel_anti_alias: panel_anti_alias.map_or(anti_alias_default.0, |resolved| resolved.0),
            panel_hairline_fade: panel_hairline_fade
                .map_or(hairline_fade_default.0, |resolved| resolved.0),
        };
        let records = collect_panel_records(
            &context,
            &result.commands,
            computed.draw_order(),
            &mut store,
        );
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
    draw_order: &DrawOrderProjection,
    store: &mut PanelLineBatchStore,
) -> Vec<(LineBatchKey, LineBatchRecord)> {
    group_line_primitives(collect_line_primitives(render_commands, draw_order))
        .into_iter()
        .filter_map(|group| build_panel_line_group(context, group, store))
        .map(|built| (built.batch_key, built.record))
        .collect()
}

fn collect_line_primitives<'a>(
    render_commands: &'a [RenderCommand],
    draw_order: &DrawOrderProjection,
) -> Vec<LinePrimitiveSource<'a>> {
    let mut primitives = Vec::new();
    for (command_index, command) in render_commands.iter().enumerate() {
        let RenderCommandKind::Lines { lines } = &command.kind else {
            continue;
        };
        let Some(draw_depth) = draw_order.depth_for(command_index) else {
            continue;
        };
        for line in lines {
            for primitive in line.primitives() {
                primitives.push(LinePrimitiveSource {
                    element_index: command.element_idx,
                    draw_depth,
                    line,
                    primitive,
                });
            }
        }
    }
    primitives
}

fn build_panel_line_group(
    context: &PanelLineReconcileContext<'_>,
    group: Vec<LinePrimitiveSource<'_>>,
    store: &mut PanelLineBatchStore,
) -> Option<BuiltPanelLinePrimitive> {
    let members: Vec<&LinePrimitiveSource<'_>> = group
        .iter()
        .filter(|source| !clipped_out(source.primitive.bounds(), source.primitive.clip()))
        .collect();
    let first = members.first()?;

    // Element override else the panel's cascade-resolved value; each line may
    // override both. Fade is per-curve in the merged path, so mixed policies
    // share this one group.
    let element_hairline_fade = context
        .panel
        .tree()
        .element_hairline_fade(first.element_index)
        .unwrap_or(context.panel_hairline_fade);
    let path_members: Vec<PanelLineMember<'_>> = members
        .iter()
        .map(|source| PanelLineMember {
            primitive:     source.primitive,
            fade_exponent: source
                .line
                .hairline_fade()
                .unwrap_or(element_hairline_fade)
                .fade_exponent(),
        })
        .collect();
    let path = path::build_panel_line_path(
        &path_members,
        first.line.owner_bounds(),
        first.primitive.clip(),
        &context.path_context,
    )?;
    // A merged silhouette has one depth; the lowest member offset keeps the
    // group at the depth the front-most authoring order produced alone.
    let depth_bias = members
        .iter()
        .map(|source| primitive_depth_bias(source))
        .fold(f32::INFINITY, f32::min);
    let oit_depth_offset = members
        .iter()
        .map(|source| primitive_oit_depth_offset(source))
        .fold(f32::INFINITY, f32::min);
    let base = render::resolve_material(
        context.panel.tree().element_material(first.element_index),
        context.panel.material(),
        None,
    );
    let base_material = store.intern_base_material(&base);
    let visual = VisualBatchKey {
        base_material,
        alpha: BatchAlphaMode::Blend,
        lighting: context.panel_lighting,
        sidedness: context.panel_sidedness,
        shadow: context.shadow,
        layers: context.layers.clone(),
    };
    let batch_key = LineBatchKey::new(visual, first.draw_depth.z_level());
    let key = PanelLineRenderKey {
        panel:  context.panel_entity,
        source: first.primitive.source_key(),
    };
    // Element override else the panel's cascade-resolved value. The merge key
    // groups per element, so every member of this group shares one resolution.
    let anti_alias = context
        .panel
        .tree()
        .element_anti_alias(first.element_index)
        .unwrap_or(context.panel_anti_alias);
    let linear = first.primitive.color().to_linear();
    let run = RunRecord {
        transform: context.panel_transform,
        fill_color: Vec4::new(linear.red, linear.green, linear.blue, linear.alpha),
        render_mode: u32::from(RenderMode::Text),
        depth_nudge: depth_bias,
        oit_depth_offset,
        aa_flags: anti_alias.aa_flags(),
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

fn primitive_depth_bias(source: &LinePrimitiveSource<'_>) -> f32 {
    let line_depth = line_depth_order(source.line).to_f32().mul_add(
        PANEL_LINE_LINE_DEPTH_BIAS_STEP,
        source.draw_depth.depth_bias().get(),
    );
    source
        .primitive
        .part_order()
        .to_f32()
        .mul_add(PANEL_LINE_PART_DEPTH_BIAS_STEP, line_depth)
}

fn primitive_oit_depth_offset(source: &LinePrimitiveSource<'_>) -> f32 {
    let line_depth = line_depth_order(source.line).to_f32().mul_add(
        PANEL_LINE_LINE_OIT_DEPTH_STEP,
        source.draw_depth.oit_depth_offset().get(),
    );
    source
        .primitive
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
    anti_alias: AntiAlias,
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
    anti_alias: AntiAlias,
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
            aa_flags:         0,
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
    anti_alias: AntiAlias,
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
    base.unlit = matches!(key.visual.lighting, Lighting::Unlit);
    render::apply_glyph_sidedness(&mut base, key.visual.sidedness);
    base.depth_bias = key.depth_bias().get();
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

#[cfg(test)]
#[allow(clippy::panic, reason = "tests should panic on unexpected values")]
mod tests {
    use std::sync::Arc;

    use bevy::asset::AssetPlugin;
    use bevy::color::Color;

    use super::*;
    use crate::El;
    use crate::Mm;
    use crate::cascade::CascadePlugin;
    use crate::cascade::CascadeSet;
    use crate::cascade::DrawLayer;
    use crate::layout::PanelDraw;
    use crate::layout::PanelLine;
    use crate::layout::PanelLinePrimitiveGeometry;
    use crate::layout::PanelLinePrimitiveKey;
    use crate::layout::PanelLinePrimitiveKind;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::Bounds;
    use crate::render::HairlineWidth;
    use crate::render::PathContour;
    use crate::render::QuadraticSegment;
    use crate::render::constants::DRAW_LEVEL_GEOMETRY_LANES;
    use crate::render::constants::LAYER_DEPTH_BIAS;
    use crate::text::DiegeticTextMeasurer;

    /// Headless app wired with panel layout, the AA/fade cascade plugins
    /// (via [`HeadlessLayoutPlugin`]), the lighting/sidedness cascade plugins
    /// (which `RenderPlugin` gets from `TextRenderPlugin`), the production
    /// cascade-root sync systems, and the panel-line reconcile.
    fn line_batch_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .insert_resource(DiegeticTextMeasurer {
                measure_fn: Arc::new(|_text: &str, measure: &TextMeasure| TextDimensions {
                    width:       measure.size,
                    height:      measure.size,
                    line_height: measure.size,
                }),
            })
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<Lighting>::default())
            .add_plugins(CascadePlugin::<Sidedness>::default())
            .init_resource::<AntiAlias>()
            .init_resource::<HairlineWidth>()
            .init_resource::<PanelLineBatchStore>()
            .init_asset::<Mesh>()
            .init_asset::<TextMaterial>()
            .init_asset::<ShaderBuffer>()
            .add_systems(
                Update,
                (
                    crate::render::sync_anti_alias,
                    crate::render::sync_hairline_fade,
                )
                    .before(CascadeSet::Propagate),
            )
            .add_systems(PostUpdate, reconcile_panel_line_batches);
        app
    }

    fn horizontal_line() -> PanelLine {
        PanelLine::new((0.0, 5.0), (20.0, 5.0))
            .width(0.4)
            .color(Color::WHITE)
    }

    fn spawn_line_panel(app: &mut App, draw_layer: Option<DrawLayer>) -> Entity {
        let mut line_element = El::new()
            .size(40.0, 20.0)
            .draw(PanelDraw::lines([horizontal_line()]));
        if let Some(draw_layer) = draw_layer {
            line_element = line_element.draw_layer(draw_layer);
        }
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(60.0))
            .layout(|builder| {
                builder.with(line_element, |_| {});
            })
            .build()
            .unwrap_or_else(|error| panic!("line panel should build: {error:?}"));
        app.world_mut().spawn(panel).id()
    }

    fn settle(app: &mut App) {
        for _ in 0..3 {
            app.update();
        }
    }

    fn one_line_batch_values(app: &App) -> (i8, f32, f32, Vec<(f32, f32)>) {
        let store = app.world().resource::<PanelLineBatchStore>();
        let Some((key, batch)) = store.batches().next() else {
            panic!("one line batch should exist");
        };
        let Some(gpu) = batch.gpu.as_ref() else {
            panic!("line batch should have GPU assets");
        };
        let Some(material) = app
            .world()
            .resource::<Assets<TextMaterial>>()
            .get(&gpu.material)
        else {
            panic!("line batch material should exist");
        };
        let mut records: Vec<(f32, f32)> = batch
            .run_records()
            .into_iter()
            .map(|record| (record.depth_nudge, record.oit_depth_offset))
            .collect();
        records.sort_by(|left, right| left.0.total_cmp(&right.0));
        (
            key.z_level,
            material.base.depth_bias,
            render::text_material_oit_depth_offset(material),
            records,
        )
    }

    /// Per record: the run's AA flags and the packed outline's fade exponent
    /// (fade is per-curve data carried by the record's contours, not a run
    /// field).
    fn sorted_run_fields(store: &PanelLineBatchStore) -> Vec<(u32, u32)> {
        let mut fields: Vec<(u32, u32)> = store
            .batches()
            .flat_map(|(_, batch)| &batch.records)
            .map(|record| {
                (
                    record.run.aa_flags,
                    record.outline.contours[0].fade_exponent.to_bits(),
                )
            })
            .collect();
        fields.sort_unstable();
        fields
    }

    #[test]
    fn default_lines_across_panels_share_one_level_zero_batch() {
        let mut app = line_batch_app();
        spawn_line_panel(&mut app, None);
        spawn_line_panel(&mut app, None);
        settle(&mut app);

        let store = app.world().resource::<PanelLineBatchStore>();
        assert_eq!(store.batches().count(), 1);
        let Some((_, batch)) = store.batches().next() else {
            panic!("one line batch should exist");
        };
        assert_eq!(batch.record_count(), 2);

        let (z_level, material_depth_bias, material_oit_offset, records) =
            one_line_batch_values(&app);
        let previous_line_lane = (DRAW_LEVEL_GEOMETRY_LANES - 1).to_f32() * LAYER_DEPTH_BIAS;
        assert_eq!(z_level, 0);
        assert_eq!(
            previous_line_lane.to_bits(),
            draw_order::line_batch_depth_bias(0).get().to_bits()
        );
        assert_eq!(
            material_depth_bias.to_bits(),
            draw_order::line_batch_depth_bias(0).get().to_bits()
        );
        assert_eq!(material_oit_offset.to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            records,
            vec![(0.0, 0.0), (0.0, 0.0)],
            "per-record offsets stay in the run table"
        );
    }

    #[test]
    fn line_draw_layers_route_to_matching_level_batches() {
        let mut app = line_batch_app();
        spawn_line_panel(&mut app, Some(DrawLayer(-1)));
        spawn_line_panel(&mut app, Some(DrawLayer(1)));
        settle(&mut app);

        let store = app.world().resource::<PanelLineBatchStore>();
        let mut levels: Vec<i8> = store.batches().map(|(key, _)| key.z_level).collect();
        levels.sort_unstable();
        assert_eq!(levels, vec![-1, 1]);

        let materials = app.world().resource::<Assets<TextMaterial>>();
        let mut depth_biases: Vec<(i8, u32)> = store
            .batches()
            .map(|(key, batch)| {
                let Some(gpu) = batch.gpu.as_ref() else {
                    panic!("line batch should have GPU assets");
                };
                let Some(material) = materials.get(&gpu.material) else {
                    panic!("line batch material should exist");
                };
                (key.z_level, material.base.depth_bias.to_bits())
            })
            .collect();
        depth_biases.sort_by_key(|(z_level, _)| *z_level);
        assert_eq!(
            depth_biases,
            vec![
                (-1, draw_order::line_batch_depth_bias(-1).get().to_bits()),
                (1, draw_order::line_batch_depth_bias(1).get().to_bits()),
            ],
        );
    }

    /// Phase C acceptance: an element-level AA override renders with its own
    /// mode while its sibling keeps the inherited mode without increasing the
    /// batch count, and a global `AntiAlias` / `HairlineWidth::fade`
    /// change applied after the override exists re-packs the non-overridden
    /// run records while the overrides hold.
    #[test]
    fn element_overrides_share_one_batch_and_global_changes_repack() {
        let mut app = line_batch_app();

        // Element 1: no overrides. Element 2: AA Off + fade pinned to Full.
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(60.0))
            .layout(|builder| {
                builder.with(
                    El::new()
                        .size(40.0, 20.0)
                        .draw(PanelDraw::lines([horizontal_line()])),
                    |_| {},
                );
                builder.with(
                    El::new()
                        .size(40.0, 20.0)
                        .anti_alias(AntiAlias::Off)
                        .hairline_fade(HairlineFade::Full)
                        .draw(PanelDraw::lines([horizontal_line()])),
                    |_| {},
                );
            })
            .build()
            .unwrap_or_else(|error| panic!("test panel should build: {error:?}"));
        app.world_mut().spawn(panel);
        for _ in 0..3 {
            app.update();
        }

        {
            let store = app.world().resource::<PanelLineBatchStore>();
            assert_eq!(
                store.batches().count(),
                1,
                "an element AA override must not split the batch"
            );
            assert_eq!(
                sorted_run_fields(store),
                vec![
                    (AntiAlias::Off.aa_flags(), 0.0_f32.to_bits()),
                    (AntiAlias::Both.aa_flags(), 0.0_f32.to_bits()),
                ],
                "one run carries the element override, the other the global default"
            );
        }

        // Global changes after the overrides exist: the cascade-root sync +
        // propagation re-resolve the panel, and `Changed<Resolved<A>>`
        // re-packs the run records.
        *app.world_mut().resource_mut::<AntiAlias>() = AntiAlias::Anisotropic;
        app.world_mut().resource_mut::<HairlineWidth>().fade = HairlineFade::Fade { exponent: 1.5 };
        for _ in 0..3 {
            app.update();
        }

        let store = app.world().resource::<PanelLineBatchStore>();
        assert_eq!(
            sorted_run_fields(store),
            vec![
                (AntiAlias::Off.aa_flags(), 0.0_f32.to_bits()),
                (AntiAlias::Anisotropic.aa_flags(), 1.5_f32.to_bits()),
            ],
            "the non-overridden run re-packs to the new globals; the element overrides hold"
        );
    }

    /// Phase D acceptance: the typography overlay rebuilds its guide panels
    /// by despawning and respawning them on every metric refresh, so the
    /// panel-entity portion of every batch key churns. Repeated recreation
    /// must leave zero records keyed to a dead panel entity.
    #[test]
    fn recreated_guide_panels_leave_no_stale_records() {
        fn spawn_guide_panel(app: &mut App, offset: f32) -> Entity {
            let panel = DiegeticPanel::world()
                .size(Mm(100.0), Mm(60.0))
                .layout(move |builder| {
                    builder.with(
                        El::new()
                            .size(40.0, 20.0)
                            .hairline_fade(HairlineFade::Full)
                            .draw(PanelDraw::lines([
                                PanelLine::new((0.0, offset), (20.0, offset))
                                    .width(0.4)
                                    .color(Color::WHITE),
                                PanelLine::new((offset, 0.0), (offset, 15.0))
                                    .width(0.4)
                                    .color(Color::WHITE),
                            ])),
                        |_| {},
                    );
                })
                .build()
                .unwrap_or_else(|error| panic!("guide panel should build: {error:?}"));
            app.world_mut().spawn(panel).id()
        }

        let mut app = line_batch_app();
        let mut current = spawn_guide_panel(&mut app, 5.0);
        for _ in 0..3 {
            app.update();
        }

        for refresh in 0..5 {
            app.world_mut().entity_mut(current).despawn();
            current = spawn_guide_panel(&mut app, 4.0 + refresh.to_f32());
            for _ in 0..3 {
                app.update();
            }

            let store = app.world().resource::<PanelLineBatchStore>();
            let stale_records: Vec<Entity> = store
                .batches()
                .flat_map(|(_, batch)| &batch.records)
                .map(|record| record.key.panel)
                .filter(|panel| *panel != current)
                .collect();
            assert!(
                stale_records.is_empty(),
                "refresh {refresh}: records keyed to dead panels: {stale_records:?}"
            );

            let stale_index: Vec<Entity> = store
                .panel_index
                .keys()
                .copied()
                .filter(|panel| *panel != current)
                .collect();
            assert!(
                stale_index.is_empty(),
                "refresh {refresh}: panel index holds dead panels: {stale_index:?}"
            );
        }

        let store = app.world().resource::<PanelLineBatchStore>();
        let record_count: usize = store.batches().map(|(_, batch)| batch.records.len()).sum();
        assert_eq!(
            record_count, 1,
            "the surviving panel's guide lines must stay batched as one merged record"
        );
    }

    #[test]
    fn line_ordinal_contributes_to_depth_inside_grouped_command() {
        let first = test_line(0);
        let second = test_line(1);
        let command_bounds = first.visual_bounds();
        let commands = vec![RenderCommand {
            bounds:      command_bounds,
            element_idx: 0,
            kind:        RenderCommandKind::Lines {
                lines: vec![first.clone(), second.clone()],
            },
            z_index:     None,
        }];
        let draw_depth = draw_depth_for_command(&commands, 0);
        let first_source = LinePrimitiveSource {
            element_index: 0,
            draw_depth,
            line: &first,
            primitive: &first.primitives()[0],
        };
        let second_source = LinePrimitiveSource {
            element_index: 0,
            draw_depth,
            line: &second,
            primitive: &second.primitives()[0],
        };

        assert!(primitive_depth_bias(&second_source) > primitive_depth_bias(&first_source));
        assert!(
            primitive_oit_depth_offset(&second_source) > primitive_oit_depth_offset(&first_source)
        );
    }

    #[test]
    fn collect_line_primitives_preserves_resolved_line_clip() {
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
            start: Vec2::ZERO,
            end: Vec2::new(50.0, 0.0),
            shaft_start: Vec2::ZERO,
            shaft_end: Vec2::new(50.0, 0.0),
            width: 2.0,
            color: Color::WHITE,
            hairline_fade: None,
            primitives: vec![primitive],
        };
        let commands = vec![RenderCommand {
            bounds:      primitive_bounds,
            element_idx: 1,
            kind:        RenderCommandKind::Lines { lines: vec![line] },
            z_index:     None,
        }];

        let projection = DrawOrderProjection::from_commands(&commands);
        let collected = collect_line_primitives(&commands, &projection);

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
                lighting: Lighting::Lit,
                sidedness: Sidedness::DoubleSided,
                shadow: VisualShadow::Cast,
                layers: BatchRenderLayers(RenderLayers::layer(0)),
            },
            0,
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
                aa_flags:         AntiAlias::Both.aa_flags(),
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
                min_feature:   0.0,
                fade_exponent: 0.0,
                segments:      vec![
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
            start: Vec2::ZERO,
            end: Vec2::X,
            shaft_start: Vec2::ZERO,
            shaft_end: Vec2::X,
            width: 1.0,
            color: Color::WHITE,
            hairline_fade: None,
            primitives: vec![primitive],
        }
    }

    fn draw_depth_for_command(
        commands: &[RenderCommand],
        command_index: usize,
    ) -> DrawCommandDepth {
        let projection = DrawOrderProjection::from_commands(commands);
        match projection.depth_for(command_index) {
            Some(draw_depth) => draw_depth,
            None => panic!("line command should receive draw depth"),
        }
    }
}

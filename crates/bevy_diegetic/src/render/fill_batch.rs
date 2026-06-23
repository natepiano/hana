//! Batched SDF fill route backed by the frame material table.
//!
//! Builds typed SDF fill/border records and their material-table rows in one
//! frame-local pass. Production panel backgrounds, borders, and divider
//! rectangles render through visible `DiegeticSdfFillBatch` entities.

use std::collections::HashMap;
use std::collections::HashSet;

use bevy::asset::Asset;
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::visibility::VisibilitySystems;
use bevy::light::NotShadowCaster;
use bevy::math::Vec3A;
use bevy::mesh::Indices;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MATERIAL_BIND_GROUP_INDEX;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialExtensionKey;
use bevy::pbr::MaterialExtensionPipeline;
use bevy::pbr::MaterialPlugin;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy::render::render_resource::ShaderSize;
use bevy::render::render_resource::ShaderType;
use bevy::render::render_resource::SpecializedMeshPipelineError;
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use bevy::transform::TransformSystems;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::BatchAlphaMode;
use super::BatchRenderLayers;
use super::CommandIndex;
use super::Dirty;
use super::batch_key;
use super::batch_key::PipelineCompatibility;
use super::batch_key::ResourceCompatibility;
use super::batch_key::VisualShadow;
use super::draw_order;
use super::draw_order::DrawCommandDepth;
use super::material;
use super::material_table;
use super::material_table::BatchResourcesReady;
use super::material_table::FrameMaterialTableBuilder;
use super::material_table::GpuMaterialSlotId;
use super::material_table::MaterialSlotAppend;
use super::material_table::MaterialSlotAppended;
use super::material_table::MaterialSlotCandidate;
use super::material_table::MaterialSlotInput;
use super::material_table::MaterialTableAppendReady;
#[cfg(test)]
use super::material_table::SdfDriverRunOrder;
use super::material_table::SdfPaintMaterial;
use super::panel_geometry::ResolvedSdfSurface;
use super::panel_geometry::ResolvedSdfSurfaceRegistry;
use crate::cascade::CascadeDefault;
use crate::cascade::SdfMaterial;
use crate::constants::EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;

/// Extra `depth_nudge` layer-units pushing an opaque/alpha-mask SDF fill away
/// from the camera so coplanar panel text, drawn at its true depth, wins the
/// depth test instead of z-fighting the fill. Only the depth-buffer regime
/// (`Opaque`/`Mask`) applies it; transparent and OIT fills order through
/// `ScreenDepthBias`/`OitDepthOffset` and get zero here. Starts far below one
/// layer-unit; raise by powers of ten until text shows without the fill
/// visibly floating off the surface.
const OPAQUE_FILL_DEPTH_PUSH_LAYERS: f32 = 1.0;

/// Render material used by batched SDF fill and border records.
pub(crate) type SdfExtendedMaterial = ExtendedMaterial<StandardMaterial, SdfExtension>;

/// Marker inserted on private SDF fill batch entities, BRP-inspectable.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(crate) struct DiegeticSdfFillBatch;

/// BRP-inspectable snapshot of one built GPU SDF record's resolved fields.
#[derive(Clone, Copy, Debug, Reflect)]
pub(crate) struct SdfRecordSnapshot {
    /// `SdfRenderRecord::fill_material` slot id (`u32::MAX` when not authored).
    pub fill_material:   u32,
    /// `SdfRenderRecord::border_material` slot id (`u32::MAX` when not authored).
    pub border_material: u32,
    /// `SdfRenderRecord::paint_mask` role bits.
    pub paint_mask:      u32,
    /// SDF rounded-rect half-size.
    pub half_size:       Vec2,
    /// Quad half-size including AA padding.
    pub mesh_half_size:  Vec2,
    /// SDF fragment flags.
    pub flags:           u32,
}

/// Per-frame diagnostic snapshot of built SDF GPU records, BRP-inspectable.
#[derive(Debug, Default, Reflect, Resource)]
#[reflect(Resource)]
pub(crate) struct SdfRecordDiagnostics {
    /// Live (non-padding) record snapshots committed this frame.
    pub records: Vec<SdfRecordSnapshot>,
}

/// One SDF role inside a resolved panel surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SdfMaterialRole {
    /// Panel background or element fill.
    Fill,
    /// Panel or element border ring.
    Border,
}

/// Material source identity for one SDF fill or border role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SdfMaterialSourceKey {
    /// Panel entity whose command stream produced this role.
    pub panel:         Entity,
    /// Command index of the resolved SDF surface.
    pub command_index: CommandIndex,
    /// Role within the resolved SDF surface.
    pub role:          SdfMaterialRole,
}

/// Append-time source material for one SDF material-table role.
pub(crate) struct SdfMaterialSlotInput<'a> {
    /// Source identity returned with the appended frame-local slot.
    pub key:            SdfMaterialSourceKey,
    /// Resolved `StandardMaterial` source; defaults are folded before this input exists.
    pub base_material:  &'a StandardMaterial,
    /// Layout color override for `StandardMaterial::base_color`.
    pub color_override: Option<Color>,
}

impl MaterialSlotInput for SdfMaterialSlotInput<'_> {
    type Key = SdfMaterialSourceKey;

    fn key(&self) -> Self::Key { self.key }

    fn material_slot_candidate(&self) -> MaterialSlotCandidate {
        let mut material = self.base_material.clone();
        if let Some(color) = self.color_override {
            material.base_color = color;
        }
        MaterialSlotCandidate {
            values:                 (&material).into(),
            pipeline_compatibility: (&material).into(),
            resource_compatibility: ResourceCompatibility::from(&material),
        }
    }
}

/// Per-command SDF record identity without a fill/border role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SdfRecordKey {
    /// Panel entity whose command stream produced this record.
    pub panel:         Entity,
    /// Command index of the resolved SDF surface.
    pub command_index: CommandIndex,
}

/// Panel-local rounded-rectangle SDF half-size in world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SdfHalfSize {
    /// Half-size value written to `SdfRenderRecord::half_size`.
    pub value: Vec2,
}

/// Panel-local SDF mesh half-size, including `SDF_AA_PADDING`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MeshHalfSize {
    /// Half-size value written to `SdfRenderRecord::mesh_half_size`.
    pub value: Vec2,
}

/// Panel-local SDF clip rectangle after AA padding expansion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalClipRect {
    /// `[left, bottom, right, top]` clip rectangle.
    pub value: Vec4,
}

/// Panel-local per-corner SDF radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CornerRadii {
    /// `[top_left, top_right, bottom_right, bottom_left]` radii.
    pub value: Vec4,
}

/// Panel-local per-side SDF border widths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BorderWidths {
    /// `[top, right, bottom, left]` widths.
    pub value: Vec4,
}

/// Fill and border role-presence bits read by `sdf_panel.wgsl`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, ShaderType)]
pub(crate) struct SdfPaintMask {
    /// Bit field written to `SdfRenderRecord::paint_mask`.
    bits: u32,
}

impl SdfPaintMask {
    /// Fill role bit in `SdfPaintMask`.
    pub(crate) const FILL: u32 = 1;
    /// Border role bit in `SdfPaintMask`.
    pub(crate) const BORDER: u32 = 1 << 1;

    /// Builds role bits from CPU-side SDF material presence.
    #[must_use]
    pub(crate) const fn from_materials(fill: SdfPaintMaterial, border: SdfPaintMaterial) -> Self {
        let fill_bit = match fill {
            SdfPaintMaterial::Authored(_) => Self::FILL,
            SdfPaintMaterial::NotAuthored => 0,
        };
        let border_bit = match border {
            SdfPaintMaterial::Authored(_) => Self::BORDER,
            SdfPaintMaterial::NotAuthored => 0,
        };
        Self {
            bits: fill_bit | border_bit,
        }
    }

    /// Returns the raw bit field written to WGSL.
    #[must_use]
    pub(crate) const fn bits(self) -> u32 { self.bits }

    /// Returns an empty role mask for capacity-tail records.
    #[must_use]
    pub(crate) const fn empty() -> Self { Self { bits: 0 } }
}

/// Per-command monotonic run ordinal used to preserve cross-batch order.
///
/// Example: panel order is `background`, `text`, `border`; `background` and
/// `border` are both SDF-compatible, but they must not merge into one SDF
/// batch because that would draw `background`, `border`, `text`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct ContiguousDrawnRun {
    /// Frame-local run ordinal.
    pub value: u32,
}

/// Key for one SDF fill batch and one `SdfBatchResources` entry.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SdfBatchKey {
    /// Panel entity; batching is panel-scoped, so the union-centroid sort
    /// center stays valid (dropping this field requires a sorted-vs-OIT parity
    /// revisit, per `SdfBatchKey` docs).
    pub panel_entity:           Entity,
    /// Authored z-level for the shared SDF fill sort lane.
    pub z_level:                i8,
    /// Render layers copied from the panel.
    pub layers:                 BatchRenderLayers,
    /// Shadow participation for this batch.
    pub shadow:                 VisualShadow,
    /// Maximal compatible command run in `DrawCommandDepth::ordinal_index()` order.
    pub contiguous_drawn_run:   ContiguousDrawnRun,
    /// Material-derived pipeline facts that must agree inside this SDF draw.
    pub pipeline_compatibility: PipelineCompatibility,
    /// Texture and bind-group facts copied into the SDF render material.
    pub resource_compatibility: ResourceCompatibility,
}

/// Material slots and compatibility values appended for one SDF record.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SdfRecordMaterialSlots {
    /// Fill role material row or `SdfPaintMaterial::NotAuthored`.
    pub fill:                   SdfPaintMaterial,
    /// Border role material row or `SdfPaintMaterial::NotAuthored`.
    pub border:                 SdfPaintMaterial,
    /// Pipeline compatibility selected for the enclosing SDF batch.
    pub pipeline_compatibility: PipelineCompatibility,
    /// Resource compatibility selected for the enclosing SDF batch.
    pub resource_compatibility: ResourceCompatibility,
}

/// CPU-side SDF record retained by `SdfBatchStore`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedSdfBatchRecord {
    /// Per-command SDF record identity.
    pub record_key:       SdfRecordKey,
    /// Fill material source identity.
    pub fill_source:      SdfMaterialSourceKey,
    /// Border material source identity.
    pub border_source:    SdfMaterialSourceKey,
    /// Full draw-depth projection for this command.
    pub draw_depth:       DrawCommandDepth,
    /// Batch key selected while appending frame material rows.
    pub batch_key:        SdfBatchKey,
    /// Panel-local transform from `ResolvedSdfSurface`.
    pub local_transform:  Transform,
    /// World transform written after `TransformSystems::Propagate`.
    pub transform:        Mat4,
    /// Rounded-rectangle SDF half-size.
    pub half_size:        SdfHalfSize,
    /// Mesh half-size including AA padding.
    pub mesh_half_size:   MeshHalfSize,
    /// Per-corner rounded-rectangle radii.
    pub corner_radii:     CornerRadii,
    /// Per-side border widths.
    pub border_widths:    BorderWidths,
    /// Local clip rectangle.
    pub clip_rect:        LocalClipRect,
    /// Fill material table row state.
    pub fill_material:    SdfPaintMaterial,
    /// Border material table row state.
    pub border_material:  SdfPaintMaterial,
    /// Role-present bits derived from `fill_material` and `border_material`.
    pub paint_mask:       SdfPaintMask,
    /// Clip-space depth nudge in layer units for non-OIT views.
    pub depth_nudge:      f32,
    /// Per-record OIT position-z offset for coplanar ordering.
    pub oit_depth_offset: f32,
    /// Extra shader flags for the SDF fragment path.
    pub flags:            u32,
}

impl ResolvedSdfBatchRecord {
    /// Builds a CPU-retained SDF record from one resolved panel-local surface.
    #[must_use]
    pub(crate) fn from_resolved(
        surface: &ResolvedSdfSurface<'_>,
        materials: SdfRecordMaterialSlots,
        contiguous_drawn_run: ContiguousDrawnRun,
    ) -> Self {
        let fill_source = SdfMaterialSourceKey {
            panel:         surface.panel_entity,
            command_index: surface.command_index,
            role:          SdfMaterialRole::Fill,
        };
        let border_source = SdfMaterialSourceKey {
            panel:         surface.panel_entity,
            command_index: surface.command_index,
            role:          SdfMaterialRole::Border,
        };
        let batch_key = SdfBatchKey {
            panel_entity: surface.panel_entity,
            z_level: surface.draw_depth.z_level(),
            layers: BatchRenderLayers(surface.render_layers.clone()),
            shadow: surface.surface_shadow.into(),
            contiguous_drawn_run,
            pipeline_compatibility: materials.pipeline_compatibility,
            resource_compatibility: materials.resource_compatibility.clone(),
        };
        let paint_mask = SdfPaintMask::from_materials(materials.fill, materials.border);
        Self {
            record_key: SdfRecordKey {
                panel:         surface.panel_entity,
                command_index: surface.command_index,
            },
            fill_source,
            border_source,
            draw_depth: surface.draw_depth,
            batch_key,
            local_transform: surface.local_transform,
            transform: surface.local_transform.to_matrix(),
            half_size: SdfHalfSize {
                value: surface.sdf_half_size,
            },
            mesh_half_size: MeshHalfSize {
                value: surface.mesh_half_size,
            },
            corner_radii: CornerRadii {
                value: Vec4::from_array(surface.corner_radii),
            },
            border_widths: BorderWidths {
                value: Vec4::from_array(surface.border_widths),
            },
            clip_rect: LocalClipRect {
                value: surface.clip_rect,
            },
            fill_material: materials.fill,
            border_material: materials.border,
            paint_mask,
            depth_nudge: surface.draw_depth.depth_bias().get()
                - draw_order::fill_batch_depth_bias(surface.draw_depth.z_level()).get()
                - opaque_fill_depth_push(
                    materials.pipeline_compatibility.alpha,
                    surface.surface_shadow.into(),
                ),
            oit_depth_offset: surface.draw_depth.oit_depth_offset().get(),
            flags: 0,
        }
    }

    /// Rewrites `SdfRenderRecord::transform` from the panel's propagated transform.
    pub(crate) fn update_world_transform(&mut self, panel_transform: &GlobalTransform) {
        self.transform = panel_transform.to_matrix() * self.local_transform.to_matrix();
    }
}

/// GPU mirror for one batched SDF fill/border surface.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(crate) struct SdfRenderRecord {
    /// World-space transform used by the vertex shader.
    pub transform:        Mat4,
    /// Rounded-rectangle SDF half-size in local record space.
    pub half_size:        Vec2,
    /// Quad half-size including AA padding in local record space.
    pub mesh_half_size:   Vec2,
    /// Per-corner radii `[top_left, top_right, bottom_right, bottom_left]`.
    pub corner_radii:     Vec4,
    /// Per-side widths `[top, right, bottom, left]`.
    pub border_widths:    Vec4,
    /// Local clip rectangle `[left, bottom, right, top]`.
    pub clip_rect:        Vec4,
    /// Fill material row, or `INVALID_GPU_MATERIAL_SLOT`.
    pub fill_material:    GpuMaterialSlotId,
    /// Border material row, or `INVALID_GPU_MATERIAL_SLOT`.
    pub border_material:  GpuMaterialSlotId,
    /// Role-present bits checked before material-table reads.
    pub paint_mask:       SdfPaintMask,
    /// Extra shader flags for SDF rendering.
    pub flags:            u32,
    /// Clip-space depth nudge in layer units for non-OIT views.
    pub depth_nudge:      f32,
    /// Per-record OIT position-z offset for coplanar ordering.
    pub oit_depth_offset: f32,
}

impl SdfRenderRecord {
    /// Converts the CPU retained record to the GPU storage-buffer mirror.
    #[must_use]
    pub(crate) const fn from_resolved(record: &ResolvedSdfBatchRecord) -> Self {
        Self {
            transform:        record.transform,
            half_size:        record.half_size.value,
            mesh_half_size:   record.mesh_half_size.value,
            corner_radii:     record.corner_radii.value,
            border_widths:    record.border_widths.value,
            clip_rect:        record.clip_rect.value,
            fill_material:    record.fill_material.to_gpu(),
            border_material:  record.border_material.to_gpu(),
            paint_mask:       record.paint_mask,
            flags:            record.flags,
            depth_nudge:      record.depth_nudge,
            oit_depth_offset: record.oit_depth_offset,
        }
    }

    /// Capacity-tail record that collapses before any material-table read.
    #[must_use]
    pub(crate) const fn padded() -> Self {
        Self {
            transform:        Mat4::ZERO,
            half_size:        Vec2::ZERO,
            mesh_half_size:   Vec2::ZERO,
            corner_radii:     Vec4::ZERO,
            border_widths:    Vec4::ZERO,
            clip_rect:        Vec4::ZERO,
            fill_material:    SdfPaintMaterial::NotAuthored.to_gpu(),
            border_material:  SdfPaintMaterial::NotAuthored.to_gpu(),
            paint_mask:       SdfPaintMask::empty(),
            flags:            0,
            depth_nudge:      0.0,
            oit_depth_offset: 0.0,
        }
    }
}

/// Placeholder SDF mesh table row bound at `SDF_MESH_BINDING`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub(crate) struct SdfMeshRecord {
    /// Reserved data lane for future mesh-level SDF batch metadata.
    pub reserved: UVec4,
}

const _: () = assert!(SdfRenderRecord::SHADER_SIZE.get() == 160);
const _: () = assert!(SdfMeshRecord::SHADER_SIZE.get() == 16);

/// GPU assets owned by one SDF fill batch.
#[derive(Debug)]
pub(crate) struct SdfBatchResources {
    /// `SdfRenderRecord` storage buffer at binding 107.
    pub records:      Handle<ShaderBuffer>,
    /// Reserved SDF mesh-record storage buffer at binding 108.
    pub mesh_records: Handle<ShaderBuffer>,
    /// Inert capacity-sized mesh; replaced on capacity growth.
    pub mesh:         Handle<Mesh>,
    /// Batch render material.
    pub material:     Handle<SdfExtendedMaterial>,
    /// Record capacity represented by `records`, `mesh_records`, and `mesh`.
    pub capacity:     u32,
}

/// CPU record set and GPU handles for one `SdfBatchKey`.
#[derive(Debug, Default)]
pub(crate) struct SdfBatch {
    /// Batch render entity; `None` before the first GPU allocation.
    pub entity:        Option<Entity>,
    /// GPU handles; `None` before the first GPU allocation.
    pub gpu:           Option<SdfBatchResources>,
    /// Record-buffer upload state for this batch.
    pub record_upload: Dirty,
    /// Bounds recomputation state for this batch.
    pub bounds_update: Dirty,
    records:           Vec<ResolvedSdfBatchRecord>,
}

impl SdfBatch {
    /// CPU records in upload order.
    #[must_use]
    pub(crate) fn records(&self) -> &[ResolvedSdfBatchRecord] { &self.records }

    /// Number of live SDF records in this batch.
    #[must_use]
    pub(crate) fn record_count(&self) -> u32 { self.records.len().to_u32() }

    /// Whether this batch has no live SDF records.
    #[must_use]
    pub(crate) const fn is_empty(&self) -> bool { self.records.is_empty() }

    fn position_of(&self, key: SdfRecordKey) -> Option<usize> {
        self.records
            .iter()
            .position(|record| record.record_key == key)
    }

    fn sort_records(&mut self) {
        self.records.sort_by(|left, right| {
            left.draw_depth
                .ordinal_index()
                .cmp(&right.draw_depth.ordinal_index())
                .then(
                    left.record_key
                        .command_index
                        .cmp(&right.record_key.command_index),
                )
        });
    }

    fn upsert_record(&mut self, record: ResolvedSdfBatchRecord) {
        if let Some(position) = self.position_of(record.record_key) {
            if self.records[position] == record {
                return;
            }
            self.records[position] = record;
        } else {
            self.records.push(record);
        }
        self.sort_records();
        self.record_upload.mark();
        self.bounds_update.mark();
    }

    fn remove_record(&mut self, key: SdfRecordKey) {
        if let Some(position) = self.position_of(key) {
            self.records.remove(position);
            self.record_upload.mark();
            self.bounds_update.mark();
        }
    }

    /// World-space union from clipped record corners using the R14 recipe.
    #[must_use]
    pub(crate) fn world_bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::MAX;
        let mut max = Vec3::MIN;
        let mut any = false;
        for record in &self.records {
            let Some((local_min, local_max)) =
                clipped_local_bounds(record.mesh_half_size, record.clip_rect)
            else {
                continue;
            };
            for corner in clipped_corners(local_min, local_max) {
                let world = record.transform * Vec4::new(corner.x, corner.y, 0.0, 1.0);
                min = min.min(world.xyz());
                max = max.max(world.xyz());
                any = true;
            }
        }
        any.then_some((min, max))
    }
}

/// Store that maps SDF records to `SdfBatchKey` batches.
#[derive(Debug, Default, Resource)]
pub(crate) struct SdfBatchStore {
    /// Batch entries keyed by compatibility and ordering fields.
    batches:      HashMap<SdfBatchKey, SdfBatch>,
    /// Current batch key for each routed SDF record.
    record_index: HashMap<SdfRecordKey, SdfBatchKey>,
}

impl SdfBatchStore {
    /// Inserts or moves one retained SDF record.
    pub(crate) fn upsert_record(&mut self, record: ResolvedSdfBatchRecord) {
        let record_key = record.record_key;
        let batch_key = record.batch_key.clone();
        if let Some(current) = self.record_index.get(&record_key) {
            if *current == batch_key {
                if let Some(batch) = self.batches.get_mut(&batch_key) {
                    batch.upsert_record(record);
                }
                return;
            }
            let previous = current.clone();
            if let Some(batch) = self.batches.get_mut(&previous) {
                batch.remove_record(record_key);
            }
            self.record_index.remove(&record_key);
        }
        self.batches
            .entry(batch_key.clone())
            .or_default()
            .upsert_record(record);
        self.record_index.insert(record_key, batch_key);
    }

    /// Removes one SDF record from its batch.
    pub(crate) fn remove_record(&mut self, record_key: SdfRecordKey) {
        let Some(batch_key) = self.record_index.remove(&record_key) else {
            return;
        };
        if let Some(batch) = self.batches.get_mut(&batch_key) {
            batch.remove_record(record_key);
        }
    }

    /// Removes every retained SDF record absent from the current append pass.
    pub(crate) fn retain_records(&mut self, active: &HashSet<SdfRecordKey>) {
        let stale: Vec<SdfRecordKey> = self
            .record_index
            .keys()
            .copied()
            .filter(|record_key| !active.contains(record_key))
            .collect();
        for record_key in stale {
            self.remove_record(record_key);
        }
    }

    /// All SDF batches.
    pub(crate) fn batches(&self) -> impl Iterator<Item = (&SdfBatchKey, &SdfBatch)> {
        self.batches.iter()
    }

    /// All SDF batches, mutable.
    pub(crate) fn batches_mut(&mut self) -> impl Iterator<Item = (&SdfBatchKey, &mut SdfBatch)> {
        self.batches.iter_mut()
    }

    /// One SDF batch by key, mutable.
    pub(crate) fn get_mut(&mut self, key: &SdfBatchKey) -> Option<&mut SdfBatch> {
        self.batches.get_mut(key)
    }

    /// Drops empty batch entries, returning their entities for despawn.
    pub(crate) fn take_empty_batches(&mut self) -> Vec<Entity> {
        let empty: Vec<SdfBatchKey> = self
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
}

/// How an SDF batch pipeline sources its per-quad geometry.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub(crate) enum SdfPipelineMode {
    /// Pulls per-quad geometry from the record storage buffer in the vertex
    /// stage (`sdf_panel.wgsl`); the only production mode.
    VertexPulled,
    /// Reads geometry from mesh vertex attributes instead of pulling it.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "non-pulled geometry mode is not produced yet")
    )]
    MeshAttributes,
}

/// SDF material extension over `StandardMaterial`.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
#[bind_group_data(SdfExtensionKey)]
pub(crate) struct SdfExtension {
    /// SDF render records read by vertex and fragment stages.
    #[storage(107, read_only, visibility(vertex, fragment))]
    records:        Handle<ShaderBuffer>,
    /// Shared `MaterialSlotValues` table read by SDF fragments.
    #[storage(106, read_only, visibility(fragment))]
    material_table: Handle<ShaderBuffer>,
    /// Reserved SDF mesh metadata table, present for binding-layout parity.
    #[storage(108, read_only, visibility(vertex))]
    mesh_records:   Handle<ShaderBuffer>,
    /// Geometry-sourcing mode for `sdf_panel.wgsl`.
    pipeline_mode:  SdfPipelineMode,
}

/// Pipeline-specialization key for `SdfExtension`.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct SdfExtensionKey {
    /// Mirror of `SdfExtension::pipeline_mode`.
    pipeline_mode: SdfPipelineMode,
}

impl From<&SdfExtension> for SdfExtensionKey {
    fn from(extension: &SdfExtension) -> Self {
        Self {
            pipeline_mode: extension.pipeline_mode,
        }
    }
}

impl MaterialExtension for SdfExtension {
    fn vertex_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH.into() }

    fn fragment_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH.into() }

    fn prepass_vertex_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH.into() }

    fn prepass_fragment_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH.into() }

    fn deferred_vertex_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH.into() }

    fn deferred_fragment_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH.into() }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        add_sdf_stripped_material_group_def(descriptor, key.bind_group_data.pipeline_mode);
        Ok(())
    }
}

fn add_sdf_stripped_material_group_def(
    descriptor: &mut RenderPipelineDescriptor,
    pipeline_mode: SdfPipelineMode,
) {
    if matches!(pipeline_mode, SdfPipelineMode::VertexPulled)
        && material_group_is_stripped(descriptor)
    {
        descriptor
            .vertex
            .shader_defs
            .push("SDF_STRIPPED_MATERIAL_GROUP".into());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment
                .shader_defs
                .push("SDF_STRIPPED_MATERIAL_GROUP".into());
        }
    }
}

fn material_group_is_stripped(descriptor: &RenderPipelineDescriptor) -> bool {
    descriptor
        .layout
        .get(MATERIAL_BIND_GROUP_INDEX)
        .is_none_or(|material_layout| material_layout.entries.is_empty())
}

/// Inputs for one `SdfExtendedMaterial` batch asset.
pub(crate) struct SdfBatchMaterialInput {
    /// SDF batch key whose compatibility fields configure the material.
    pub key:          SdfBatchKey,
    /// SDF render-record buffer.
    pub records:      Handle<ShaderBuffer>,
    /// Reserved SDF mesh-record buffer.
    pub mesh_records: Handle<ShaderBuffer>,
}

/// Builds the render material for one SDF batch.
#[must_use]
pub(crate) fn sdf_batch_material(input: SdfBatchMaterialInput) -> SdfExtendedMaterial {
    let SdfBatchMaterialInput {
        key,
        records,
        mesh_records,
    } = input;
    let mut base = batch_key::apply_resource_compatibility_to_standard_material(
        &material::default_panel_material(),
        &key.resource_compatibility,
    );
    base.alpha_mode = sdf_batch_alpha_mode(key.pipeline_compatibility.alpha, key.shadow);
    base.double_sided = key.pipeline_compatibility.double_sided;
    base.cull_mode = key.pipeline_compatibility.cull_mode;
    base.unlit = key.pipeline_compatibility.unlit;
    base.fog_enabled = key.pipeline_compatibility.fog_enabled;
    base.opaque_render_method = key.pipeline_compatibility.opaque_render_method.into();
    base.deferred_lighting_pass_id = key.pipeline_compatibility.deferred_lighting_pass_id;
    base.depth_bias = draw_order::fill_batch_depth_bias(key.z_level).get();
    ExtendedMaterial {
        base,
        extension: SdfExtension {
            records,
            material_table: Handle::default(),
            mesh_records,
            pipeline_mode: SdfPipelineMode::VertexPulled,
        },
    }
}

/// SDF batch alpha-mode rule for authored source materials.
///
/// Table-row base-color alpha controls normal transparent composition inside
/// `sdf_panel.wgsl`. In prepass/shadow pipelines, `fill_alpha >
/// 0.001` is read from the retained material table when a fill role is present;
/// border-only records skip the fill row and route the transparent interior as
/// empty. `Opaque`, `Mask`, `Blend`, `Premultiplied`, `Add`, `Multiply`, and
/// `AlphaToCoverage` remain stored in `PipelineCompatibility`. A shadow-casting
/// opaque batch maps to `Mask(0.0)` so Bevy keeps the material bind group on the
/// shadow/prepass pipeline (the SDF prepass shader reads the table for
/// `fill_alpha`); a non-casting opaque batch enters no prepass and stays
/// `Opaque`, rendering in the opaque phase like a normal Bevy mesh.
#[must_use]
pub(crate) fn sdf_batch_alpha_mode(alpha: BatchAlphaMode, shadow: VisualShadow) -> AlphaMode {
    match (AlphaMode::from(alpha), shadow) {
        (AlphaMode::Opaque, VisualShadow::Cast) => AlphaMode::Mask(0.0),
        (mode, _) => mode,
    }
}

/// Extra `depth_nudge` layer-units to push the SDF fill away from the camera,
/// non-zero only for the depth-buffer regime (`Opaque`/`Mask`). Transparent and
/// OIT fills order through their sort/list levers, so they get zero.
#[must_use]
fn opaque_fill_depth_push(alpha: BatchAlphaMode, shadow: VisualShadow) -> f32 {
    match sdf_batch_alpha_mode(alpha, shadow) {
        AlphaMode::Opaque | AlphaMode::Mask(_) => OPAQUE_FILL_DEPTH_PUSH_LAYERS,
        _ => 0.0,
    }
}

/// Repoints an SDF batch material at the current frame material table buffer.
pub(crate) fn set_sdf_material_table_buffer(
    material: &mut SdfExtendedMaterial,
    material_table: Handle<ShaderBuffer>,
) {
    material.extension.material_table = material_table;
}

/// Repoints an SDF batch material at replacement record buffers after growth.
pub(crate) fn set_sdf_material_record_buffers(
    material: &mut SdfExtendedMaterial,
    records: Handle<ShaderBuffer>,
    mesh_records: Handle<ShaderBuffer>,
) {
    material.extension.records = records;
    material.extension.mesh_records = mesh_records;
}

/// Appends fill and border rows for one resolved SDF surface atomically.
pub(crate) fn append_sdf_record_materials(
    builder: &mut FrameMaterialTableBuilder,
    surface: &ResolvedSdfSurface<'_>,
    materials: &Assets<StandardMaterial>,
    asset_server: &AssetServer,
    default_material: &CascadeDefault<SdfMaterial>,
) -> Option<SdfRecordMaterialSlots> {
    let rollback_row_count = builder.row_count();
    let fill = append_sdf_role(
        builder,
        surface,
        SdfMaterialRole::Fill,
        materials,
        asset_server,
        default_material,
    );
    let border = append_sdf_role(
        builder,
        surface,
        SdfMaterialRole::Border,
        materials,
        asset_server,
        default_material,
    );
    match (fill, border) {
        (SdfRoleAppend::DroppedLimit, _) | (_, SdfRoleAppend::DroppedLimit) => {
            builder.truncate_rows(rollback_row_count);
            builder.record_dropped_limit();
            None
        },
        (SdfRoleAppend::Held, _) | (_, SdfRoleAppend::Held) => {
            builder.truncate_rows(rollback_row_count);
            None
        },
        (SdfRoleAppend::NotAuthored, SdfRoleAppend::NotAuthored) => None,
        (fill, border) => {
            SdfRecordMaterialSlots::from_appended(fill.into_appended(), border.into_appended())
        },
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SdfRoleAppend {
    NotAuthored,
    Appended(MaterialSlotAppended<SdfMaterialSourceKey>),
    Held,
    DroppedLimit,
}

impl SdfRoleAppend {
    fn into_appended(self) -> Option<MaterialSlotAppended<SdfMaterialSourceKey>> {
        match self {
            Self::Appended(appended) => Some(appended),
            Self::NotAuthored | Self::Held | Self::DroppedLimit => None,
        }
    }
}

impl SdfRecordMaterialSlots {
    fn from_appended(
        fill: Option<MaterialSlotAppended<SdfMaterialSourceKey>>,
        border: Option<MaterialSlotAppended<SdfMaterialSourceKey>>,
    ) -> Option<Self> {
        let fill_material = fill
            .as_ref()
            .map_or(SdfPaintMaterial::NotAuthored, |appended| {
                SdfPaintMaterial::Authored(appended.slot)
            });
        let border_material = border
            .as_ref()
            .map_or(SdfPaintMaterial::NotAuthored, |appended| {
                SdfPaintMaterial::Authored(appended.slot)
            });
        let compatibility = fill.as_ref().or(border.as_ref())?;
        if let (Some(fill), Some(border)) = (&fill, &border) {
            debug_assert_eq!(
                fill.pipeline_compatibility, border.pipeline_compatibility,
                "one SDF record cannot bind two different pipeline policies"
            );
            debug_assert_eq!(
                fill.resource_compatibility, border.resource_compatibility,
                "one SDF record cannot bind two different texture/resource sets"
            );
        }
        Some(Self {
            fill:                   fill_material,
            border:                 border_material,
            pipeline_compatibility: compatibility.pipeline_compatibility,
            resource_compatibility: compatibility.resource_compatibility.clone(),
        })
    }
}

fn append_sdf_role(
    builder: &mut FrameMaterialTableBuilder,
    surface: &ResolvedSdfSurface<'_>,
    role: SdfMaterialRole,
    materials: &Assets<StandardMaterial>,
    asset_server: &AssetServer,
    default_material: &CascadeDefault<SdfMaterial>,
) -> SdfRoleAppend {
    let material = match role {
        SdfMaterialRole::Fill => &surface.fill_material,
        SdfMaterialRole::Border => &surface.border_material,
    };
    if !material.authorship.is_authored() {
        return SdfRoleAppend::NotAuthored;
    }
    if !builder.has_remaining_rows(1) {
        return SdfRoleAppend::DroppedLimit;
    }
    let handle = material
        .base_material
        .cloned()
        .unwrap_or_else(|| default_material.0.0.clone());
    let Some(base_material) =
        material::material_asset_for_frame(materials, asset_server, &handle, &default_material.0.0)
    else {
        return SdfRoleAppend::Held;
    };
    let input = SdfMaterialSlotInput {
        key: SdfMaterialSourceKey {
            panel: surface.panel_entity,
            command_index: surface.command_index,
            role,
        },
        base_material,
        color_override: material.color,
    };
    match material_table::append_material_slot(builder, &input) {
        MaterialSlotAppend::Appended(appended) => SdfRoleAppend::Appended(appended),
        MaterialSlotAppend::DroppedLimit => SdfRoleAppend::DroppedLimit,
    }
}

/// Assigns contiguous run ordinals to already-sorted SDF records.
pub(crate) fn assign_contiguous_runs(
    records: Vec<(SdfRecordMaterialSlots, &ResolvedSdfSurface<'_>)>,
) -> Vec<ResolvedSdfBatchRecord> {
    let mut sorted = records;
    sorted.sort_by(|(_, left), (_, right)| {
        left.draw_depth
            .ordinal_index()
            .cmp(&right.draw_depth.ordinal_index())
            .then(left.command_index.cmp(&right.command_index))
    });

    let mut output = Vec::with_capacity(sorted.len());
    let mut previous = None;
    let mut run = ContiguousDrawnRun::default();
    for (materials, surface) in sorted {
        let compatibility = SdfRunCompatibility::from_surface(surface, &materials);
        if previous
            .as_ref()
            .is_some_and(|previous| previous != &compatibility)
        {
            run.value = run.value.saturating_add(1);
        }
        previous = Some(compatibility);
        output.push(ResolvedSdfBatchRecord::from_resolved(
            surface, materials, run,
        ));
    }
    output
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SdfRunCompatibility {
    panel_entity:           Entity,
    z_level:                i8,
    layers:                 BatchRenderLayers,
    shadow:                 VisualShadow,
    pipeline_compatibility: PipelineCompatibility,
    resource_compatibility: ResourceCompatibility,
}

impl SdfRunCompatibility {
    fn from_surface(surface: &ResolvedSdfSurface<'_>, materials: &SdfRecordMaterialSlots) -> Self {
        Self {
            panel_entity:           surface.panel_entity,
            z_level:                surface.draw_depth.z_level(),
            layers:                 BatchRenderLayers(surface.render_layers.clone()),
            shadow:                 surface.surface_shadow.into(),
            pipeline_compatibility: materials.pipeline_compatibility,
            resource_compatibility: materials.resource_compatibility.clone(),
        }
    }
}

/// Render plugin for the SDF fill batch material type.
pub(super) struct FillBatchPlugin;

impl Plugin for FillBatchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SdfBatchStore>()
            .init_resource::<DiegeticPerfStats>()
            .init_resource::<SdfRecordDiagnostics>()
            .add_plugins(MaterialPlugin::<SdfExtendedMaterial>::default())
            .add_systems(
                PostUpdate,
                route_sdf_batch_records
                    .after(material_table::clear_frame_material_table)
                    .in_set(MaterialTableAppendReady),
            )
            .add_systems(
                PostUpdate,
                update_sdf_batch_world_transforms.after(TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                reconcile_sdf_batch_entities
                    .after(update_sdf_batch_world_transforms)
                    .before(VisibilitySystems::CalculateBounds)
                    .in_set(BatchResourcesReady),
            )
            .add_systems(
                PostUpdate,
                material_table::register_sdf_batch_materials::<DiegeticSdfFillBatch>
                    .after(reconcile_sdf_batch_entities)
                    .in_set(BatchResourcesReady),
            )
            .add_systems(
                PostUpdate,
                update_sdf_batch_bounds
                    .after(material_table::register_sdf_batch_materials::<DiegeticSdfFillBatch>)
                    .after(VisibilitySystems::CalculateBounds)
                    .before(VisibilitySystems::CheckVisibility)
                    .in_set(BatchResourcesReady),
            )
            .add_systems(
                PostUpdate,
                commit_sdf_batch_buffers
                    .after(material_table::register_sdf_batch_materials::<DiegeticSdfFillBatch>)
                    .after(update_sdf_batch_bounds)
                    .after(VisibilitySystems::CheckVisibility)
                    .in_set(BatchResourcesReady),
            );
    }
}

fn route_sdf_batch_records(
    #[cfg(test)] mut run_order: Option<ResMut<SdfDriverRunOrder>>,
    mut build: ResMut<material_table::FrameMaterialTableBuild>,
    mut surfaces: ResMut<ResolvedSdfSurfaceRegistry>,
    standard_materials: Res<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    sdf_material_default: Res<CascadeDefault<SdfMaterial>>,
    panels: Query<(&DiegeticPanel, Option<&RenderLayers>, Option<&Visibility>)>,
    mut store: ResMut<SdfBatchStore>,
) {
    #[cfg(test)]
    material_table::record_sdf_driver_run(&mut run_order, material_table::SDF_DRIVER_ROUTE_RESOLVE);

    let mut resolved_surfaces = Vec::new();
    let mut active_records = HashSet::new();
    let mut stale_panels = HashSet::new();
    for stored_surface in surfaces.surfaces() {
        let Ok((panel, panel_layers, panel_visibility)) = panels.get(stored_surface.panel_entity())
        else {
            stale_panels.insert(stored_surface.panel_entity());
            continue;
        };
        if matches!(panel_visibility, Some(Visibility::Hidden)) {
            continue;
        }
        resolved_surfaces.push(stored_surface.as_resolved(
            panel_layers.cloned().unwrap_or(RenderLayers::layer(0)),
            panel.surface_shadow(),
        ));
    }
    let mut appended = Vec::new();
    let builder = build.builder_mut();
    for surface in &resolved_surfaces {
        let record_key = SdfRecordKey {
            panel:         surface.panel_entity,
            command_index: surface.command_index,
        };
        if let Some(materials) = append_sdf_record_materials(
            builder,
            surface,
            &standard_materials,
            &asset_server,
            &sdf_material_default,
        ) {
            active_records.insert(record_key);
            appended.push((materials, surface));
        }
    }

    for record in assign_contiguous_runs(appended) {
        store.upsert_record(record);
    }
    store.retain_records(&active_records);
    drop(resolved_surfaces);
    for panel_entity in stale_panels {
        surfaces.remove_panel(panel_entity);
    }
}

fn update_sdf_batch_world_transforms(
    #[cfg(test)] mut run_order: Option<ResMut<SdfDriverRunOrder>>,
    mut store: ResMut<SdfBatchStore>,
    panel_transforms: Query<&GlobalTransform, With<DiegeticPanel>>,
) {
    #[cfg(test)]
    material_table::record_sdf_driver_run(
        &mut run_order,
        material_table::SDF_DRIVER_WORLD_TRANSFORMS,
    );

    for (_, batch) in store.batches_mut() {
        let mut transform_update = Dirty::No;
        for record in &mut batch.records {
            let Ok(panel_transform) = panel_transforms.get(record.record_key.panel) else {
                continue;
            };
            let previous = record.transform;
            record.update_world_transform(panel_transform);
            if record.transform != previous {
                transform_update.mark();
            }
        }
        if transform_update.is_set() {
            batch.record_upload.mark();
            batch.bounds_update.mark();
        }
    }
}

fn reconcile_sdf_batch_entities(
    #[cfg(test)] mut run_order: Option<ResMut<SdfDriverRunOrder>>,
    mut store: ResMut<SdfBatchStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SdfExtendedMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut commands: Commands,
) {
    #[cfg(test)]
    material_table::record_sdf_driver_run(
        &mut run_order,
        material_table::SDF_DRIVER_RECONCILE_SPAWN,
    );

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
        let Some(batch) = store.get_mut(&key) else {
            continue;
        };
        spawn_sdf_batch_entity(
            &key,
            batch,
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut storage_buffers,
        );
    }
    for key in to_grow {
        let Some(batch) = store.get_mut(&key) else {
            continue;
        };
        grow_sdf_batch_assets(
            batch,
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut storage_buffers,
        );
    }
}

fn padded_sdf_render_records(
    records: &[ResolvedSdfBatchRecord],
    capacity: u32,
) -> Vec<SdfRenderRecord> {
    let mut padded = Vec::with_capacity(capacity.to_usize());
    padded.extend(records.iter().map(SdfRenderRecord::from_resolved));
    padded.resize(
        capacity.to_usize().max(records.len()),
        SdfRenderRecord::padded(),
    );
    padded
}

fn padded_sdf_mesh_records(capacity: u32) -> Vec<SdfMeshRecord> {
    vec![SdfMeshRecord::default(); capacity.to_usize()]
}

fn inert_sdf_batch_mesh(capacity: u32) -> Mesh {
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

fn clipped_local_bounds(
    mesh_half_size: MeshHalfSize,
    clip_rect: LocalClipRect,
) -> Option<(Vec2, Vec2)> {
    let mesh_min = -mesh_half_size.value;
    let mesh_max = mesh_half_size.value;
    let clip_min = Vec2::new(clip_rect.value.x, clip_rect.value.y);
    let clip_max = Vec2::new(clip_rect.value.z, clip_rect.value.w);
    let min = mesh_min.max(clip_min);
    let max = mesh_max.min(clip_max);
    (min.x < max.x && min.y < max.y).then_some((min, max))
}

const fn clipped_corners(min: Vec2, max: Vec2) -> [Vec2; 4] {
    [
        Vec2::new(min.x, max.y),
        Vec2::new(max.x, max.y),
        Vec2::new(max.x, min.y),
        Vec2::new(min.x, min.y),
    ]
}

/// Spawns an SDF batch entity and its GPU assets.
pub(crate) fn spawn_sdf_batch_entity(
    key: &SdfBatchKey,
    batch: &mut SdfBatch,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<SdfExtendedMaterial>,
    storage_buffers: &mut Assets<ShaderBuffer>,
) {
    let capacity = batch.record_count().max(1).next_power_of_two();
    let records = storage_buffers.add(ShaderBuffer::from(padded_sdf_render_records(
        batch.records(),
        capacity,
    )));
    let mesh_records = storage_buffers.add(ShaderBuffer::from(padded_sdf_mesh_records(capacity)));
    let mesh = meshes.add(inert_sdf_batch_mesh(capacity));
    let material = materials.add(sdf_batch_material(SdfBatchMaterialInput {
        key:          key.clone(),
        records:      records.clone(),
        mesh_records: mesh_records.clone(),
    }));
    let mut entity = commands.spawn((
        DiegeticSdfFillBatch,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Visibility::Inherited,
        NoAutoAabb,
        Aabb::default(),
        key.layers.0.clone(),
    ));
    if key.shadow == VisualShadow::None {
        entity.insert(NotShadowCaster);
    }
    batch.entity = Some(entity.id());
    batch.gpu = Some(SdfBatchResources {
        records,
        mesh_records,
        mesh,
        material,
        capacity,
    });
    batch.record_upload.clear();
}

/// Grows one SDF batch's buffers and inert mesh to fit its live records.
pub(crate) fn grow_sdf_batch_assets(
    batch: &mut SdfBatch,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<SdfExtendedMaterial>,
    storage_buffers: &mut Assets<ShaderBuffer>,
) {
    let Some(entity) = batch.entity else {
        return;
    };
    let Some(current_capacity) = batch.gpu.as_ref().map(|gpu| gpu.capacity) else {
        return;
    };
    let required = batch.record_count().max(1);
    if required <= current_capacity {
        return;
    }
    let mut capacity = current_capacity.max(1);
    while capacity < required {
        capacity *= 2;
    }
    let records = storage_buffers.add(ShaderBuffer::from(padded_sdf_render_records(
        batch.records(),
        capacity,
    )));
    let mesh_records = storage_buffers.add(ShaderBuffer::from(padded_sdf_mesh_records(capacity)));
    let mesh = meshes.add(inert_sdf_batch_mesh(capacity));
    commands.entity(entity).insert(Mesh3d(mesh.clone()));
    let Some(gpu) = batch.gpu.as_mut() else {
        return;
    };
    if let Some(mut material) = materials.get_mut(&gpu.material) {
        set_sdf_material_record_buffers(&mut material, records.clone(), mesh_records.clone());
    }
    gpu.records = records;
    gpu.mesh_records = mesh_records;
    gpu.mesh = mesh;
    gpu.capacity = capacity;
    batch.record_upload.clear();
}

/// Updates SDF batch entity placement and local `Aabb` from record bounds.
pub(crate) fn update_sdf_batch_bounds(
    #[cfg(test)] mut run_order: Option<ResMut<SdfDriverRunOrder>>,
    mut store: ResMut<SdfBatchStore>,
    mut batch_entities: Query<
        (&mut Transform, &mut GlobalTransform, &mut Aabb),
        With<DiegeticSdfFillBatch>,
    >,
) {
    #[cfg(test)]
    material_table::record_sdf_driver_run(&mut run_order, material_table::SDF_DRIVER_BOUNDS);

    for (_, batch) in store.batches_mut() {
        if !batch.bounds_update.is_set() {
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
        batch.bounds_update.clear();
    }
}

/// Uploads dirty SDF record buffers with fixed-capacity payloads.
pub(crate) fn commit_sdf_batch_buffers(
    #[cfg(test)] mut run_order: Option<ResMut<SdfDriverRunOrder>>,
    mut store: ResMut<SdfBatchStore>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut diagnostics: ResMut<SdfRecordDiagnostics>,
) {
    #[cfg(test)]
    material_table::record_sdf_driver_run(&mut run_order, material_table::SDF_DRIVER_COMMIT);

    let mut uploads = 0_usize;
    let mut batches = 0_usize;
    let mut records = 0_usize;
    diagnostics.records.clear();
    for (_, batch) in store.batches_mut() {
        batches += 1;
        records += batch.records().len();
        for resolved in batch.records() {
            let record = SdfRenderRecord::from_resolved(resolved);
            diagnostics.records.push(SdfRecordSnapshot {
                fill_material:   record.fill_material.as_u32(),
                border_material: record.border_material.as_u32(),
                paint_mask:      record.paint_mask.bits(),
                half_size:       record.half_size,
                mesh_half_size:  record.mesh_half_size,
                flags:           record.flags,
            });
        }
        if !batch.record_upload.is_set() {
            continue;
        }
        let Some(gpu) = &batch.gpu else {
            continue;
        };
        let payload = padded_sdf_render_records(batch.records(), gpu.capacity);
        batch.record_upload.clear();
        if let Some(mut buffer) = storage_buffers.get_mut(&gpu.records) {
            buffer.set_data(payload);
            uploads += 1;
        }
    }
    perf.panel_geometry.sdf_batches = batches;
    perf.panel_geometry.sdf_records = records;
    perf.panel_geometry.sdf_uploads = uploads;
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "tests should fail loudly when SDF batch fixtures are invalid"
)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::ops::Range;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Arc;

    use bevy::asset::Asset;
    use bevy::asset::AssetEvent;
    use bevy::asset::AssetPlugin;
    use bevy::camera::visibility::RenderLayers;
    use bevy::ecs::message::Messages;
    use bevy::prelude::AlphaMode;
    use bevy::render::render_resource::Face;
    use bevy::render::render_resource::FragmentState;
    use bevy::shader::Shader;
    use bevy::shader::ShaderDefVal;
    use bevy_kana::ToF32;

    use super::*;
    use crate::Mm;
    use crate::layout::Border;
    use crate::layout::BoundingBox;
    use crate::layout::DrawZIndex;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::RectangleSource;
    use crate::layout::RenderCommand;
    use crate::layout::RenderCommandKind;
    use crate::layout::Sizing;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::panel::SurfaceShadow;
    use crate::render::PathExtendedMaterial;
    use crate::render::material_table::FrameMaterialSlotAppend;
    use crate::render::material_table::FrameMaterialTableBuild;
    use crate::render::material_table::INVALID_GPU_MATERIAL_SLOT;
    use crate::render::material_table::MATERIAL_TABLE_BINDING;
    use crate::render::material_table::MaterialSlotId;
    use crate::render::material_table::MaterialTableBuffer;
    use crate::render::material_table::MaterialTablePlugin;
    use crate::render::material_table::SDF_MESH_BINDING;
    use crate::render::material_table::SDF_RENDER_RECORDS_BINDING;
    use crate::render::panel_geometry::PanelGeometryPlugin;
    use crate::render::panel_geometry::ResolvedSdfMaterial;
    use crate::render::panel_geometry::ResolvedSdfSurface;
    use crate::render::panel_geometry::SdfRoleAuthorship;
    use crate::text::DiegeticTextMeasurer;

    const ANIMATED_BASE_BLUE_SPEED: f32 = 0.29;
    const ANIMATED_BASE_COLOR_SWING: f32 = 0.05;
    const ANIMATED_BASE_GREEN_SPEED: f32 = 0.23;
    const ANIMATED_BASE_RED_SPEED: f32 = 0.19;
    const SDF_SETTLE_FRAMES: usize = 3;

    fn zero_measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|_: &str, measure: &TextMeasure| TextDimensions {
                width:       0.0,
                height:      measure.size,
                line_height: measure.size,
            }),
        }
    }

    fn sdf_pipeline_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .add_plugins(TransformPlugin)
            .insert_resource(zero_measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .init_asset::<Mesh>()
            .init_asset::<Shader>()
            .init_asset::<ShaderBuffer>()
            .init_asset::<StandardMaterial>()
            .init_asset::<PathExtendedMaterial>()
            .init_asset::<SdfExtendedMaterial>()
            .add_plugins((MaterialTablePlugin, FillBatchPlugin, PanelGeometryPlugin));
        let default_material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(material::default_panel_material());
        app.insert_resource(CascadeDefault(SdfMaterial(default_material)));
        app
    }

    fn settle_sdf_pipeline(app: &mut App) {
        for _ in 0..SDF_SETTLE_FRAMES {
            app.update();
        }
    }

    fn spawn_sdf_panel(app: &mut App, tree: LayoutTree, material: StandardMaterial) -> Entity {
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(material);
        app.world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .material(material)
                    .with_tree(tree)
                    .build()
                    .expect("panel should build"),
            )
            .id()
    }

    fn single_surface_tree(color: Color) -> LayoutTree {
        LayoutBuilder::with_root(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .background(color),
        )
        .build()
    }

    fn stacked_surface_tree(first: Color, second: Color) -> LayoutTree {
        let mut builder = LayoutBuilder::new(Mm(100.0), Mm(50.0));
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .background(first),
            |builder| {
                builder.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .background(second),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    // Drives each fill's base color through the production `.background()` path
    // (the layout color override), while the material supplies the animated
    // scalar PBR fields the background does not touch.
    fn two_material_surface_tree(
        first: Handle<StandardMaterial>,
        first_color: Color,
        second: Handle<StandardMaterial>,
        second_color: Color,
    ) -> LayoutTree {
        let mut builder = LayoutBuilder::new(Mm(100.0), Mm(50.0));
        builder.with(
            El::column().width(Sizing::GROW).height(Sizing::GROW),
            |builder| {
                builder.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .background(first_color)
                        .material(first),
                    |_| {},
                );
                builder.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .background(second_color)
                        .material(second),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    fn bordered_surface_tree(fill: Color, border: Color) -> LayoutTree {
        LayoutBuilder::with_root(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .background(fill)
                .border(Border::all(Mm(1.0), border)),
        )
        .build()
    }

    /// Text-command state used by `text_toggle_tree`.
    #[derive(Clone, Copy)]
    enum TextContentState {
        /// The tree includes one text command after the background.
        Present,
        /// The tree includes only the background command.
        Removed,
    }

    fn text_toggle_tree(text_content_state: TextContentState) -> LayoutTree {
        let mut builder = LayoutBuilder::new(Mm(100.0), Mm(50.0));
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .background(Color::WHITE),
            |builder| {
                if matches!(text_content_state, TextContentState::Present) {
                    builder.text("Alpha", TextStyle::new(10.0));
                }
            },
        );
        builder.build()
    }

    fn topology_churn_tree(survivor: Color, added: Color) -> LayoutTree {
        let mut builder = LayoutBuilder::new(Mm(100.0), Mm(50.0));
        builder.with(
            El::new().width(Sizing::GROW).height(Sizing::GROW),
            |builder| {
                builder.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .background(survivor),
                    |_| {},
                );
                builder.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .background(added),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    fn sdf_records(app: &App) -> Vec<ResolvedSdfBatchRecord> {
        let mut records: Vec<ResolvedSdfBatchRecord> = app
            .world()
            .resource::<SdfBatchStore>()
            .batches()
            .flat_map(|(_, batch)| batch.records().iter().cloned())
            .collect();
        records.sort_by(|left, right| {
            left.record_key
                .command_index
                .cmp(&right.record_key.command_index)
        });
        records
    }

    fn single_sdf_batch_records(app: &App) -> Vec<ResolvedSdfBatchRecord> {
        let store = app.world().resource::<SdfBatchStore>();
        let batches: Vec<&SdfBatch> = store.batches().map(|(_, batch)| batch).collect();
        assert_eq!(batches.len(), 1, "expected exactly one SDF batch");
        batches[0].records().to_vec()
    }

    fn live_sdf_batch_count(app: &mut App) -> usize {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<DiegeticSdfFillBatch>>();
        query.iter(app.world()).count()
    }

    fn live_sdf_batch_entities(app: &mut App) -> Vec<Entity> {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<DiegeticSdfFillBatch>>();
        let mut entities: Vec<Entity> = query.iter(app.world()).collect();
        entities.sort_by_key(|entity| entity.to_bits());
        entities
    }

    fn sdf_batch_visibilities(app: &mut App) -> Vec<Visibility> {
        let mut query = app
            .world_mut()
            .query_filtered::<&Visibility, With<DiegeticSdfFillBatch>>();
        query.iter(app.world()).copied().collect()
    }

    fn linear_color(color: Color) -> Vec4 {
        let linear = color.to_linear();
        Vec4::new(linear.red, linear.green, linear.blue, linear.alpha)
    }

    fn fill_row_color(app: &App, record: &ResolvedSdfBatchRecord) -> Vec4 {
        let SdfPaintMaterial::Authored(slot) = record.fill_material else {
            panic!("record should have an authored fill material");
        };
        let row_index = usize::try_from(slot.as_u32()).expect("slot index should fit usize");
        app.world()
            .resource::<FrameMaterialTableBuild>()
            .table()
            .rows()[row_index]
            .base_color
    }

    fn frame_material_row_count(app: &App) -> usize {
        app.world()
            .resource::<FrameMaterialTableBuild>()
            .table()
            .row_count()
    }

    fn frame_material_capacity(app: &App) -> u32 {
        app.world().resource::<MaterialTableBuffer>().capacity
    }

    fn authored_slot_ids(records: &[ResolvedSdfBatchRecord]) -> Vec<MaterialSlotId> {
        records
            .iter()
            .flat_map(|record| [record.fill_material, record.border_material])
            .filter_map(|material| match material {
                SdfPaintMaterial::Authored(slot) => Some(slot),
                SdfPaintMaterial::NotAuthored => None,
            })
            .collect()
    }

    fn authored_slot_count(records: &[ResolvedSdfBatchRecord]) -> usize {
        authored_slot_ids(records).len()
    }

    fn assert_authored_slots_are_distinct(records: &[ResolvedSdfBatchRecord]) {
        let slot_ids = authored_slot_ids(records);
        let distinct_slot_ids: HashSet<_> = slot_ids.iter().copied().collect();
        assert_eq!(distinct_slot_ids.len(), slot_ids.len());
    }

    fn assert_fill_row_colors(
        app: &App,
        records: &[ResolvedSdfBatchRecord],
        expected_colors: &[Color],
    ) {
        assert_eq!(records.len(), expected_colors.len());
        for (record, expected_color) in records.iter().zip(expected_colors) {
            assert_eq!(fill_row_color(app, record), linear_color(*expected_color));
        }
    }

    fn animated_scalar_material(frame: usize, base: Color, metallic: f32) -> StandardMaterial {
        let frame = frame.to_f32();
        let base = base.to_srgba();
        StandardMaterial {
            base_color: Color::srgb(
                (frame * ANIMATED_BASE_RED_SPEED)
                    .sin()
                    .mul_add(ANIMATED_BASE_COLOR_SWING, base.red)
                    .clamp(0.0, 1.0),
                (frame * ANIMATED_BASE_GREEN_SPEED)
                    .sin()
                    .mul_add(ANIMATED_BASE_COLOR_SWING, base.green)
                    .clamp(0.0, 1.0),
                (frame * ANIMATED_BASE_BLUE_SPEED)
                    .sin()
                    .mul_add(ANIMATED_BASE_COLOR_SWING, base.blue)
                    .clamp(0.0, 1.0),
            ),
            metallic,
            perceptual_roughness: frame.mul_add(0.03, 0.25).min(0.9),
            reflectance: frame.mul_add(0.02, 0.35).min(0.9),
            ..Default::default()
        }
    }

    fn clear_asset_events<A: Asset>(app: &mut App) {
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<A>>>()
            .clear();
    }

    fn modified_asset_events<A: Asset>(app: &App) -> usize {
        app.world()
            .resource::<Messages<AssetEvent<A>>>()
            .iter_current_update_messages()
            .filter(|event| matches!(event, AssetEvent::Modified { .. }))
            .count()
    }

    fn crate_wgsl_files() -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_wgsl_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        files.sort();
        files
    }

    fn collect_wgsl_files(path: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("crate source directory should be readable") {
            let entry = entry.expect("crate source entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                collect_wgsl_files(&path, files);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "wgsl")
            {
                files.push(path);
            }
        }
    }

    fn function_body_range(source: &str, signature: &str) -> Range<usize> {
        let signature_start = source
            .find(signature)
            .expect("function signature should exist");
        let body_start = signature_start
            + source[signature_start..]
                .find('{')
                .expect("function body should start");
        let mut depth = 0_u32;
        for (offset, byte) in source[body_start..].bytes().enumerate() {
            match byte {
                b'{' => depth = depth.saturating_add(1),
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return body_start..body_start + offset + 1;
                    }
                },
                _ => {},
            }
        }
        panic!("function body should close");
    }

    fn storage_binding_for_field(source: &str, field_name: &str) -> u32 {
        let struct_body = &source[function_body_range(source, "pub(crate) struct SdfExtension")];
        let field_offset = struct_body
            .find(&format!("{field_name}:"))
            .unwrap_or_else(|| panic!("{field_name} should be an SdfExtension field"));
        let attr_prefix = "#[storage(";
        let attr_offset = struct_body[..field_offset]
            .rfind(attr_prefix)
            .unwrap_or_else(|| panic!("{field_name} should have a storage binding"));
        let binding_start = attr_offset + attr_prefix.len();
        let binding_digits: String = struct_body[binding_start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        binding_digits
            .parse()
            .expect("storage binding should be a u32 literal")
    }

    fn assert_wgsl_storage_binding(source: &str, binding: u32, field_name: &str) {
        assert!(
            source.contains(&format!(
                "@binding({binding}) var<storage, read> {field_name}"
            )),
            "{field_name} should use binding {binding}"
        );
    }

    fn assert_wgsl_binding_constant(source: &str, binding_name: &str, binding: u32) {
        assert!(
            source.contains(&format!("const {binding_name}: u32 = {binding}u;")),
            "{binding_name} should equal {binding}"
        );
    }

    fn assert_sdf_shader_ref(shader_ref: ShaderRef) {
        let ShaderRef::Path(path) = shader_ref else {
            panic!("SdfExtension shader entry point should use the shared SDF shader path");
        };
        assert_eq!(path.to_string(), EMBEDDED_SDF_PANEL_BATCH_SHADER_PATH);
    }

    fn assert_ordered(source: &str, ordered_needles: &[&str]) {
        let mut previous = 0;
        for needle in ordered_needles {
            let offset = source[previous..]
                .find(needle)
                .unwrap_or_else(|| panic!("{needle} should appear after byte {previous}"));
            previous += offset + needle.len();
        }
    }

    fn fill_input(material: &StandardMaterial, color: Option<Color>) -> SdfMaterialSlotInput<'_> {
        SdfMaterialSlotInput {
            key:            SdfMaterialSourceKey {
                panel:         Entity::from_bits(1),
                command_index: CommandIndex::from(0),
                role:          SdfMaterialRole::Fill,
            },
            base_material:  material,
            color_override: color,
        }
    }

    #[test]
    fn material_slot_input_projects_color_override_into_table_values() {
        let material = StandardMaterial {
            base_color: Color::srgb(0.1, 0.2, 0.3),
            ..Default::default()
        };
        let input = fill_input(&material, Some(Color::srgba(0.8, 0.7, 0.6, 0.5)));
        let candidate = input.material_slot_candidate();
        let expected = Color::srgba(0.8, 0.7, 0.6, 0.5).to_linear();

        assert_eq!(
            candidate.values.base_color,
            Vec4::new(expected.red, expected.green, expected.blue, expected.alpha)
        );
        assert_eq!(
            candidate.pipeline_compatibility,
            PipelineCompatibility::from(&StandardMaterial {
                base_color: Color::srgba(0.8, 0.7, 0.6, 0.5),
                ..material
            })
        );
    }

    #[test]
    fn paint_mask_and_gpu_slots_distinguish_fill_border_and_absent_roles() {
        let fill = SdfPaintMaterial::Authored(MaterialSlotId::try_from(0).expect("slot 0 is live"));
        let border = SdfPaintMaterial::NotAuthored;
        let mask = SdfPaintMask::from_materials(fill, border);

        assert_eq!(mask.bits(), SdfPaintMask::FILL);
        assert_eq!(fill.to_gpu().as_u32(), 0);
        assert_eq!(border.to_gpu().as_u32(), INVALID_GPU_MATERIAL_SLOT);
        assert_eq!(SdfRenderRecord::padded().paint_mask.bits(), 0);
        assert_eq!(
            SdfRenderRecord::padded().fill_material.as_u32(),
            INVALID_GPU_MATERIAL_SLOT
        );
    }

    #[test]
    fn first_role_limit_drop_skips_the_whole_record() {
        let mut builder = FrameMaterialTableBuilder::default();
        builder.clear(0);
        let (materials, default_material, material) =
            material_context_for_test(StandardMaterial::default());
        let surface = resolved_surface_for_test(
            0,
            &material,
            SdfRoleAuthorship::Authored,
            SdfRoleAuthorship::Authored,
        );
        let asset_server = asset_server_for_test();

        let append = append_sdf_record_materials(
            &mut builder,
            &surface,
            &materials,
            &asset_server,
            &default_material,
        );

        assert!(append.is_none());
        assert_eq!(builder.row_count(), 0);
        assert_eq!(builder.dropped_record_count(), 1);
    }

    #[test]
    fn post_fill_border_limit_drop_rolls_back_the_record() {
        let mut builder = FrameMaterialTableBuilder::default();
        builder.clear(1);
        let (materials, default_material, material) =
            material_context_for_test(StandardMaterial::default());
        let surface = resolved_surface_for_test(
            0,
            &material,
            SdfRoleAuthorship::Authored,
            SdfRoleAuthorship::Authored,
        );
        let asset_server = asset_server_for_test();

        let append = append_sdf_record_materials(
            &mut builder,
            &surface,
            &materials,
            &asset_server,
            &default_material,
        );

        assert!(append.is_none());
        assert_eq!(builder.row_count(), 0);
        assert_eq!(builder.dropped_record_count(), 1);
    }

    #[test]
    fn two_role_append_keeps_fill_and_border_when_both_rows_fit() {
        let mut builder = FrameMaterialTableBuilder::default();
        let (materials, default_material, material) =
            material_context_for_test(StandardMaterial::default());
        let surface = resolved_surface_for_test(
            0,
            &material,
            SdfRoleAuthorship::Authored,
            SdfRoleAuthorship::Authored,
        );
        builder.clear(2);
        let asset_server = asset_server_for_test();
        let append = append_sdf_record_materials(
            &mut builder,
            &surface,
            &materials,
            &asset_server,
            &default_material,
        )
        .expect("both authored roles fit when two rows remain");

        assert_eq!(builder.row_count(), 2);
        assert!(matches!(append.fill, SdfPaintMaterial::Authored(_)));
        assert!(matches!(append.border, SdfPaintMaterial::Authored(_)));
    }

    #[test]
    fn sdf_fallback_material_uses_blend_for_roles_without_base_material() {
        let mut builder = FrameMaterialTableBuilder::default();
        let material = StandardMaterial {
            alpha_mode: AlphaMode::Opaque,
            ..Default::default()
        };
        let (materials, default_material, material) = material_context_for_test(material);
        let mut surface = resolved_surface_for_test(
            0,
            &material,
            SdfRoleAuthorship::Authored,
            SdfRoleAuthorship::Authored,
        );
        surface.fill_material.base_material = None;
        surface.border_material.base_material = None;
        builder.clear(2);
        let asset_server = asset_server_for_test();

        let append = append_sdf_record_materials(
            &mut builder,
            &surface,
            &materials,
            &asset_server,
            &default_material,
        )
        .expect("color-authored SDF roles should append fallback material rows");

        assert_eq!(builder.row_count(), 2);
        assert_eq!(append.pipeline_compatibility.alpha, BatchAlphaMode::Blend);
        assert!(matches!(append.fill, SdfPaintMaterial::Authored(_)));
        assert!(matches!(append.border, SdfPaintMaterial::Authored(_)));
    }

    #[test]
    fn slot_zero_can_be_live_while_padded_records_are_absent() {
        let mut builder = FrameMaterialTableBuilder::default();
        builder.clear(4);
        let material = StandardMaterial::default();
        let input = fill_input(&material, Some(Color::WHITE));

        let append = material_table::append_material_slot(&mut builder, &input);
        let MaterialSlotAppend::Appended(appended) = append else {
            panic!("first slot should append");
        };

        assert_eq!(
            FrameMaterialSlotAppend::Appended(appended.slot),
            FrameMaterialSlotAppend::Appended(MaterialSlotId::try_from(0).expect("slot 0"))
        );
        assert_eq!(
            SdfPaintMaterial::Authored(appended.slot).to_gpu().as_u32(),
            0
        );
        assert_eq!(
            SdfRenderRecord::padded().border_material.as_u32(),
            INVALID_GPU_MATERIAL_SLOT
        );
    }

    #[test]
    fn scalar_pbr_edits_do_not_change_sdf_batch_key_compatibility() {
        let mut first = StandardMaterial {
            base_color: Color::srgb(0.1, 0.2, 0.3),
            metallic: 0.1,
            perceptual_roughness: 0.2,
            reflectance: 0.3,
            ..Default::default()
        };
        let second = StandardMaterial {
            base_color: Color::srgb(0.8, 0.7, 0.6),
            metallic: 0.9,
            perceptual_roughness: 0.8,
            reflectance: 0.7,
            ..Default::default()
        };
        first.alpha_mode = AlphaMode::Blend;

        assert_eq!(
            PipelineCompatibility::from(&first),
            PipelineCompatibility::from(&StandardMaterial {
                alpha_mode: AlphaMode::Blend,
                ..second.clone()
            })
        );
        assert_eq!(
            ResourceCompatibility::from(&first),
            ResourceCompatibility::from(&second)
        );
    }

    #[test]
    fn alpha_mode_and_culling_configure_sdf_batch_material() {
        for shadow in [VisualShadow::None, VisualShadow::Cast] {
            for cull_mode in [None, Some(Face::Back), Some(Face::Front)] {
                let mut pipeline_compatibility = PipelineCompatibility::from(&StandardMaterial {
                    alpha_mode: AlphaMode::Opaque,
                    double_sided: cull_mode.is_none(),
                    cull_mode,
                    unlit: true,
                    fog_enabled: false,
                    deferred_lighting_pass_id: 3,
                    ..Default::default()
                });
                pipeline_compatibility.double_sided = cull_mode.is_none();
                let key = SdfBatchKey {
                    panel_entity: Entity::from_bits(1),
                    z_level: 0,
                    layers: BatchRenderLayers(RenderLayers::layer(0)),
                    shadow,
                    contiguous_drawn_run: ContiguousDrawnRun::default(),
                    pipeline_compatibility,
                    resource_compatibility: ResourceCompatibility::from(
                        &StandardMaterial::default(),
                    ),
                };
                let material = sdf_batch_material(SdfBatchMaterialInput {
                    key,
                    records: Handle::default(),
                    mesh_records: Handle::default(),
                });

                let expected_alpha = match shadow {
                    VisualShadow::Cast => AlphaMode::Mask(0.0),
                    VisualShadow::None => AlphaMode::Opaque,
                };
                assert_eq!(material.base.alpha_mode, expected_alpha);
                assert_eq!(material.base.cull_mode, cull_mode);
                assert_eq!(material.base.double_sided, cull_mode.is_none());
                assert!(material.base.unlit);
                assert!(!material.base.fog_enabled);
                assert_eq!(material.base.deferred_lighting_pass_id, 3);
            }
        }
    }

    #[test]
    fn opaque_sdf_batches_use_mask_gpu_alpha_mode() {
        // Casting opaque batches map to Mask(0.0) to retain the material group on
        // the shadow/prepass pipeline; non-casting opaque batches stay Opaque.
        assert_eq!(
            sdf_batch_alpha_mode(AlphaMode::Opaque.into(), VisualShadow::Cast),
            AlphaMode::Mask(0.0)
        );
        assert_eq!(
            sdf_batch_alpha_mode(AlphaMode::Opaque.into(), VisualShadow::None),
            AlphaMode::Opaque
        );
        for mode in [
            AlphaMode::Mask(0.25),
            AlphaMode::Blend,
            AlphaMode::Premultiplied,
            AlphaMode::Add,
            AlphaMode::Multiply,
            AlphaMode::AlphaToCoverage,
        ] {
            for shadow in [VisualShadow::None, VisualShadow::Cast] {
                assert_eq!(sdf_batch_alpha_mode(mode.into(), shadow), mode);
            }
        }
    }

    #[test]
    fn interleaved_compatibility_keys_split_into_maximal_draw_runs() {
        let material_a = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..Default::default()
        };
        let material_b = StandardMaterial {
            alpha_mode: AlphaMode::Add,
            ..Default::default()
        };
        let mut materials = Assets::<StandardMaterial>::default();
        let handle_a = materials.add(material_a.clone());
        let handle_b = materials.add(material_b.clone());
        let surface_0 = resolved_surface_for_test(
            0,
            &handle_a,
            SdfRoleAuthorship::Authored,
            SdfRoleAuthorship::Unauthored,
        );
        let surface_1 = resolved_surface_for_test(
            1,
            &handle_b,
            SdfRoleAuthorship::Authored,
            SdfRoleAuthorship::Unauthored,
        );
        let surface_2 = resolved_surface_for_test(
            2,
            &handle_a,
            SdfRoleAuthorship::Authored,
            SdfRoleAuthorship::Unauthored,
        );
        let records = assign_contiguous_runs(vec![
            (slots_for_test(0, &material_a), &surface_0),
            (slots_for_test(1, &material_b), &surface_1),
            (slots_for_test(2, &material_a), &surface_2),
        ]);

        let runs: Vec<u32> = records
            .iter()
            .map(|record| record.batch_key.contiguous_drawn_run.value)
            .collect();

        assert_eq!(runs, vec![0, 1, 2]);
    }

    #[test]
    fn batch_bounds_use_clipped_world_corners() {
        let mut batch = SdfBatch::default();
        let record = ResolvedSdfBatchRecord {
            record_key:       SdfRecordKey {
                panel:         Entity::from_bits(1),
                command_index: CommandIndex::from(0),
            },
            fill_source:      SdfMaterialSourceKey {
                panel:         Entity::from_bits(1),
                command_index: CommandIndex::from(0),
                role:          SdfMaterialRole::Fill,
            },
            border_source:    SdfMaterialSourceKey {
                panel:         Entity::from_bits(1),
                command_index: CommandIndex::from(0),
                role:          SdfMaterialRole::Border,
            },
            draw_depth:       draw_depth_for_test(0),
            batch_key:        test_batch_key(),
            local_transform:  Transform::IDENTITY,
            transform:        Mat4::from_translation(Vec3::new(2.0, 3.0, 4.0)),
            half_size:        SdfHalfSize { value: Vec2::ONE },
            mesh_half_size:   MeshHalfSize {
                value: Vec2::splat(2.0),
            },
            corner_radii:     CornerRadii { value: Vec4::ZERO },
            border_widths:    BorderWidths { value: Vec4::ZERO },
            clip_rect:        LocalClipRect {
                value: Vec4::new(-1.0, -2.0, 2.0, 1.0),
            },
            fill_material:    SdfPaintMaterial::NotAuthored,
            border_material:  SdfPaintMaterial::NotAuthored,
            paint_mask:       SdfPaintMask::empty(),
            depth_nudge:      0.0,
            oit_depth_offset: 0.0,
            flags:            0,
        };
        batch.upsert_record(record);

        let (min, max) = batch.world_bounds().expect("clipped record has bounds");

        assert_eq!(min, Vec3::new(1.0, 1.0, 4.0));
        assert_eq!(max, Vec3::new(4.0, 4.0, 4.0));
    }

    #[test]
    fn fully_clipped_records_do_not_contribute_bounds() {
        assert_eq!(
            clipped_local_bounds(
                MeshHalfSize { value: Vec2::ONE },
                LocalClipRect {
                    value: Vec4::new(2.0, 2.0, 3.0, 3.0),
                },
            ),
            None
        );
    }

    #[test]
    fn sdf_batch_systems_run_in_driver_order() {
        let mut app = sdf_pipeline_app();
        app.init_resource::<SdfDriverRunOrder>();
        spawn_sdf_panel(
            &mut app,
            single_surface_tree(Color::WHITE),
            StandardMaterial::default(),
        );

        app.update();

        let run_order = app.world().resource::<SdfDriverRunOrder>();
        assert_eq!(run_order.names, material_table::SDF_DRIVER_RUN_ORDER);
        assert!(
            run_order.registered_batch_entity,
            "register_sdf_batch_materials should see the batch entity spawned by reconcile_sdf_batch_entities"
        );
    }

    #[test]
    fn sdf_batch_entities_spawn_visible_for_production_route() {
        let mut app = sdf_pipeline_app();
        spawn_sdf_panel(
            &mut app,
            single_surface_tree(Color::WHITE),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let visibilities = sdf_batch_visibilities(&mut app);

        assert_eq!(visibilities.len(), 1);
        assert!(
            visibilities
                .iter()
                .all(|visibility| *visibility != Visibility::Hidden),
            "production SDF batch entities must not spawn hidden"
        );
    }

    #[test]
    fn unchanged_sdf_frame_rewrites_only_the_material_table_buffer() {
        let mut app = sdf_pipeline_app();
        spawn_sdf_panel(
            &mut app,
            single_surface_tree(Color::srgba(0.2, 0.4, 0.6, 0.5)),
            StandardMaterial {
                alpha_mode: AlphaMode::Blend,
                ..Default::default()
            },
        );
        settle_sdf_pipeline(&mut app);
        clear_asset_events::<SdfExtendedMaterial>(&mut app);
        clear_asset_events::<PathExtendedMaterial>(&mut app);
        clear_asset_events::<ShaderBuffer>(&mut app);

        app.update();

        assert_eq!(modified_asset_events::<SdfExtendedMaterial>(&app), 0);
        assert_eq!(modified_asset_events::<PathExtendedMaterial>(&app), 0);
        assert_eq!(modified_asset_events::<ShaderBuffer>(&app), 1);
    }

    #[test]
    fn text_toggle_updates_sdf_oit_depth_offset_without_replacing_batch_entity() {
        let mut app = sdf_pipeline_app();
        let panel = spawn_sdf_panel(
            &mut app,
            text_toggle_tree(TextContentState::Present),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let entity_before = live_sdf_batch_entities(&mut app);
        let records_before = sdf_records(&app);
        assert_eq!(entity_before.len(), 1);
        assert_eq!(records_before.len(), 1);
        assert_eq!(
            records_before[0].oit_depth_offset.to_bits(),
            (-crate::render::constants::OIT_DEPTH_STEP).to_bits()
        );

        app.world_mut()
            .commands()
            .set_tree(panel, text_toggle_tree(TextContentState::Removed));
        settle_sdf_pipeline(&mut app);

        let entity_after = live_sdf_batch_entities(&mut app);
        let records_after = sdf_records(&app);
        assert_eq!(entity_after, entity_before);
        assert_eq!(records_after.len(), 1);
        assert_eq!(
            records_after[0].oit_depth_offset.to_bits(),
            0.0_f32.to_bits()
        );
    }

    #[test]
    fn opaque_sdf_fill_is_pushed_back_relative_to_a_blend_fill() {
        let fill_depth_nudge = |alpha_mode: AlphaMode| {
            let mut app = sdf_pipeline_app();
            spawn_sdf_panel(
                &mut app,
                single_surface_tree(Color::WHITE),
                StandardMaterial {
                    alpha_mode,
                    ..default()
                },
            );
            settle_sdf_pipeline(&mut app);
            let records = sdf_records(&app);
            assert_eq!(records.len(), 1);
            records[0].depth_nudge
        };

        let opaque_nudge = fill_depth_nudge(AlphaMode::Opaque);
        let blend_nudge = fill_depth_nudge(AlphaMode::Blend);

        assert_eq!(
            opaque_nudge.to_bits(),
            (blend_nudge - OPAQUE_FILL_DEPTH_PUSH_LAYERS).to_bits()
        );
    }

    #[test]
    fn color_only_sdf_updates_keep_the_batch_entity() {
        let mut app = sdf_pipeline_app();
        let panel = spawn_sdf_panel(
            &mut app,
            single_surface_tree(Color::WHITE),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let entity_before = live_sdf_batch_entities(&mut app);
        assert_eq!(entity_before.len(), 1);

        app.world_mut()
            .commands()
            .set_tree(panel, single_surface_tree(Color::srgb(0.7, 0.2, 0.1)));
        settle_sdf_pipeline(&mut app);

        let entity_after = live_sdf_batch_entities(&mut app);
        let records_after = sdf_records(&app);
        assert_eq!(entity_after, entity_before);
        assert_eq!(records_after.len(), 1);
        assert_eq!(
            fill_row_color(&app, &records_after[0]),
            linear_color(Color::srgb(0.7, 0.2, 0.1))
        );
    }

    #[test]
    fn scalar_material_animation_keeps_sdf_batch_and_table_capacity_stable() {
        let mut app = sdf_pipeline_app();
        let first_material = animated_scalar_material(0, Color::srgb(0.2, 0.4, 0.8), 0.0);
        let second_material = animated_scalar_material(1, Color::srgb(0.8, 0.3, 0.2), 1.0);
        let initial_colors = [first_material.base_color, second_material.base_color];
        let first_handle = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(first_material);
        let second_handle = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(second_material);
        let panel = spawn_sdf_panel(
            &mut app,
            two_material_surface_tree(
                first_handle,
                initial_colors[0],
                second_handle,
                initial_colors[1],
            ),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let initial_records = sdf_records(&app);
        let initial_capacity = frame_material_capacity(&app);
        assert_eq!(live_sdf_batch_count(&mut app), 1);
        assert_eq!(initial_records.len(), 2);
        assert_eq!(authored_slot_count(&initial_records), 2);
        assert_authored_slots_are_distinct(&initial_records);
        assert_fill_row_colors(&app, &initial_records, &initial_colors);
        assert_eq!(
            frame_material_row_count(&app),
            authored_slot_count(&initial_records)
        );

        for frame in 2..6 {
            let first_material = animated_scalar_material(frame, Color::srgb(0.1, 0.5, 0.9), 0.2);
            let second_material =
                animated_scalar_material(frame + 1, Color::srgb(0.9, 0.4, 0.1), 0.8);
            let expected_colors = [first_material.base_color, second_material.base_color];
            let first_handle = app
                .world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .add(first_material);
            let second_handle = app
                .world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .add(second_material);
            app.world_mut().commands().set_tree(
                panel,
                two_material_surface_tree(
                    first_handle,
                    expected_colors[0],
                    second_handle,
                    expected_colors[1],
                ),
            );
            settle_sdf_pipeline(&mut app);

            let records = sdf_records(&app);
            assert_eq!(live_sdf_batch_count(&mut app), 1);
            assert_eq!(records.len(), 2);
            assert_eq!(authored_slot_count(&records), 2);
            assert_authored_slots_are_distinct(&records);
            assert_fill_row_colors(&app, &records, &expected_colors);
            assert_eq!(
                frame_material_row_count(&app),
                authored_slot_count(&records)
            );
            assert_eq!(frame_material_capacity(&app), initial_capacity);
        }
    }

    #[test]
    fn sdf_batch_key_splits_on_authored_policy_and_resource_compatibility() {
        let mut images = Assets::<Image>::default();
        let texture = images.add(Image::default());
        let baseline_material = StandardMaterial::default();
        let baseline_key = sdf_batch_key_for_material(&baseline_material, VisualShadow::None);
        let splitters = [
            (
                "alpha mode",
                sdf_batch_key_for_material(
                    &StandardMaterial {
                        alpha_mode: AlphaMode::Blend,
                        ..Default::default()
                    },
                    VisualShadow::None,
                ),
            ),
            (
                "double sided",
                sdf_batch_key_for_material(
                    &StandardMaterial {
                        double_sided: true,
                        ..Default::default()
                    },
                    VisualShadow::None,
                ),
            ),
            (
                "cull mode",
                sdf_batch_key_for_material(
                    &StandardMaterial {
                        cull_mode: Some(Face::Front),
                        ..Default::default()
                    },
                    VisualShadow::None,
                ),
            ),
            (
                "texture resource",
                sdf_batch_key_for_material(
                    &StandardMaterial {
                        base_color_texture: Some(texture),
                        ..Default::default()
                    },
                    VisualShadow::None,
                ),
            ),
            (
                "lighting mode",
                sdf_batch_key_for_material(
                    &StandardMaterial {
                        unlit: true,
                        ..Default::default()
                    },
                    VisualShadow::None,
                ),
            ),
            (
                "shadow policy",
                sdf_batch_key_for_material(&StandardMaterial::default(), VisualShadow::Cast),
            ),
        ];

        for (splitter, key) in splitters {
            assert_ne!(key, baseline_key, "{splitter} should split from baseline");
        }
    }

    fn sdf_batch_key_for_material(
        material: &StandardMaterial,
        shadow: VisualShadow,
    ) -> SdfBatchKey {
        SdfBatchKey {
            shadow,
            pipeline_compatibility: PipelineCompatibility::from(material),
            resource_compatibility: ResourceCompatibility::from(material),
            ..test_batch_key()
        }
    }

    #[test]
    fn cull_modes_are_distinct_sdf_compatibility_values() {
        let keys: HashSet<PipelineCompatibility> = [None, Some(Face::Back), Some(Face::Front)]
            .into_iter()
            .map(|cull_mode| {
                PipelineCompatibility::from(&StandardMaterial {
                    cull_mode,
                    ..Default::default()
                })
            })
            .collect();

        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn record_and_material_rows_follow_live_sdf_surfaces() {
        let mut app = sdf_pipeline_app();
        let panel = spawn_sdf_panel(
            &mut app,
            stacked_surface_tree(Color::srgb(0.1, 0.1, 0.8), Color::srgb(0.2, 0.7, 0.3)),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let initial = sdf_records(&app);
        assert_eq!(initial.len(), 2);
        assert_authored_slots_are_distinct(&initial);
        assert_eq!(
            frame_material_row_count(&app),
            authored_slot_count(&initial)
        );

        app.world_mut()
            .commands()
            .set_tree(panel, single_surface_tree(Color::srgb(0.9, 0.2, 0.1)));
        settle_sdf_pipeline(&mut app);

        let reduced = sdf_records(&app);
        assert_eq!(reduced.len(), 1);
        assert_authored_slots_are_distinct(&reduced);
        assert_eq!(
            frame_material_row_count(&app),
            authored_slot_count(&reduced)
        );
    }

    #[test]
    fn removed_and_respawned_panels_use_current_frame_material_rows() {
        let mut app = sdf_pipeline_app();
        let panel = spawn_sdf_panel(
            &mut app,
            bordered_surface_tree(Color::srgb(0.2, 0.4, 0.8), Color::srgb(0.9, 0.8, 0.2)),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let initial = sdf_records(&app);
        assert_eq!(initial.len(), 1);
        assert_eq!(authored_slot_count(&initial), 2);
        assert_eq!(frame_material_row_count(&app), 2);

        let _ = app.world_mut().despawn(panel);
        settle_sdf_pipeline(&mut app);

        assert!(sdf_records(&app).is_empty());
        assert_eq!(frame_material_row_count(&app), 0);

        spawn_sdf_panel(
            &mut app,
            bordered_surface_tree(Color::srgb(0.1, 0.8, 0.4), Color::srgb(0.8, 0.2, 0.9)),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let respawned = sdf_records(&app);
        assert_eq!(respawned.len(), 1);
        assert_eq!(authored_slot_count(&respawned), 2);
        assert_eq!(frame_material_row_count(&app), 2);
        assert_eq!(
            fill_row_color(&app, &respawned[0]),
            linear_color(Color::srgb(0.1, 0.8, 0.4))
        );
    }

    #[test]
    fn visibility_and_layer_changes_rekey_sdf_batches_same_frame() {
        let mut app = sdf_pipeline_app();
        let panel = spawn_sdf_panel(
            &mut app,
            single_surface_tree(Color::WHITE),
            StandardMaterial::default(),
        );
        app.world_mut()
            .entity_mut(panel)
            .insert(RenderLayers::layer(0));
        settle_sdf_pipeline(&mut app);

        let initial_records = sdf_records(&app);
        assert_eq!(initial_records.len(), 1);
        assert_eq!(
            initial_records[0].batch_key.layers,
            BatchRenderLayers(RenderLayers::layer(0))
        );
        assert_eq!(live_sdf_batch_count(&mut app), 1);

        app.world_mut()
            .entity_mut(panel)
            .insert(RenderLayers::layer(1));
        app.update();

        let relayered = sdf_records(&app);
        assert_eq!(relayered.len(), 1);
        assert_eq!(
            relayered[0].batch_key.layers,
            BatchRenderLayers(RenderLayers::layer(1))
        );
        assert_eq!(live_sdf_batch_count(&mut app), 1);

        app.world_mut().entity_mut(panel).insert(Visibility::Hidden);
        app.update();

        assert!(sdf_records(&app).is_empty());
        assert_eq!(app.world().resource::<SdfBatchStore>().batches().count(), 0);
        assert_eq!(live_sdf_batch_count(&mut app), 0);
    }

    #[test]
    fn topology_churn_reuses_command_rows_without_stale_material_slots() {
        let survivor = Color::srgb(0.2, 0.7, 0.3);
        let added = Color::srgb(0.9, 0.2, 0.1);
        let mut app = sdf_pipeline_app();
        let panel = spawn_sdf_panel(
            &mut app,
            stacked_surface_tree(Color::srgb(0.1, 0.1, 0.8), survivor),
            StandardMaterial::default(),
        );
        settle_sdf_pipeline(&mut app);

        let initial = sdf_records(&app);
        assert_eq!(initial.len(), 2);
        assert_eq!(initial[0].record_key.command_index, CommandIndex::from(0));
        assert_eq!(initial[1].record_key.command_index, CommandIndex::from(1));

        app.world_mut()
            .commands()
            .set_tree(panel, topology_churn_tree(survivor, added));
        app.update();

        let churned = sdf_records(&app);
        assert_eq!(churned.len(), 2);
        assert_eq!(churned[0].record_key.command_index, CommandIndex::from(0));
        assert_eq!(churned[1].record_key.command_index, CommandIndex::from(1));
        assert_eq!(fill_row_color(&app, &churned[0]), linear_color(survivor));
        assert_eq!(fill_row_color(&app, &churned[1]), linear_color(added));
        assert_eq!(app.world().resource::<SdfBatchStore>().batches().count(), 1);
    }

    #[test]
    fn in_batch_upload_order_follows_draw_depth_then_command_index() {
        let mut app = sdf_pipeline_app();
        spawn_sdf_panel(
            &mut app,
            stacked_surface_tree(
                Color::srgba(0.9, 0.1, 0.1, 0.5),
                Color::srgba(0.1, 0.1, 0.9, 0.5),
            ),
            StandardMaterial {
                alpha_mode: AlphaMode::Blend,
                ..Default::default()
            },
        );
        settle_sdf_pipeline(&mut app);

        let records = single_sdf_batch_records(&app);

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].record_key.command_index, CommandIndex::from(0));
        assert_eq!(records[1].record_key.command_index, CommandIndex::from(1));
        assert!(records[0].draw_depth.ordinal_index() <= records[1].draw_depth.ordinal_index());
        assert!(records[0].oit_depth_offset < records[1].oit_depth_offset);
    }

    #[test]
    fn shader_sources_route_material_table_reads_through_helper() {
        let helper_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("shaders")
            .join("sdf_material_table.wgsl");
        let helper = fs::read_to_string(&helper_path).expect("SDF material helper should load");
        let body_range = function_body_range(&helper, "fn pbr_input_from_material_table");
        let body = &helper[body_range.clone()];
        let mut outside = String::new();
        outside.push_str(&helper[..body_range.start]);
        outside.push_str(&helper[body_range.end..]);

        assert!(helper.contains("fn pbr_input_from_material_table"));
        assert_eq!(outside.matches("material_table[").count(), 0);
        // Two mutually exclusive in-helper reads: the depth/shadow prepass reads
        // base_color only, the main pass reads the full row.
        assert_eq!(body.matches("material_table[").count(), 2);
        for path in crate_wgsl_files() {
            let source = fs::read_to_string(&path).expect("crate WGSL file should load");
            if path == helper_path {
                continue;
            }
            assert_eq!(
                source.matches("material_table[").count(),
                0,
                "{} should route material table reads through pbr_input_from_material_table",
                path.display()
            );
        }
        assert_ordered(
            body,
            &[
                "if !role_present",
                "if material_id == INVALID_GPU_MATERIAL_SLOT",
                "if material_id >= arrayLength(&material_table)",
                "material_table[material_index]",
            ],
        );
    }

    #[test]
    fn material_table_uv_transform_is_composed_before_texture_sampling() {
        let helper = include_str!("../shaders/sdf_material_table.wgsl");
        let body_range = function_body_range(helper, "fn pbr_input_from_material_table");
        let body = &helper[body_range];

        assert_ordered(
            body,
            &[
                "let values = material_table[material_index]",
                "sampled_input.uv = compute_material_sampled_uv(in.uv, values.uv_transform)",
                "pbr_input_from_standard_material(sampled_input, is_front)",
                "apply_material_slot_values(&pbr_input, values)",
            ],
        );
    }

    #[test]
    fn border_dominant_pixels_use_border_pbr_lighting_fields() {
        let shader = include_str!("../shaders/sdf_panel.wgsl");

        assert_ordered(
            shader,
            &[
                "let use_border_material",
                "pbr_input.material.emissive = border_pbr.material.emissive",
                "pbr_input.material.reflectance = border_pbr.material.reflectance",
                "pbr_input.material.perceptual_roughness = border_pbr.material.perceptual_roughness",
                "pbr_input.material.metallic = border_pbr.material.metallic",
                "pbr_input.material.ior = border_pbr.material.ior",
                "apply_pbr_lighting(pbr_input)",
            ],
        );
    }

    #[test]
    fn sdf_bindings_match_material_table_constants() {
        let rust_source = include_str!("fill_batch.rs");
        let helper = include_str!("../shaders/sdf_material_table.wgsl");
        let shader = include_str!("../shaders/sdf_panel.wgsl");
        let constants = include_str!("material_table.wgsl");

        assert_eq!(
            storage_binding_for_field(rust_source, "material_table"),
            MATERIAL_TABLE_BINDING
        );
        assert_eq!(
            storage_binding_for_field(rust_source, "records"),
            SDF_RENDER_RECORDS_BINDING
        );
        assert_eq!(
            storage_binding_for_field(rust_source, "mesh_records"),
            SDF_MESH_BINDING
        );
        assert_wgsl_storage_binding(helper, MATERIAL_TABLE_BINDING, "material_table");
        assert_wgsl_storage_binding(shader, SDF_RENDER_RECORDS_BINDING, "sdf_records");
        assert_wgsl_storage_binding(shader, SDF_MESH_BINDING, "sdf_mesh_records");
        assert_wgsl_binding_constant(constants, "MATERIAL_TABLE_BINDING", MATERIAL_TABLE_BINDING);
        assert_wgsl_binding_constant(
            constants,
            "SDF_RENDER_RECORDS_BINDING",
            SDF_RENDER_RECORDS_BINDING,
        );
        assert_wgsl_binding_constant(constants, "SDF_MESH_BINDING", SDF_MESH_BINDING);

        assert_sdf_shader_ref(SdfExtension::vertex_shader());
        assert_sdf_shader_ref(SdfExtension::fragment_shader());
        assert_sdf_shader_ref(SdfExtension::prepass_vertex_shader());
        assert_sdf_shader_ref(SdfExtension::prepass_fragment_shader());
        assert_sdf_shader_ref(SdfExtension::deferred_vertex_shader());
        assert_sdf_shader_ref(SdfExtension::deferred_fragment_shader());
        assert!(
            shader.contains(
                "#import bevy_diegetic::sdf_material_table::pbr_input_from_material_table"
            )
        );
        assert_ordered(
            shader,
            &[
                "fn fill_alpha_for_prepass",
                "pbr_input_from_material_table(",
                "#ifdef PREPASS_PIPELINE",
                "@fragment",
            ],
        );
        assert_ordered(
            shader,
            &[
                "#else\n@fragment",
                "let fill_pbr = pbr_input_from_material_table(",
                "let border_pbr = pbr_input_from_material_table(",
            ],
        );
        // Specialized per-pipeline layout assertions stay in the GPU/visual pass because
        // `RenderPipelineDescriptor` layouts require a render-device-backed specialization path.
    }

    #[test]
    fn stripped_material_group_fallback_is_present_for_prepass_review() {
        let shader = include_str!("../shaders/sdf_panel.wgsl");
        let helper = include_str!("../shaders/sdf_material_table.wgsl");
        let mut descriptor = RenderPipelineDescriptor {
            fragment: Some(FragmentState::default()),
            ..Default::default()
        };

        add_sdf_stripped_material_group_def(&mut descriptor, SdfPipelineMode::MeshAttributes);
        assert!(descriptor.vertex.shader_defs.is_empty());
        assert!(
            descriptor
                .fragment
                .as_ref()
                .expect("fragment state should exist")
                .shader_defs
                .is_empty()
        );

        add_sdf_stripped_material_group_def(&mut descriptor, SdfPipelineMode::VertexPulled);

        let stripped = ShaderDefVal::from("SDF_STRIPPED_MATERIAL_GROUP");
        assert!(descriptor.vertex.shader_defs.contains(&stripped));
        assert!(
            descriptor
                .fragment
                .as_ref()
                .expect("fragment state should exist")
                .shader_defs
                .contains(&stripped)
        );
        assert!(shader.contains("#ifdef SDF_STRIPPED_MATERIAL_GROUP"));
        assert!(shader.contains("#ifdef PREPASS_PIPELINE"));
        assert!(helper.contains("#ifdef SDF_STRIPPED_MATERIAL_GROUP"));
        assert!(helper.contains("pbr_types::pbr_input_new()"));
        assert!(
            !helper[function_body_range(helper, "fn stripped_material_group_pbr_input")]
                .contains("pbr_input_from_standard_material")
        );
        assert_eq!(
            sdf_batch_alpha_mode(AlphaMode::Opaque.into(), VisualShadow::Cast),
            AlphaMode::Mask(0.0)
        );
    }

    fn test_batch_key() -> SdfBatchKey {
        SdfBatchKey {
            panel_entity:           Entity::from_bits(1),
            z_level:                0,
            layers:                 BatchRenderLayers(RenderLayers::layer(0)),
            shadow:                 VisualShadow::Cast,
            contiguous_drawn_run:   ContiguousDrawnRun::default(),
            pipeline_compatibility: PipelineCompatibility::from(&StandardMaterial::default()),
            resource_compatibility: ResourceCompatibility::from(&StandardMaterial::default()),
        }
    }

    fn resolved_surface_for_test(
        command_index: usize,
        material: &Handle<StandardMaterial>,
        fill_authorship: SdfRoleAuthorship,
        border_authorship: SdfRoleAuthorship,
    ) -> ResolvedSdfSurface<'_> {
        ResolvedSdfSurface {
            panel_entity:    Entity::from_bits(1),
            command_index:   CommandIndex::from(command_index),
            draw_depth:      draw_depth_for_test(command_index),
            fill_material:   ResolvedSdfMaterial {
                authorship:    fill_authorship,
                base_material: Some(material),
                color:         fill_authorship.is_authored().then_some(Color::WHITE),
            },
            border_material: ResolvedSdfMaterial {
                authorship:    border_authorship,
                base_material: Some(material),
                color:         border_authorship.is_authored().then_some(Color::BLACK),
            },
            local_center:    Vec2::ZERO,
            local_transform: Transform::IDENTITY,
            sdf_half_size:   Vec2::ONE,
            mesh_half_size:  Vec2::splat(1.01),
            corner_radii:    [0.0; 4],
            border_widths:   [0.0; 4],
            clip_rect:       Vec4::new(-1.0, -1.0, 1.0, 1.0),
            render_layers:   RenderLayers::layer(0),
            surface_shadow:  SurfaceShadow::Off,
        }
    }

    fn slots_for_test(index: usize, material: &StandardMaterial) -> SdfRecordMaterialSlots {
        let slot_index = u32::try_from(index).expect("slot index fits in u32");
        SdfRecordMaterialSlots {
            fill:                   SdfPaintMaterial::Authored(
                MaterialSlotId::try_from(slot_index).expect("slot"),
            ),
            border:                 SdfPaintMaterial::NotAuthored,
            pipeline_compatibility: PipelineCompatibility::from(material),
            resource_compatibility: ResourceCompatibility::from(material),
        }
    }

    fn material_context_for_test(
        material: StandardMaterial,
    ) -> (
        Assets<StandardMaterial>,
        CascadeDefault<SdfMaterial>,
        Handle<StandardMaterial>,
    ) {
        let mut materials = Assets::<StandardMaterial>::default();
        let default = materials.add(material::default_panel_material());
        let material = materials.add(material);
        (materials, CascadeDefault(SdfMaterial(default)), material)
    }

    fn asset_server_for_test() -> AssetServer {
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default());
        app.world().resource::<AssetServer>().clone()
    }

    fn draw_depth_for_test(index: usize) -> DrawCommandDepth {
        let commands: Vec<RenderCommand> = (0..=index)
            .map(|element_idx| RenderCommand {
                bounds: BoundingBox {
                    x:      0.0,
                    y:      0.0,
                    width:  1.0,
                    height: 1.0,
                },
                kind: RenderCommandKind::Rectangle {
                    color:  Color::WHITE,
                    source: RectangleSource::Background,
                },
                element_idx,
                z_index: DrawZIndex(0),
            })
            .collect();
        draw_order::DrawOrderProjection::from_commands(&commands)
            .depth_for(index)
            .expect("draw command should have depth")
    }
}

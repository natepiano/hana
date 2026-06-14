//! Batch store for analytic path instancing: groups runs by batch key, owns
//! each batch's CPU record vectors and GPU handles, and derives run ranges by
//! rebuild (`docs/bevy_diegetic/glyph_instancing.md`, decision 4).
//!
//! Membership has a single mutation point — [`GlyphBatchStore::upsert_run`] /
//! [`GlyphBatchStore::remove_run`] update the run→batch index and the batch's
//! run set together — and ranges have a single writer: `rebuild` recomputes
//! them from the live run set, so they cannot go stale relative to the record
//! vectors they index. The store is plain data; the routing systems own all
//! entity and asset work (spawning batch entities, creating meshes and
//! buffers, uploading dirty records).

use std::collections::HashMap;
use std::ops::Range;

use bevy::math::Mat4;
use bevy::math::Vec2;
use bevy::math::Vec3;
use bevy::math::Vec4;
use bevy::math::Vec4Swizzles;
use bevy::pbr::StandardMaterial;
use bevy::prelude::Entity;
use bevy::prelude::Handle;
use bevy::prelude::Mesh;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::material::TextMaterial;
use super::packing::GlyphInstanceRecord;
use super::packing::RunRecord;
use crate::layout::GlyphShadowMode;
use crate::layout::Lighting;
use crate::layout::Sidedness;
use crate::render::BaseMaterialId;
use crate::render::BatchAlphaMode;
use crate::render::BatchRenderLayers;
use crate::render::VisualMaterialInterner;
use crate::text::RunStorageKey;

/// What splits text draws: every pipeline/material/entity-level property. A
/// run differing on several fields at once still maps to exactly one batch.
/// `fill_color`, `render_mode`, and the depth nudge are stored per run in
/// [`RunRecord`]s, so they do not split.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BatchKey {
    /// Interned authored base material (`DiegeticPanel::text_material` or the
    /// library default).
    pub base_material: BaseMaterialId,
    /// Resolved `TextAlpha` cascade value.
    pub alpha:         BatchAlphaMode,
    /// Resolved `Lighting` cascade value.
    pub lighting:      Lighting,
    /// Resolved `Sidedness` cascade value.
    pub sidedness:     Sidedness,
    /// Panel command z-level for the batch's shared screen sort lane.
    pub z_level:       i8,
    /// The run's `GlyphShadowMode` (`Cast` batches cast, `None` batches carry
    /// `NotShadowCaster`).
    pub shadow:        GlyphShadowMode,
    /// The owning panel's render layers.
    pub layers:        BatchRenderLayers,
}

/// GPU-side handles for one batch, created by the routing system on the
/// batch's first frame.
///
/// Both record buffers are **padded to capacity on every upload** so their
/// byte length never changes between growths: bevy re-creates the wgpu buffer
/// when the length changes (`bevy_render/src/storage.rs` `prepare_asset`),
/// and the material's bind group would keep pointing at the old buffer —
/// whether a same-frame material re-prepare sees the new buffer is a prepare
/// -order race. Constant-length uploads always write the existing buffer in
/// place, which existing bind groups observe. A capacity growth creates new
/// buffer assets and rewrites the material's handles, which re-prepares
/// reliably (a missing render asset retries next frame).
#[derive(Debug)]
pub(crate) struct BatchGpu {
    /// `GlyphInstanceRecord` storage buffer (binding 104), `capacity` records.
    pub instances:    Handle<ShaderBuffer>,
    /// `RunRecord` storage buffer (binding 105), `run_capacity` records.
    pub run_table:    Handle<ShaderBuffer>,
    /// Inert capacity-sized mesh; re-created and swapped on capacity growth.
    pub mesh:         Handle<Mesh>,
    /// The batch's material; its buffer handles are rewritten on growth.
    pub material:     Handle<TextMaterial>,
    /// Glyph-record capacity of `mesh` and `instances`.
    pub capacity:     u32,
    /// Run-record capacity of `run_table`.
    pub run_capacity: u32,
}

/// One member run's contribution: the source data `rebuild` derives the
/// concatenated record vectors from, plus its derived range.
#[derive(Debug)]
struct BatchRun {
    key:    RunStorageKey,
    /// Glyph records in run order; `run_index` is stamped by `rebuild`.
    glyphs: Vec<GlyphInstanceRecord>,
    record: RunRecord,
    /// This run's slots in the concatenated glyph records — derived state,
    /// written only by `rebuild`.
    range:  Range<u32>,
}

/// One render entity + one material + one mesh per [`BatchKey`]: the CPU
/// record vectors the GPU tables upload from, the member runs they derive
/// from, and the split dirty flags the commit system reads.
#[derive(Debug, Default)]
pub struct GlyphBatch {
    /// The batch render entity; `None` until the routing system spawns it.
    pub entity:          Option<Entity>,
    /// GPU handles; `None` until the routing system creates them.
    pub gpu:             Option<BatchGpu>,
    /// Glyph records changed — the instance buffer needs an upload.
    pub instances_dirty: bool,
    /// Run records changed — the run table needs an upload.
    pub run_table_dirty: bool,
    /// Membership, geometry, or a transform changed — the Aabb-union system
    /// recomputes this batch's bounds.
    pub bounds_dirty:    bool,
    runs:                Vec<BatchRun>,
    glyph_records:       Vec<GlyphInstanceRecord>,
    run_records:         Vec<RunRecord>,
}

impl GlyphBatch {
    /// Concatenated glyph records, `run_index` stamped — the instance-buffer
    /// upload payload.
    #[must_use]
    pub fn glyph_records(&self) -> &[GlyphInstanceRecord] { &self.glyph_records }

    /// One record per member run — the run-table upload payload.
    #[must_use]
    pub fn run_records(&self) -> &[RunRecord] { &self.run_records }

    /// Number of member runs.
    #[must_use]
    pub const fn run_count(&self) -> usize { self.runs.len() }

    /// Number of glyph records across all member runs.
    #[must_use]
    pub fn glyph_record_count(&self) -> u32 { self.glyph_records.len().to_u32() }

    /// Whether the last member run has left.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.runs.is_empty() }

    /// World-space bounds over every glyph rect × its run transform, for the
    /// Aabb-union system. `None` when the batch holds no records.
    #[must_use]
    pub fn world_bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::MAX;
        let mut max = Vec3::MIN;
        let mut any = false;
        for record in &self.glyph_records {
            let run = &self.run_records[record.run_index.to_usize()];
            for (corner_x, corner_y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let local = record.rect_min + Vec2::new(corner_x, corner_y) * record.rect_size;
                let world = run.transform * Vec4::new(local.x, local.y, 0.0, 1.0);
                min = min.min(world.xyz());
                max = max.max(world.xyz());
                any = true;
            }
        }
        any.then_some((min, max))
    }

    fn position_of(&self, key: RunStorageKey) -> Option<usize> {
        self.runs.iter().position(|run| run.key == key)
    }

    /// Recomputes the concatenated record vectors, run indices, and ranges
    /// from the live run set. The sole writer of `range`, so ranges cannot go
    /// stale relative to the vectors they index. Every record field is
    /// stamped from the run's source data — never defaulted.
    fn rebuild(&mut self) {
        self.glyph_records.clear();
        self.run_records.clear();
        for (index, run) in self.runs.iter_mut().enumerate() {
            let start = self.glyph_records.len().to_u32();
            let run_index = index.to_u32();
            self.glyph_records
                .extend(run.glyphs.iter().map(|record| GlyphInstanceRecord {
                    run_index,
                    ..*record
                }));
            run.range = start..self.glyph_records.len().to_u32();
            self.run_records.push(run.record);
        }
        self.instances_dirty = true;
        self.run_table_dirty = true;
        self.bounds_dirty = true;
    }

    fn push_run(
        &mut self,
        key: RunStorageKey,
        glyphs: Vec<GlyphInstanceRecord>,
        record: RunRecord,
    ) {
        self.runs.push(BatchRun {
            key,
            glyphs,
            record,
            range: 0..0,
        });
        self.rebuild();
    }

    fn remove_run(&mut self, key: RunStorageKey) {
        if let Some(position) = self.position_of(key) {
            self.runs.remove(position);
            self.rebuild();
        }
    }

    /// Replaces a member run's records. A same-count edit (the steady-state
    /// stress case) writes the run's range in place; a count change takes the
    /// rebuild path.
    fn update_run(
        &mut self,
        key: RunStorageKey,
        glyphs: Vec<GlyphInstanceRecord>,
        record: RunRecord,
    ) {
        let Some(position) = self.position_of(key) else {
            return;
        };
        if self.runs[position].glyphs.len() == glyphs.len() {
            let run_index = position.to_u32();
            let range = self.runs[position].range.clone();
            for (slot, source) in range.zip(glyphs.iter()) {
                self.glyph_records[slot.to_usize()] = GlyphInstanceRecord {
                    run_index,
                    ..*source
                };
            }
            if self.run_records[position] != record {
                self.run_records[position] = record;
                self.run_table_dirty = true;
            }
            self.runs[position].glyphs = glyphs;
            self.runs[position].record = record;
            self.instances_dirty = true;
            self.bounds_dirty = true;
        } else {
            self.runs[position].glyphs = glyphs;
            self.runs[position].record = record;
            self.rebuild();
        }
    }

    /// Writes one member run's transform into its `RunRecord` slot, dirtying
    /// the run table only when the matrix actually changed.
    fn update_run_transform(&mut self, key: RunStorageKey, transform: Mat4) {
        let Some(position) = self.position_of(key) else {
            return;
        };
        if self.runs[position].record.transform == transform {
            return;
        }
        self.runs[position].record.transform = transform;
        self.run_records[position].transform = transform;
        self.run_table_dirty = true;
        self.bounds_dirty = true;
    }
}

/// Routes every panel-text run to its batch: one [`GlyphBatch`] per
/// [`BatchKey`], a run→batch index, and the base-material interner.
#[derive(Debug, Default)]
pub(crate) struct GlyphBatchStore {
    batches:   HashMap<BatchKey, GlyphBatch>,
    run_index: HashMap<RunStorageKey, BatchKey>,
    interner:  VisualMaterialInterner,
}

impl GlyphBatchStore {
    /// Id for an authored base material, minting one on first sight.
    pub fn intern_base_material(&mut self, material: &StandardMaterial) -> BaseMaterialId {
        self.interner.intern_base_material(material)
    }

    /// The authored material behind an interned id.
    #[must_use]
    pub fn base_material(&self, id: BaseMaterialId) -> &StandardMaterial {
        self.interner.base_material(id)
    }

    /// Whether a run is currently routed to any batch.
    #[must_use]
    pub fn is_routed(&self, run: RunStorageKey) -> bool { self.run_index.contains_key(&run) }

    /// Inserts a run into its key's batch, moves it when its key changed, or
    /// updates it in place — the single membership mutation point, together
    /// with [`Self::remove_run`].
    pub fn upsert_run(
        &mut self,
        key: BatchKey,
        run: RunStorageKey,
        glyphs: Vec<GlyphInstanceRecord>,
        record: RunRecord,
    ) {
        if let Some(current) = self.run_index.get(&run) {
            if *current == key {
                if let Some(batch) = self.batches.get_mut(&key) {
                    batch.update_run(run, glyphs, record);
                }
                return;
            }
            let previous = current.clone();
            if let Some(batch) = self.batches.get_mut(&previous) {
                batch.remove_run(run);
            }
            self.run_index.remove(&run);
        }
        self.batches
            .entry(key.clone())
            .or_default()
            .push_run(run, glyphs, record);
        self.run_index.insert(run, key);
    }

    /// Removes a run from its batch. The emptied batch keeps its store entry
    /// until the routing system reconciles it via [`Self::take_empty_batches`].
    pub fn remove_run(&mut self, run: RunStorageKey) {
        let Some(key) = self.run_index.remove(&run) else {
            return;
        };
        if let Some(batch) = self.batches.get_mut(&key) {
            batch.remove_run(run);
        }
    }

    /// Writes a routed run's world transform into its `RunRecord` slot. A
    /// no-op for unrouted runs (e.g. a fully clipped label).
    pub fn update_run_transform(&mut self, run: RunStorageKey, transform: Mat4) {
        let Some(key) = self.run_index.get(&run) else {
            return;
        };
        if let Some(batch) = self.batches.get_mut(key) {
            batch.update_run_transform(run, transform);
        }
    }

    /// All batches.
    pub fn batches(&self) -> impl Iterator<Item = (&BatchKey, &GlyphBatch)> { self.batches.iter() }

    /// All batches, mutable.
    pub fn batches_mut(&mut self) -> impl Iterator<Item = (&BatchKey, &mut GlyphBatch)> {
        self.batches.iter_mut()
    }

    /// One batch by key.
    #[must_use]
    pub fn get(&self, key: &BatchKey) -> Option<&GlyphBatch> { self.batches.get(key) }

    /// One batch by key, mutable.
    pub fn get_mut(&mut self, key: &BatchKey) -> Option<&mut GlyphBatch> {
        self.batches.get_mut(key)
    }

    /// Drops batches whose last run left, returning their entities for the
    /// routing system to despawn (the batch analogue of the R10 empty-run
    /// path).
    pub fn take_empty_batches(&mut self) -> Vec<Entity> {
        let empty: Vec<BatchKey> = self
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should fail loudly when fixture batches are missing"
)]
mod tests {
    use bevy::camera::visibility::RenderLayers;
    use bevy::color::Color;
    use bevy::prelude::AlphaMode;

    use super::*;

    fn glyph(rect_min: Vec2, atlas_index: u32) -> GlyphInstanceRecord {
        GlyphInstanceRecord {
            rect_min,
            rect_size: Vec2::ONE,
            uv_min: Vec2::ZERO,
            uv_size: Vec2::ONE,
            atlas_index,
            run_index: 0,
        }
    }

    fn record(transform: Mat4) -> RunRecord {
        RunRecord {
            transform,
            fill_color: Vec4::ONE,
            render_mode: 1,
            depth_nudge: 0.0,
            oit_depth_offset: 0.0,
            aa_flags: 3,
        }
    }

    fn key(store: &mut GlyphBatchStore, alpha: AlphaMode) -> BatchKey {
        let id = store.intern_base_material(&StandardMaterial::default());
        BatchKey {
            base_material: id,
            alpha:         alpha.into(),
            lighting:      Lighting::Lit,
            sidedness:     Sidedness::DoubleSided,
            z_level:       0,
            shadow:        GlyphShadowMode::Cast,
            layers:        BatchRenderLayers(RenderLayers::layer(0)),
        }
    }

    fn run_key(bits: u64) -> RunStorageKey { RunStorageKey::from(Entity::from_bits(bits)) }

    #[test]
    fn two_runs_one_key_share_a_batch_with_contiguous_ranges() {
        let mut store = GlyphBatchStore::default();
        let batch_key = key(&mut store, AlphaMode::Blend);
        let first = run_key(1);
        let second = run_key(2);

        store.upsert_run(
            batch_key.clone(),
            first,
            vec![glyph(Vec2::ZERO, 0), glyph(Vec2::X, 1)],
            record(Mat4::IDENTITY),
        );
        store.upsert_run(
            batch_key.clone(),
            second,
            vec![glyph(Vec2::Y, 2)],
            record(Mat4::IDENTITY),
        );

        assert_eq!(store.batches().count(), 1);
        let batch = store.get(&batch_key).expect("batch should exist");
        assert_eq!(batch.run_count(), 2);
        assert_eq!(batch.glyph_record_count(), 3);
        // Records are concatenated in insertion order with run indices stamped.
        let stamped: Vec<u32> = batch
            .glyph_records()
            .iter()
            .map(|record| record.run_index)
            .collect();
        assert_eq!(stamped, vec![0, 0, 1]);
        assert!(batch.instances_dirty);
        assert!(batch.run_table_dirty);
        assert!(batch.bounds_dirty);
    }

    #[test]
    fn removing_a_run_rebuilds_the_survivors_ranges() {
        let mut store = GlyphBatchStore::default();
        let batch_key = key(&mut store, AlphaMode::Blend);
        let first = run_key(1);
        let second = run_key(2);
        store.upsert_run(
            batch_key.clone(),
            first,
            vec![glyph(Vec2::ZERO, 0), glyph(Vec2::X, 1)],
            record(Mat4::IDENTITY),
        );
        store.upsert_run(
            batch_key.clone(),
            second,
            vec![glyph(Vec2::Y, 2)],
            record(Mat4::IDENTITY),
        );

        store.remove_run(first);

        assert!(!store.is_routed(first));
        let batch = store.get(&batch_key).expect("batch should survive");
        assert_eq!(batch.run_count(), 1);
        assert_eq!(batch.glyph_record_count(), 1);
        // The surviving run shifted to index 0 and its records re-stamped.
        assert_eq!(batch.glyph_records()[0].run_index, 0);
        assert_eq!(batch.glyph_records()[0].atlas_index, 2);
    }

    #[test]
    fn same_count_edit_writes_in_place_without_touching_the_run_table() {
        let mut store = GlyphBatchStore::default();
        let batch_key = key(&mut store, AlphaMode::Blend);
        let run = run_key(1);
        let stamped = record(Mat4::IDENTITY);
        store.upsert_run(
            batch_key.clone(),
            run,
            vec![glyph(Vec2::ZERO, 0), glyph(Vec2::X, 1)],
            stamped,
        );
        {
            let batch = store.get_mut(&batch_key).expect("batch should exist");
            batch.instances_dirty = false;
            batch.run_table_dirty = false;
        }

        // Same record count, different atlas indices — the stress-test edit
        // pattern ("07 412" → "07 413").
        store.upsert_run(
            batch_key.clone(),
            run,
            vec![glyph(Vec2::ZERO, 5), glyph(Vec2::X, 6)],
            stamped,
        );

        let batch = store.get(&batch_key).expect("batch should exist");
        assert!(batch.instances_dirty, "glyph records changed");
        assert!(
            !batch.run_table_dirty,
            "an unchanged run record must not dirty the run table"
        );
        let atlas: Vec<u32> = batch
            .glyph_records()
            .iter()
            .map(|record| record.atlas_index)
            .collect();
        assert_eq!(atlas, vec![5, 6]);
    }

    #[test]
    fn count_change_takes_the_rebuild_path() {
        let mut store = GlyphBatchStore::default();
        let batch_key = key(&mut store, AlphaMode::Blend);
        let run = run_key(1);
        store.upsert_run(
            batch_key.clone(),
            run,
            vec![glyph(Vec2::ZERO, 0)],
            record(Mat4::IDENTITY),
        );

        store.upsert_run(
            batch_key.clone(),
            run,
            vec![glyph(Vec2::ZERO, 0), glyph(Vec2::X, 1)],
            record(Mat4::IDENTITY),
        );

        let batch = store.get(&batch_key).expect("batch should exist");
        assert_eq!(batch.glyph_record_count(), 2);
        assert!(batch.run_table_dirty, "a rebuild re-uploads both buffers");
    }

    #[test]
    fn key_change_moves_the_run_between_batches() {
        let mut store = GlyphBatchStore::default();
        let blend = key(&mut store, AlphaMode::Blend);
        let add = key(&mut store, AlphaMode::Add);
        let run = run_key(1);
        store.upsert_run(
            blend.clone(),
            run,
            vec![glyph(Vec2::ZERO, 0)],
            record(Mat4::IDENTITY),
        );

        store.upsert_run(
            add.clone(),
            run,
            vec![glyph(Vec2::ZERO, 0)],
            record(Mat4::IDENTITY),
        );

        let source = store.get(&blend).expect("source batch entry persists");
        assert!(source.is_empty(), "the run left its old batch");
        let destination = store.get(&add).expect("destination batch exists");
        assert_eq!(destination.run_count(), 1);
        let emptied = store.take_empty_batches();
        assert!(emptied.is_empty(), "no entity was ever spawned");
        assert!(store.get(&blend).is_none(), "the emptied entry is dropped");
    }

    #[test]
    fn move_then_remove_in_one_pass_leaves_both_maps_consistent() {
        let mut store = GlyphBatchStore::default();
        let blend = key(&mut store, AlphaMode::Blend);
        let add = key(&mut store, AlphaMode::Add);
        let run = run_key(1);
        store.upsert_run(
            blend,
            run,
            vec![glyph(Vec2::ZERO, 0)],
            record(Mat4::IDENTITY),
        );

        // One routing pass: the run re-keys (a live cascade change) and is
        // removed (its label despawned) before any reconcile runs.
        store.upsert_run(add, run, vec![glyph(Vec2::ZERO, 0)], record(Mat4::IDENTITY));
        store.remove_run(run);

        assert!(!store.is_routed(run));
        let total_runs: usize = store.batches().map(|(_, batch)| batch.run_count()).sum();
        assert_eq!(total_runs, 0, "no batch retains the removed run");
        store.take_empty_batches();
        assert_eq!(
            store.batches().count(),
            0,
            "both the source and destination entries empty out and drop"
        );
    }

    #[test]
    fn transform_update_dirties_the_run_table_only_on_change() {
        let mut store = GlyphBatchStore::default();
        let batch_key = key(&mut store, AlphaMode::Blend);
        let run = run_key(1);
        store.upsert_run(
            batch_key.clone(),
            run,
            vec![glyph(Vec2::ZERO, 0)],
            record(Mat4::IDENTITY),
        );
        {
            let batch = store.get_mut(&batch_key).expect("batch should exist");
            batch.run_table_dirty = false;
            batch.bounds_dirty = false;
        }

        store.update_run_transform(run, Mat4::IDENTITY);
        let batch = store.get(&batch_key).expect("batch should exist");
        assert!(
            !batch.run_table_dirty,
            "an identical matrix must not dirty the run table"
        );

        let moved = Mat4::from_translation(Vec3::X);
        store.update_run_transform(run, moved);
        let batch = store.get(&batch_key).expect("batch should exist");
        assert!(batch.run_table_dirty);
        assert!(batch.bounds_dirty);
        assert_eq!(batch.run_records()[0].transform, moved);
    }

    #[test]
    fn interner_assigns_one_id_per_distinct_material() {
        let mut store = GlyphBatchStore::default();
        let default_id = store.intern_base_material(&StandardMaterial::default());
        let same_id = store.intern_base_material(&StandardMaterial::default());
        let tinted = StandardMaterial {
            base_color: Color::srgb(0.5, 0.2, 0.2),
            ..Default::default()
        };
        let tinted_id = store.intern_base_material(&tinted);

        assert_eq!(default_id, same_id);
        assert_ne!(default_id, tinted_id);
        assert_eq!(
            store.base_material(tinted_id).base_color,
            Color::srgb(0.5, 0.2, 0.2)
        );
    }

    #[test]
    fn world_bounds_unions_rects_across_run_transforms() {
        let mut store = GlyphBatchStore::default();
        let batch_key = key(&mut store, AlphaMode::Blend);
        store.upsert_run(
            batch_key.clone(),
            run_key(1),
            vec![glyph(Vec2::ZERO, 0)],
            record(Mat4::IDENTITY),
        );
        store.upsert_run(
            batch_key.clone(),
            run_key(2),
            vec![glyph(Vec2::ZERO, 0)],
            record(Mat4::from_translation(Vec3::new(4.0, 0.0, -2.0))),
        );

        let (min, max) = store
            .get(&batch_key)
            .expect("batch should exist")
            .world_bounds()
            .expect("two records produce bounds");

        // Unit rects at the origin and at (4, 0, -2).
        assert_eq!(min, Vec3::new(0.0, 0.0, -2.0));
        assert_eq!(max, Vec3::new(5.0, 1.0, 0.0));
    }
}

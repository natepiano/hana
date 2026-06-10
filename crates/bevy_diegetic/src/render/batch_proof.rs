//! Step-1 vertex-pulling proof scaffolding for `examples/glyph_batch_proof.rs`
//! (glyph-instancing plan, `docs/bevy_diegetic/glyph_instancing.md`; removed
//! or repurposed at Step 4).
//!
//! Once text shaping has run for the example's ordinary panel and seeded the
//! shared glyph atlas, [`spawn_proof_batch`] hand-builds one batch entity
//! beside the production renderer: an inert capacity-sized mesh, a
//! `GlyphInstanceRecord` buffer copied from the panel's prepared run with
//! live atlas indices, and a `RunRecord` table with fixed hand-written
//! placements. [`force_capacity_growth`] appends another run copy, re-creates
//! the mesh at doubled capacity, and swaps it onto the entity in the same
//! frame while capturing frame-stepped screenshots — the no-blink gate.

use std::time::Instant;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::mesh::allocator::MeshAllocator;
use bevy::render::storage::ShaderBuffer;
use bevy::render::view::window::screenshot::Screenshot;
use bevy::render::view::window::screenshot::save_to_disk;
use bevy_kana::ToF32;
use bevy_kana::ToU32;

use super::BatchTextMaterialInput;
use super::GlyphAtlasHandles;
use super::GlyphInstanceRecord;
use super::RenderMode;
use super::RunRecord;
use super::TextMaterial;
use super::constants;
use super::panel_text;
use super::panel_text::PreparedPanelText;
use super::world_text::TextContent;
use crate::text::GlyphCache;

/// Directory the growth-frame screenshots are written to (`target/` is wiped
/// by `cargo clean`, so they are stored under `/private/tmp`).
const SCREENSHOT_DIR: &str = "/private/tmp/glyph_batch_proof";
/// Vertical drop from the source label to the first batch-run copy.
const RUN_VERTICAL_STEP: f32 = 0.55;
/// Yaw applied to the second run copy, proving a full 3-D run transform.
const TILTED_RUN_YAW_RADIANS: f32 = 0.35;
/// Number of consecutive frames captured around a forced capacity growth.
const GROWTH_CAPTURE_FRAMES: u8 = 3;

/// Marker on the hand-built batch entity, BRP-inspectable.
#[derive(Component)]
pub struct GlyphBatchProof;

/// Handles and CPU-side records for the one proof batch.
#[derive(Default, Resource)]
pub struct ProofBatchState {
    batch: Option<ProofBatch>,
    /// Same-layout mesh allocated ahead of the batch mesh so the batch mesh
    /// lands at a nonzero `first_vertex_index` in the shared slab.
    decoy: Option<Handle<Mesh>>,
}

struct ProofBatch {
    entity:           Entity,
    mesh:             Handle<Mesh>,
    instances:        Handle<ShaderBuffer>,
    run_table:        Handle<ShaderBuffer>,
    material:         Handle<TextMaterial>,
    capacity:         u32,
    glyph_records:    Vec<GlyphInstanceRecord>,
    run_records:      Vec<RunRecord>,
    /// One run's worth of glyph records with `run_index` 0; each appended run
    /// re-stamps the index.
    template:         Vec<GlyphInstanceRecord>,
    /// Snapshot of the source label's world matrix the copies are placed from.
    source_transform: Mat4,
    fill_color:       Vec4,
}

/// Batch mesh asset id the render-world logger reports `first_vertex_index`
/// for.
#[derive(Clone, Default, ExtractResource, Resource)]
pub struct ProofMeshId(Option<AssetId<Mesh>>);

/// Countdown of growth screenshots still to capture, plus a serial so
/// repeated growths write distinct files.
#[derive(Default, Resource)]
pub struct GrowthCaptures {
    remaining: u8,
    serial:    u32,
}

/// Plugin wiring the proof systems; the example binds
/// [`force_capacity_growth`] and [`toggle_debug_index`] to shortcuts.
pub struct GlyphBatchProofPlugin;

impl Plugin for GlyphBatchProofPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProofBatchState>()
            .init_resource::<ProofMeshId>()
            .init_resource::<GrowthCaptures>()
            .add_plugins(ExtractResourcePlugin::<ProofMeshId>::default())
            .add_systems(Startup, allocate_decoy_mesh)
            .add_systems(Update, spawn_proof_batch)
            .add_systems(PostUpdate, capture_growth_frames);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(Render, log_first_vertex.in_set(RenderSystems::Queue));
        }
    }
}

/// Allocates a small same-layout mesh before the batch mesh exists, so the
/// batch mesh shares its slab and draws with a nonzero `base_vertex` — the
/// `first_vertex_index` subtraction is then exercised, not vacuous.
fn allocate_decoy_mesh(mut state: ResMut<ProofBatchState>, mut meshes: ResMut<Assets<Mesh>>) {
    state.decoy = Some(meshes.add(panel_text::inert_batch_mesh(1)));
}

/// Builds and spawns the proof batch once text shaping has produced the
/// seeded panel's run and its glyphs are packed into the shared atlas.
fn spawn_proof_batch(
    mut state: ResMut<ProofBatchState>,
    labels: Query<(&PreparedPanelText, &GlobalTransform), With<TextContent>>,
    mut cache: ResMut<GlyphCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut mesh_id: ResMut<ProofMeshId>,
    mut commands: Commands,
) {
    if state.batch.is_some() {
        return;
    }
    let Some((panel_run, label_transform)) = labels.iter().next() else {
        return;
    };
    if panel_run.prepared.glyph_count() == 0 {
        return;
    }

    // Template records copied from the source run — hand-written placements,
    // live atlas indices. `None` means a glyph is not packed yet; retry next
    // frame.
    let Some(template) =
        panel_text::build_glyph_records(&cache, &panel_run.prepared, panel_run.clip_rect)
    else {
        return;
    };
    if template.is_empty() {
        return;
    }
    let Some(atlas) = cache.commit_glyph_atlas(&mut buffers, &mut materials) else {
        return;
    };

    let source_transform = label_transform.to_matrix();
    let fill_color = LinearRgba::from(panel_run.fill_color);
    let fill_color = Vec4::new(
        fill_color.red,
        fill_color.green,
        fill_color.blue,
        fill_color.alpha,
    );
    // Two hand-placed runs: a straight copy below the source label
    // (depth_nudge 0) and a yawed copy below that (depth_nudge 1) — distinct
    // transforms and nudges through one record table.
    let run_records = vec![
        proof_run_record(source_transform, 0, fill_color),
        proof_run_record(source_transform, 1, fill_color),
    ];
    let glyph_records = stamped_runs(&template, run_records.len());
    let capacity = glyph_records.len().to_u32();

    let instances = buffers.add(ShaderBuffer::from(glyph_records.clone()));
    let run_table = buffers.add(ShaderBuffer::from(run_records.clone()));
    let mesh = meshes.add(panel_text::inert_batch_mesh(capacity));
    let material = materials.add(batch_material(
        &atlas,
        instances.clone(),
        run_table.clone(),
        fill_color,
    ));

    let entity = commands
        .spawn((
            Name::new("Glyph batch proof"),
            GlyphBatchProof,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(source_transform.w_axis.truncate()),
            // The inert mesh's zeroed positions can't produce a real Aabb;
            // Step 2's Aabb-union system owns culling, the proof opts out.
            NoFrustumCulling,
        ))
        .id();
    mesh_id.0 = Some(mesh.id());
    info!(
        "glyph batch proof: spawned batch {entity} — {} runs, {} glyph records, capacity {capacity}",
        run_records.len(),
        glyph_records.len(),
    );
    state.batch = Some(ProofBatch {
        entity,
        mesh,
        instances,
        run_table,
        material,
        capacity,
        glyph_records,
        run_records,
        template,
        source_transform,
        fill_color,
    });
}

/// Appends one more run copy, growing the batch past its mesh capacity.
///
/// A capacity crossing re-creates the inert mesh at doubled capacity and
/// swaps it onto the batch entity **in the same frame** (D4: prepare runs
/// before queue, so the new mesh draws this frame). Logs the CPU cost — the
/// hitch number — and starts the frame-stepped screenshot capture.
pub fn force_capacity_growth(
    mut state: ResMut<ProofBatchState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut mesh_id: ResMut<ProofMeshId>,
    mut captures: ResMut<GrowthCaptures>,
    mut commands: Commands,
) {
    let Some(batch) = state.batch.as_mut() else {
        return;
    };
    let started = Instant::now();

    let next_run = batch.run_records.len();
    batch.run_records.push(proof_run_record(
        batch.source_transform,
        next_run,
        batch.fill_color,
    ));
    batch
        .glyph_records
        .extend(batch.template.iter().map(|record| GlyphInstanceRecord {
            run_index: next_run.to_u32(),
            ..*record
        }));

    let required = batch.glyph_records.len().to_u32();
    let grew = required > batch.capacity;
    if grew {
        let mut capacity = batch.capacity.max(1);
        while capacity < required {
            capacity *= 2;
        }
        let mesh = meshes.add(panel_text::inert_batch_mesh(capacity));
        commands.entity(batch.entity).insert(Mesh3d(mesh.clone()));
        mesh_id.0 = Some(mesh.id());
        batch.mesh = mesh;
        batch.capacity = capacity;
    }
    if let Some(mut instances) = buffers.get_mut(&batch.instances) {
        instances.set_data(batch.glyph_records.clone());
    }
    if let Some(mut run_table) = buffers.get_mut(&batch.run_table) {
        run_table.set_data(batch.run_records.clone());
    }

    let elapsed_ms = started.elapsed().as_secs_f32() * 1000.0;
    info!(
        "glyph batch proof growth: capacity {} (grew: {grew}), {} runs, {} glyph records, \
         CPU cost {elapsed_ms:.3} ms",
        batch.capacity,
        batch.run_records.len(),
        batch.glyph_records.len(),
    );
    captures.remaining = GROWTH_CAPTURE_FRAMES;
    captures.serial += 1;
}

/// Flips the vertex-pull debug staircase: each glyph quad lifts by its
/// recovered glyph index, making the slab-base subtraction visible.
pub fn toggle_debug_index(world: &mut World) {
    let Some(material_handle) = world
        .get_resource::<ProofBatchState>()
        .and_then(|state| state.batch.as_ref().map(|batch| batch.material.clone()))
    else {
        return;
    };
    let Some(mut materials) = world.get_resource_mut::<Assets<TextMaterial>>() else {
        return;
    };
    let Some(mut material) = materials.get_mut(&material_handle) else {
        return;
    };
    let debug_enabled = super::toggle_text_material_debug_glyph_index(&mut material);
    info!(
        "glyph batch proof: glyph-index staircase {}",
        if debug_enabled { "on" } else { "off" }
    );
}

/// Captures the growth frame and the two after it (N / N+1 / N+2) so the
/// no-blink requirement is verified by screenshots, not trusted.
fn capture_growth_frames(mut captures: ResMut<GrowthCaptures>, mut commands: Commands) {
    if captures.remaining == 0 {
        return;
    }
    let frame_offset = GROWTH_CAPTURE_FRAMES - captures.remaining;
    captures.remaining -= 1;
    if let Err(error) = std::fs::create_dir_all(SCREENSHOT_DIR) {
        warn!("glyph batch proof: cannot create {SCREENSHOT_DIR}: {error}");
        return;
    }
    let path = format!(
        "{SCREENSHOT_DIR}/growth_{:02}_frame_{frame_offset}.png",
        captures.serial
    );
    info!("glyph batch proof: capturing {path}");
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// Render-world logger: reports the batch mesh's `first_vertex_index` (its
/// vertex-slab element offset). Nonzero means the shader's slab-base
/// subtraction is exercised by this draw.
fn log_first_vertex(
    mesh_id: Res<ProofMeshId>,
    allocator: Res<MeshAllocator>,
    mut last: Local<Option<(AssetId<Mesh>, u32)>>,
) {
    let Some(id) = mesh_id.0 else {
        return;
    };
    let Some(slice) = allocator.mesh_vertex_slice(&id) else {
        return;
    };
    let first_vertex = slice.range.start;
    if *last == Some((id, first_vertex)) {
        return;
    }
    *last = Some((id, first_vertex));
    info!("glyph batch proof: batch mesh first_vertex_index = {first_vertex}");
}

/// Hand-written placement for run copy `index`: stacked below the source
/// label, yawed from the second copy on, `depth_nudge` = the copy index.
fn proof_run_record(source_transform: Mat4, index: usize, fill_color: Vec4) -> RunRecord {
    let drop = Vec3::new(0.0, -RUN_VERTICAL_STEP * (index + 1).to_f32(), 0.0);
    let yaw = if index >= 1 {
        Mat4::from_rotation_y(TILTED_RUN_YAW_RADIANS)
    } else {
        Mat4::IDENTITY
    };
    RunRecord {
        transform: Mat4::from_translation(drop) * yaw * source_transform,
        fill_color,
        render_mode: u32::from(RenderMode::Text),
        depth_nudge: index.to_f32(),
        oit_depth_offset: 0.0,
    }
}

/// `count` copies of the template records, each stamped with its run index.
fn stamped_runs(template: &[GlyphInstanceRecord], count: usize) -> Vec<GlyphInstanceRecord> {
    (0..count)
        .flat_map(|run| {
            template.iter().map(move |record| GlyphInstanceRecord {
                run_index: run.to_u32(),
                ..*record
            })
        })
        .collect()
}

/// The proof batch's material: default panel base, the shared atlas buffers,
/// the two record buffers, and the vertex-pull route switched on.
fn batch_material(
    atlas: &GlyphAtlasHandles,
    instances: Handle<ShaderBuffer>,
    run_table: Handle<ShaderBuffer>,
    fill_color: Vec4,
) -> TextMaterial {
    let mut base = constants::default_panel_material();
    base.alpha_mode = AlphaMode::Blend;
    super::batch_text_material(BatchTextMaterialInput {
        base,
        fill_color,
        render_mode: RenderMode::Text,
        oit_depth_offset: 0.0,
        supersample: true,
        aa_band: true,
        curves: atlas.curves.clone(),
        bands: atlas.bands.clone(),
        glyphs: atlas.glyphs.clone(),
        instances,
        run_records: run_table,
        debug_glyph_index: false,
    })
}

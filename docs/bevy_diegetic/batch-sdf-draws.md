# Batch SDF Panel Draws

> Status: design plan. This is a focused plan for replacing one
> `PanelSdfMesh` entity per panel surface with retained SDF surface batches,
> following the same store-and-record pattern used by batched text and
> analytic panel lines.

## Problem

Panel text and panel lines are now batched:

- text routes through `PathBatchStore`
- panel lines route through `PanelLineBatchStore`
- each store keeps one render entity per compatibility key
- per-run or per-primitive data lives in GPU buffers
- `DiegeticPerfStats` reports live batch counts, records, and uploads

Panel SDF geometry is still per-surface:

- `render/panel_geometry.rs` gathers `Rectangle` and `Border` commands
- each element surface becomes one retained `PanelSdfMesh`
- each divider rectangle becomes another retained `PanelSdfMesh`
- each `PanelSdfMesh` owns one rectangle mesh and one `SdfPanelMaterial`

That means screen-panel frames are cheap to author but still expand into many
surface draws. A Fairy Dust screen panel usually contributes at least:

- one outer frame border surface
- one inner background plus border surface
- optional separator rectangle surfaces

The new `DiegeticPerfStats::panel_geometry.sdf_quads` counter makes this
visible, but it intentionally reports draw surfaces, not batches.

## Goal

Replace retained per-surface SDF quads with retained SDF batches.

The target shape should match the text and panel-line renderer:

- one batch render entity per `SdfBatchKey`
- one inert capacity-sized mesh per batch
- one material per batch
- one record per SDF surface
- one or more storage buffers uploaded only when records change
- retained identity keyed by `(panel_entity, command_index)`
- cleanup when a panel disappears, hides, or stops emitting a surface
- perf stats showing both surface record count and batch count

The first useful milestone is not "one global SDF draw". It is "many
screen-panel SDF surfaces collapse to a small number of SDF batches without
changing visual order."

## Non-Goals

- Do not merge text, panel lines, and SDF surfaces into one draw. They use
  different shader paths and coverage data. They can share patterns and some
  compatibility key types, but not one universal batch entity.
- Do not batch image leaves in this plan. Image rendering has texture-binding
  constraints and should stay separate.
- Do not remove SDF quality features: rounded corners, per-side borders,
  clip expansion for AA, OIT offsets, and sorted-depth behavior must remain.
- Do not make color-only updates rebuild geometry or respawn entities.

## Current SDF Data

The existing per-surface path already computes most batch-record inputs:

- layout bounds
- fill color
- border widths
- border color
- corner radii
- active clip rect
- `DrawCommandDepth`
- world-space center
- world-space mesh size
- panel render layer
- panel material, lighting, sidedness, and shadow settings

Today those fields are split between:

- `PanelSdfSurface`, used as the retained reuse signature
- `SdfPanelUniform`, stored in one material per surface
- `Transform`, stored on one entity per surface
- `StandardMaterial.depth_bias`, stored per material

Batching needs to move the per-surface fields into records or material-table
slots and keep only true compatibility fields in the batch key.

## Batch Model

### SdfBatchKey

The first implementation should keep the key conservative:

```rust
struct SdfBatchKey {
    visual: VisualBatchKey,
    z_level: i8,
    layers: BatchRenderLayers,
    sorted_panel: Option<Entity>,
}
```

`visual` should represent only fields that require a distinct pipeline,
bind group, or render entity:

- alpha mode
- lighting mode
- sidedness
- shadow-caster mode
- render layers
- texture set, if a future SDF material uses textures

`z_level` selects the shared panel-geometry lane for a `DrawZIndex` level.

`sorted_panel` is the important correctness lever. In sorted transparency, one
batch is one `Transparent3d` item, so Bevy can sort only the whole batch. The
safe first implementation should keep sorted SDF batches panel-scoped:

- screen/sorted path: key includes `panel_entity`
- OIT path: key can later omit `panel_entity` because per-fragment ordering can
  use per-record OIT offsets

This mirrors the caution already present in the older element-batching plan:
cross-panel batching is easier under OIT than under sorted alpha.

### SdfSurfaceRecord

Each SDF surface needs a record similar in spirit to `RunRecord`:

```rust
struct SdfSurfaceRecord {
    transform: Mat4,
    half_size: Vec2,
    mesh_half_size: Vec2,
    corner_radii: Vec4,
    border_widths: Vec4,
    clip_rect: Vec4,
    fill_color: Vec4,
    border_color: Vec4,
    depth_nudge: f32,
    oit_depth_offset: f32,
    flags: u32,
}
```

The initial version can store colors directly in the record. That is enough to
batch normal Fairy Dust panel frames and separators.

A later material-table phase can replace `fill_color` and `border_color` with
material indices if we need per-surface PBR material fields without splitting
batches. That larger material-table work is related to
`docs/bevy_diegetic/element-batching.md`, but it is not required for the first
SDF batching milestone.

### Mesh

Use the same vertex-pulling approach as text and panel lines:

- batch mesh capacity is a power-of-two number of quads
- each SDF surface consumes one quad
- the vertex shader derives `record_index` from the local vertex index
- capacity tail records draw degenerate quads or are guarded in shader
- capacity growth replaces the mesh and buffer handles in an ordered pass

This avoids a mesh asset per surface and makes a batch one Bevy render entity.

### Shader

The current `sdf_panel.wgsl` should be refactored, not replaced wholesale.

Keep the SDF fragment math:

- rounded rectangle distance
- border/fill composition
- clip discard
- AA padding behavior
- OIT depth offset behavior
- PBR integration

Move per-surface data reads from the material uniform to the record table.
The vertex shader should output the local SDF point plus record index. The
fragment shader should load `SdfSurfaceRecord` and then run the same coverage
logic.

## Ordering Rules

SDF batching must preserve the draw-order model:

- `DrawZIndex` moves a surface between z-levels
- command ordinal still orders geometry inside a level
- line and text batch lanes still sit above the geometry lanes for that level
- OIT offsets must match the existing `DrawCommandDepth`
- sorted depth bias must keep the same relative ordering

The current per-surface path writes:

- `base.depth_bias = surface.draw_depth.depth_bias().get()`
- `SdfPanelUniform.oit_depth_offset = surface.draw_depth.oit_depth_offset()`

The batched path should instead use:

- batch material depth bias for the z-level's shared SDF lane
- per-record `depth_nudge` for ordinal ordering inside the batch
- per-record `oit_depth_offset` for OIT ordering
- record order sorted by `DrawCommandDepth` for sorted-alpha draw order inside
  one batch

If sorted-alpha correctness cannot be proven for cross-panel SDF batches, keep
sorted batches panel-scoped. That still collapses each screen panel from many
surface entities into one or a few batch entities.

## Visibility and Lifetime

Text batching already had bugs around hidden panels, so SDF batching should
start with this invariant:

> A hidden or despawned panel contributes zero SDF records.

The SDF batch store needs the same lifecycle shape as text and lines:

- `upsert_surface(key, surface_key, record)`
- `remove_surface(surface_key)`
- `remove_panel(panel_entity)`
- stale panel cleanup after reconcile
- empty batch despawn
- dirty flags for records and transforms

The `surface_key` should be:

```rust
struct SdfSurfaceKey {
    panel: Entity,
    command_index: usize,
}
```

If a panel rebuild changes command indices, stale cleanup removes old records
and inserts new ones. This matches the current retained `PanelSdfSurface`
identity and keeps the first implementation aligned with existing behavior.

## Performance Counters

Extend `DiegeticPerfStats::panel_geometry` from a surface count into SDF batch
stats:

```rust
pub struct PanelGeometryPerfStats {
    pub sdf_quads: usize,
    pub sdf_batches: usize,
    pub sdf_records: usize,
    pub sdf_uploads: usize,
}
```

Expected overlay rows in `diegetic_text_stress`:

- `batched analytic draws`: text + panel-line batches
- `sdf batches`: live SDF batch entities
- `sdf records`: live SDF surface records
- `sdf uploads`: storage-buffer uploads this frame
- `sdf surface draws`: optional old name during migration, equal to records

After the old path is removed, avoid calling records "draws" in the UI.

## Phased Plan

### Phase 1: Record Shape and Store

Add an inert `SdfSurfaceBatchStore` beside `panel_geometry.rs`.

Deliverables:

- `SdfSurfaceKey`
- `SdfBatchKey`
- `SdfSurfaceRecord`
- `SdfSurfaceBatch`
- `SdfSurfaceBatchStore`
- unit tests for insert, update, remove, panel cleanup, and empty-batch cleanup

Acceptance:

- no renderer behavior changes
- `cargo nextest run -p bevy_diegetic` passes

### Phase 2: Extract Existing Surface Resolution

Split `build_sdf_quad` into two steps:

- resolve authored/layout data into a pure `ResolvedSdfSurface`
- either build the old per-surface quad or route it into the batch store

The old renderer remains active.

Acceptance:

- existing `panel_geometry` tests still pass
- no visual output changes
- `PanelGeometryPerfStats::sdf_quads` still reports live per-surface quads

### Phase 3: Batched SDF Shader and Batch Entity

Add a batch material and shader path:

- inert capacity mesh
- storage buffer of `SdfSurfaceRecord`
- vertex-pulled quad expansion
- fragment logic equivalent to `sdf_panel.wgsl`
- batch bounds from all records
- render layers copied from the key
- shadow caster mode copied from the key

Keep the old path behind a fallback flag while the shader is validated.

Acceptance:

- one simple screen panel renders identically under old and new paths
- rounded corners, border widths, fill alpha, and clip rects match
- hidden panels emit no SDF records

### Phase 4: Route Production SDF Surfaces Through Batches

Replace `spawn_sdf_quad` with batch-store routing.

Retain or rewrite these current behaviors:

- identical surfaces do not upload
- color-only changes update records, not meshes
- geometry changes update records and bounds
- removed surfaces delete records
- removed panels delete records
- surface-shadow off adds no shadow caster contribution

Acceptance:

- `diegetic_text_stress` reports SDF batches and records
- screen-panel SDF surface count drops from many draw entities to a small batch
  count
- current panel geometry tests pass or are updated to assert records instead of
  child quads

### Phase 5: Ordering and Transparency Tests

Add focused tests before deleting the old path:

- z-index moves an SDF surface between SDF batches
- record order matches `DrawOrderProjection`
- OIT offsets match the old per-surface path
- line and text lanes still sort above default SDF geometry
- screen and world render layers split batches
- sorted-alpha path stays panel-scoped unless proven safe
- OIT path can batch across panels only after a visual and test pass

Acceptance:

- draw-order tests cover SDF, line, and text together
- `panel_draw_order` still demonstrates z-index layering correctly

### Phase 6: Remove Old Per-Surface Entity Path

Delete or quarantine:

- per-surface `PanelSdfMesh` spawn path
- per-surface `SdfPanelMaterial` asset churn
- tests that assert child SDF entity counts instead of rendered records

Keep:

- interaction mesh path
- SDF math and AA clip expansion
- retained panel geometry stats

Acceptance:

- `cargo nextest run -p bevy_diegetic`
- `cargo check -p bevy_diegetic --examples`
- `cargo clippy --workspace --all-targets`
- `diegetic_text_stress` shows lower SDF batch count than old SDF surface
  count
- `units` still renders ruler frames and panel chrome cleanly

## Open Decisions

### Material Table Timing

The first batching pass can store fill and border colors directly in
`SdfSurfaceRecord`. That is enough for current Fairy Dust panels.

A shared material table becomes necessary if we want all scalar PBR material
fields to vary per surface without splitting batches. The larger
`element-batching.md` plan already describes that design. We should not take
that complexity in the first SDF batching milestone unless a real example needs
it.

### Cross-Panel Batching

For sorted alpha, panel-scoped SDF batches are the safe first implementation.
For OIT, cross-panel SDF batches should be possible because per-fragment OIT
uses record offsets. That should be a later optimization with screenshots and
tests.

### Shared Batch Infrastructure

Text, panel lines, and SDF will have similar stores. We should resist a shared
generic batch abstraction until the SDF implementation lands and the real
duplication is visible. The important shared pieces are:

- compatibility key vocabulary
- capacity growth discipline
- visibility cleanup tests
- perf stats shape

## Expected Result

In `diegetic_text_stress`, the current stats can show multiple SDF surface
draws for screen panels even when text is only two batches. After this work,
those SDF surfaces should become a small SDF batch count plus a larger SDF
record count.

That will make the panel explain what is actually happening:

- text batches are split by render layer, material, alpha, lighting, sidedness,
  z-level, shadow mode, and compatible camera layers
- panel-line batches are separate analytic path batches
- SDF panel chrome has its own batch store
- screen/world render layers still split batches where they must

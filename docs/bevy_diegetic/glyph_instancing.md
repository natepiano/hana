# Glyph instancing — one shared quad, per-glyph records

Option B from [`diegetic-text-perf.md`](diegetic-text-perf.md) (now retired —
A, C, and D landed there; this doc owns what remains). Goal: stop storing a
quad per glyph in per-run meshes. Keep one quad's worth of geometry, draw it N
times, and feed a per-glyph record table the GPU expands. No per-label meshes,
no per-label materials, one draw per batch per pass.

## Why — the waterfall says render-thread CPU

Baseline (2026-06-03, `diegetic_text_stress`, 100 world labels restrung every
frame, M2 Max, release, `with_perf_mode`, overlay `now / 5s-max` ms):

| Row        | now   | 5s-max |
| ---------- | ----- | ------ |
| `ms`       | 20.2  | —      |
| `layout`   | 0.09  | —      |
| `reconcile`| 0.06  | —      |
| `shaping`  | 0.34  | —      |
| `mesh`     | 0.11  | —      |
| `other`    | 1.92  | —      |
| `wait`     | 17.65 | —      |
| `render`   | 19.7  | —      |
| `assets`   | 2.01  | 11.07  |
| `prep`     | 8.70  | 18.31  |
| `gpu wait` | 2.60  | 14.83  |
| `graph`    | 6.36  | 10.30  |

The frame is paced by the render thread's CPU, not the GPU (`gpu wait` 2.60)
and not the main thread (~2.6 ms of work; `wait` is blocked time). The three
rows instancing attacks, through one mechanism (fewer render entities, fewer
asset preparations, fewer draws):

- **`prep`** — extract / prepare / specialize / queue / batch cost scales with
  extracted mesh entities. 100 label meshes + 100 materials → a handful of
  batch entities.
- **`graph`** — encode cost scales with draw calls × passes. 100+ draws per
  pass (main, prepass, shadow) → 1 per batch per pass.
- **`assets`** — 100 mesh assets re-prepared per frame (the stress test
  rewrites every label) → one record-buffer write per batch (~24 KB for 600
  glyphs vs ~96 KB of vertex data across 100 separate assets).

What instancing does **not** move: fragment cost (same pixels covered, same
per-pixel curve evaluation — see the per-run-instancing note in
[`slug_fx.md`](slug_fx.md#render-run-instancing-is-not-a-lever-here), which
rejects *run-level* quads for fx; glyph-granularity records keep per-pixel
cost identical), `gpu wait`, and the main-thread rows.

Terminology: `slug_fx.md` says "Slug currently instances per glyph" — loose
usage. Today each glyph is 4 expanded vertices in the run's mesh; there is no
instance buffer anywhere. This doc adds the real thing.

## Today's model (what gets replaced)

- **Mesh per run.** `RunMeshBuilder::push_glyph`
  (`src/text/slug/render/run_data.rs:166-207`; builder struct at `:79-85`)
  emits 4 vertices + 6 indices per glyph: `POSITION` (layout-space x/y, z=0),
  `NORMAL` (+z), `UV_0` (padded quad UVs), `UV_1.x` = global atlas record
  index. Clipping happens here, at build time: a fully clipped glyph emits
  nothing, a partially clipped one gets its rect *and* UVs shrunk
  (`run_data.rs:125-152`).
- **Material per run.** `update_panel_text_geometry`
  (`src/render/panel_text/mesh_spawning.rs:65`) builds a
  `TextMaterial = ExtendedMaterial<StandardMaterial, TextExtension>`
  (`src/text/slug/render/material.rs:43`) per label: bind group entries 100
  (uniform: `fill_color`, `render_mode`, `oit_depth_offset`, `supersample`,
  `aa_band`) and 101/102/103 (the three shared atlas buffers). The
  `StandardMaterial` base carries per-run `depth_bias` (from
  `command_index`, the M2 Geometry-mode layering), `alpha_mode`, `unlit`,
  cull mode — all from the panel base material + the three per-label cascade
  overrides (`TextAlpha` / `TextLighting` / `TextSidedness`).
- **Entity per run.** One `DiegeticTextMesh` child per label with
  `Mesh3d` + `MeshMaterial3d<TextMaterial>` + `RunStorageKey` +
  `RenderLayers`, `NotShadowCaster` when `GlyphShadowMode::None`
  (`mesh_spawning.rs:339-371`). Glyph positions are baked into the mesh in
  layout space; world placement comes from the label's propagated
  `GlobalTransform`.
- **Shared atlas (option C, keep).** `GlyphOutlineCache`
  (`src/text/slug/runtime/run.rs:142-150`) holds the append-only
  curves / bands / glyph-record vectors; `commit_glyph_atlas`
  (`src/text/slug/runtime/glyph_cache.rs:206-245`) uploads them in place only
  when `revision` grows. Untouched by this plan — instancing sits on top of it.

Stress-test counts: 100 mesh entities, 100 material assets, 100 mesh assets,
~600 glyphs ≈ 2 400 vertices, 100+ draws in each of the main / prepass /
shadow passes.

## Target model

Two GPU tables plus a per-batch entity:

```text
GlyphInstanceRecord (one per glyph, 40 B)         RunRecord (one per run, 96 B)
  rect_min:  vec2<f32>   // layout space, clipped   transform:   mat4x4<f32> // label world matrix
  rect_size: vec2<f32>                              fill_color:  vec4<f32>
  uv_min:    vec2<f32>   // padded quad UVs         render_mode: u32         // Text / PunchOut
  uv_size:   vec2<f32>                              depth_nudge: f32
  atlas_idx: u32         // GlyphRecord index       _pad:        vec2<f32>
  run_idx:   u32         // RunRecord index
```

GlyphInstanceRecord is 40 B under std430 (vec2 alignment is 8; 4×8 + 2×4 =
40, already stride-aligned — no padding field). RunRecord: 64 + 16 + 4 + 4 +
8 explicit pad = 96, 16-aligned. Transform encoding: `Mat4` — glam has no
`Mat3x4` and the existing record structs only use `Vec4` / `UVec4` through
`ShaderType`, so `Mat4` is the no-surprises choice; pack to 3×`Vec4` only if
measurement justifies it. Both structs live in
`src/text/slug/render/packing.rs` next to `CurveRecord` / `BandRecord` /
`GlyphRecord`, deriving `ShaderType` the same way. Step 1 adds compile-time
size assertions (`const _: () = assert!(size_of::<GlyphInstanceRecord>() ==
40)`, RunRecord `== 96`) and a ShaderType round-trip check.

If `MotionVectorPrepass` is ever enabled (no bevy_diegetic example uses it
today), the run table grows a `previous_transform` column and the prepass
vertex stage emits `previous_world_position` under
`#ifdef MOTION_VECTOR_PREPASS`; until then the output is omitted and the
column does not exist.

- **Batch** = one render entity + one material + one mesh per *batch key*
  (below). Runs own contiguous record ranges inside their batch's instance
  buffer; the run table holds everything per-run that varies inside a batch.
- **Vertex pulling, not step-mode instancing.** The batch mesh is inert: a
  capacity-sized vertex buffer (zeroed positions, written only when capacity
  grows) and a static `6 × capacity` index pattern. The vertex shader derives
  `glyph = vertex_index / 4`, `corner = vertex_index % 4`, reads the two
  tables, and computes world position = `run.transform × (rect corner)`. The
  fragment shader is today's `slug_text.wgsl` unchanged below the record
  lookup (atlas index now arrives from the instance record instead of
  `UV_1.x`).

Why vertex pulling instead of hardware instancing: bevy's `Material` path
owns the draw call (instance ranges come from its batching of entities), so a
step-mode instance buffer doesn't fit without a fully custom pipeline. Vertex
pulling expresses "draw one quad N times, GPU expands a table" inside the
`Material` framework — and on Apple silicon both compile to the same buffer
fetches. This keeps PBR lighting, the prepass/shadow path, and OIT working
through `ExtendedMaterial` instead of reimplementing them.

Frame flow in steady state (stress test), with schedule anchors:

1. shaping → `PreparedPanelText` per label (unchanged)
2. geometry write — `update_panel_text_geometry` (PostUpdate,
   `.before(TransformSystems::Propagate)`, unchanged slot) writes the run's
   glyph records into its batch range + the non-transform `RunRecord`
   fields; atlas commit stays inside it (records only index the atlas)
3. transform write — new system, PostUpdate
   `.after(TransformSystems::Propagate)`, copies each label's
   `GlobalTransform` into its `RunRecord` slot, gated on
   `Ref::is_changed` (decision 5)
4. Aabb union — new system `.after(transform write)`,
   `.after(VisibilitySystems::CalculateBounds)`,
   `.before(VisibilitySystems::CheckVisibility)` (decision 5)
5. buffer commit — new system `.after(geometry write).after(transform
   write)`, one `ShaderBuffer` write per dirty batch, still in PostUpdate so
   extraction sees this frame's data
6. render → per batch: one entity extracted, one draw per pass

Text edit = a range write. Label move = one `RunRecord` write. Neither
touches a mesh asset.

## Buffer-write granularity (sets expectations for the property below)

`ShaderBuffer` uploads are whole-asset (`set_data` takes the full byte
vector — same mechanism as the atlas commit at `glyph_cache.rs:217-228`), so
"a range write" on the CPU side still re-uploads that batch's whole record
buffer. That is the accepted first cut: 600 glyphs ≈ 24 KB per upload versus
today's 100 separate mesh-asset preparations — the win is fewer asset
preparations and draws, not bytes. Consequences, stated so nobody re-derives
them mid-implementation:

- An unchanged frame writes nothing (dirty flag never set) — the Phase D
  property holds.
- One edited run dirties **its batch's** record buffer (whole-buffer upload),
  never other batches, never any mesh asset.
- A scrolled clip rect changes that run's records every scrolled frame —
  scroll frames upload the batch buffer. Known, bounded, measured in Step 2.
- Sub-buffer range writes are a later optimization, justified only by a
  measured cost.

## Design decisions

### 1. Pipeline route — vertex pulling inside `ExtendedMaterial` (verified)

`TextExtension` grows two storage bindings (instances at 104, runs at 105).
`impl MaterialExtension for TextExtension` today overrides only the two
fragment stages (`material.rs:81-85`); Step 1 adds `vertex_shader()` and
`prepass_vertex_shader()`, both returning a new
`src/text/slug/shaders/slug_text_vertex_pull.wgsl` (one file, one shared
nudge/expand function — the prepass and main stages must not drift).
Verified against bevy 0.19 source: `MaterialExtension` declares both
(`bevy_pbr/src/extended_material.rs:36,66`) and `ExtendedMaterial` routes
them into the main, prepass, and shadow pipelines (`:317-370`), with
extension specialization applied after base (`:410-435`). Storage-buffer
reads in the vertex stage are an established bevy pattern on Metal (the
wireframe example does exactly this). The alternative — a custom
`SpecializedRenderPipeline` with hand-rolled phase items — buys true
step-mode instancing and full control, at the cost of reimplementing PBR
view bindings, prepass, shadow, and OIT integration. Not justified by any
measured cost; revisit only if the Material route hits a wall in Step 1.

### 2. Batch key — what splits draws

```text
(BaseMaterialId, alpha_mode, lighting, sidedness, shadow_mode, RenderLayers)
```

Everything that is a pipeline/material/entity-level property today. The three
per-label cascade overrides survive as batch splits — they are deliberate
features (kept twice against deletion), not batching casualties. In the
stress test: the 100 world labels match on all six fields → 1 batch; the
screen-space status overlay (`DiegeticPanel::screen()`, unlit
`screen_panel_material()`, own propagated `RenderLayers` —
`screen_space/mod.rs:285-295`) differs on layer + base material + lighting
→ 1 more batch, drawn only by the screen-space camera. Worst realistic case
(the showcase examples): a handful of batches. The key is a tuple — a run
differing on several fields at once still lands in exactly one batch.
`fill_color`, `render_mode`, and the depth nudge move *into* the records,
so they do not split.

**Base-material keying — intern by value (decided, review cycle 2).**
`StandardMaterial` is neither `Hash` nor `Eq`, and
`DiegeticPanel::text_material` stores an `Option<StandardMaterial>` *by
value* (`diegetic_panel.rs:113`). A small interner compares the fields panel
text materials actually carry — `base_color`, texture handles (by
`AssetId`), `emissive`, `metallic`, `perceptual_roughness`, `reflectance`,
`unlit`, `double_sided`, `cull_mode`; floats bitwise — and assigns a
`BaseMaterialId`. Panels that never customize share the default id (the
common case batches automatically; the codebase survey found ~30
`text_material` call sites, all cloning the default and overriding at most
`unlit`). Any future setter that widens the customizable surface must extend
the comparison — note that on the setter. Rejected: keying on
`Handle<StandardMaterial>` (forces an authoring API change) and hashing a
silent field subset (merges panels that differ in an unhashed field).

### 3. Per-run depth layering (M2) — depth nudge in BOTH vertex stages

Today each run's `StandardMaterial.depth_bias` (derived from
`command_index`) orders coplanar panel text in Geometry mode. In bevy that
field is a per-material *sort* bias — meaningless inside a single batched
draw, and already a no-op under OIT (see memory
`project_depth_bias_oit_bug`). Replacement: a per-run `depth_nudge` applied
in the vertex shader as a small view-space z offset for the non-OIT path,
zero under OIT (matching today's `oit_depth_offset: 0.0` policy at
`mesh_spawning.rs:273-278` — a positive OIT offset pulls text through
occluders).

**The nudge must be applied identically in the main vertex shader AND the
prepass vertex shader.** The depth prepass writes depth from the prepass
vertex stage; if only the main pass nudges, main-pass fragment depth diverges
from prepass depth and depth-equal testing rejects the fragments. Both
stages read the same `RunRecord.depth_nudge` through the shared WGSL
function in `slug_text_vertex_pull.wgsl` (decision 1), so this is one code
path, not duplicated logic. Step 3 verifies Geometry-mode layering against
the `panel_rendering` example before the old path is removed.

### 4. Buffer ownership and range allocation

The batch store is a **new field in the `GlyphCache` resource**, alongside
the outline cache and `run_storage`. Sketch (final field names are the
implementer's):

```rust
struct GlyphBatchStore {
    batches:   HashMap<BatchKey, GlyphBatch>,
    run_index: HashMap<RunStorageKey, BatchKey>,  // which batch a run is in
}
struct GlyphBatch {
    entity:        Option<Entity>,               // the batch render entity
    glyph_records: Vec<GlyphInstanceRecord>,
    run_records:   Vec<RunRecord>,
    runs:          Vec<(RunStorageKey, Range<u32>)>, // derived ranges
    instances:     Handle<ShaderBuffer>,
    run_table:     Handle<ShaderBuffer>,
    mesh:          Handle<Mesh>,                 // inert, capacity-sized
    capacity:      u32,
    dirty:         bool,
}
```

First cut — rebuild, don't allocate:

- Ranges are **derived state**, recomputed from the live run set. On any
  structural change (run joins or leaves a batch, a run's glyph count
  changes), rebuild that batch's record vectors on the CPU — all glyph
  records and the run table — recompute range offsets, and rewrite the
  buffers in place. Records are tiny (600 glyphs ≈ 24 KB); the rebuild is
  microseconds. No free-list, no compaction bookkeeping, no stale-range
  class of bug: a despawn-plus-spawn in one frame is just two inputs to the
  same rebuild.
- A same-count edit (the stress case: "07 412" → "07 413") writes the run's
  range in the CPU vector and marks the batch dirty — no rebuild.
- A fully clipped glyph **emits no record** (matching today's silent skip at
  `run_data.rs:134`); the count change triggers the rebuild path. No
  degenerate placeholder records in the first cut — revisit only if rebuild
  frequency measures hot.
- Capacity: the inert mesh and record buffers are allocated with headroom
  (start at the scene's initial glyph count rounded up, grow by doubling).
  A capacity crossing re-creates buffer + mesh — a hitch candidate; Step 1
  measures one doubling so the cost is a known number, and Step 2's
  label-add stress watches hitch frequency.
- A debug assertion guards against two writers claiming one storage key
  (toggle-era safety, decision 10).

### 5. Transforms and culling

Glyph records stay in layout space; world placement is the per-run `Mat4`.
`update_panel_text_geometry` runs `.before(TransformSystems::Propagate)`
(`panel_text/mod.rs:94-96`), so it cannot read this frame's
`GlobalTransform` — the **transform-write system** (frame-flow step 3) runs
`.after(TransformSystems::Propagate)` in PostUpdate, copies each routed
label's `GlobalTransform` into its `RunRecord` slot, and marks the batch
dirty only when the matrix actually changed (`Ref::is_changed` gating).
The buffer commit (frame-flow step 5) runs after both writers, still in
PostUpdate, so extraction sees this frame's records. Step 2's gate includes
a moving-label case (orbiting or scrolled panel) to prove there is no
one-frame transform lag.

Culling: the batch entity **must carry a real `Aabb` before Step 4 ships**,
and bevy will not maintain it — `CalculateBounds` recomputes an `Aabb` only
from mesh data, and on a capacity growth (the inert mesh is re-created) it
would install a zero-extent box computed from the zeroed positions. So the
**Aabb-union system** (frame-flow step 4) writes the component manually —
union of each run's layout-space bounds × run transform, once per frame
when any transform or membership changed — ordered
`.after(VisibilitySystems::CalculateBounds)` and
`.before(VisibilitySystems::CheckVisibility)` so the manual union always
wins the frame, including mesh-growth frames. `NoFrustumCulling` is
acceptable scaffolding during Step 2 only; an unculled all-text batch is
rasterized in every pass including shadow cascades, which is exactly the
cost this plan exists to remove. Known caveat to carry, not solve now: one
distant label inflates the union so the batch never culls — if that case
turns real, split the batch key by a coarse spatial bucket. Per-glyph
culling is the GPU's problem (degenerate quads cost nothing after the
vertex stage).

### 6. Clipping

Stays CPU-side at record-write time, exactly where the mesh builder clips
today (`run_data.rs:125-152`) — including the partial-clip rect+UV shrink.
A clipped-away glyph emits no record (decision 4). Scroll panels therefore
rewrite records on scrolled frames — see Buffer-write granularity. The
clip-must-not-inflate engine invariant is untouched.

### 7. Shadows

`shadow_mode` is in the batch key: `Cast` batches omit `NotShadowCaster`,
`None` batches carry it — same entity-level mechanism as today, one entity
per batch instead of per run. The shadow pass uses the prepass pipeline, so
the overridden `prepass_vertex_shader` (decision 1) serves it automatically:
silhouettes stay glyph-accurate. Ghost text (alpha-0 + `Cast`) keeps working
because `fill_color` is per-run record data.

### 8. Alpha mutation path

`update_panel_text_alpha` today mutates the run material's `alpha_mode` in
place. Under batching, `alpha_mode` is a batch-key field, so an alpha-mode
change *moves the run between batches*: one operation
(`move_run(run, old_key, new_key)`) that removes the run from the source
batch and adds it to the destination — both sides take the decision-4
rebuild path, so no range bookkeeping. A novel destination key specializes
its pipeline once and caches it (bevy's normal amortization); note it in
Step 2 measurements, don't fear it. Alpha *value* changes (the common case —
fades via `TextAlpha`) ride in `fill_color.a` per record and stay in-batch.

### 9. Empty runs and despawn

The R10 empty-text path (`mesh_spawning.rs:163-170`) and the
`On<Remove, DiegeticTextMesh>` storage-free observer translate to
remove-from-batch (the decision-4 rebuild). The observer moves to the label
entity (there is no per-run mesh child to observe anymore). Because ranges
are derived state, observer-vs-system ordering inside one frame cannot
corrupt them — the rebuild sees only the live run set.

### 10. The Step-2 toggle — what it gates

The toggle is a resource —
`enum TextGeometryPath { PerRunMeshes, BatchedRecords }` — gating **which
geometry path runs, whole-system**: `PerRunMeshes` → today's
`update_panel_text_geometry` mesh path; `BatchedRecords` → the batch path
plus the transform-write / Aabb / commit systems. Exactly one path executes
per frame — never per-run routing, so cascade observers, storage keys, and
despawn observers act on one world model at a time. Under `BatchedRecords`
**all** panel-text runs route through the batch store (world labels are
one-element panels; there is no separate kind to filter on) — what Step 2
defers to Step 3 is not routing but dynamics and verification: re-keying on
a later cascade change, the depth nudge, punch-out verification, and the
examples sweep. Until Step 3, flip the toggle only in the Step-2 gate
examples. Flipping at runtime tears down the inactive path's products
(per-run mesh children one way, batch entities the other — the existing
`On<Remove, DiegeticTextMesh>` observer handles the former, verified safe).
Batch entities carry a marker component (`DiegeticTextBatch`) so BRP
inspection can tell the two paths apart.

## What must not regress (acceptance checklist)

| Feature | Verified by |
| --- | --- |
| Per-label `TextAlpha` / `TextLighting` / `TextSidedness` (memory: `project_labels_control_own_alpha` — deleted twice as "unused"; they are features) | cascade-override example/tests + Step 3 batch-move test |
| `GlyphShadowMode::None` / `Cast` + ghost text (alpha-0 cast) | `diegetic_text_stress` (casting labels) + shadow screenshot |
| Punch-out render mode | punch-out usage sweep (`GlyphRenderMode::PunchOut` call sites) |
| Clip rects | scroll example: glyphs clip at rect while scrolling, no stale/missing glyphs |
| OIT on (`StableTransparency`) and off; PBR lighting (panels are physical — never unlit-by-default) | `side_by_side` + stress test with OIT toggled |
| Geometry-mode coplanar layering (M2) | `panel_rendering` |
| Per-panel `RenderLayers` | `viewports` |
| Dynamic text edits route as range writes | `typography` arrow-key word scrubber + stress test |
| Typography debug overlay still draws (reads `PanelTextLayout` via `ComputedWorldText`, not meshes) | `typography` overlay compared before/after Step 4 |
| The Phase D no-op-no-work property, restated for batching: unchanged frame → zero buffer writes; one edited run → only its batch's record buffer uploads, no mesh assets ever | Step 2 instrumentation |

## Incremental plan (measure as you go; STOP for review at each gate)

- **Step 0 — Plan review.** DONE 2026-06-03 (two-cycle team review; see the
  Review log). Baseline column above is captured.
- **Step 1 — Vertex-pulling proof.** `slug_text_vertex_pull.wgsl` + the two
  storage bindings and vertex-stage overrides on `TextExtension`; the two
  record structs in `packing.rs`; a minimal proof example (e.g.
  `examples/glyph_batch_proof.rs`, removed or repurposed at Step 4) spawns
  one hard-coded batch entity directly — `Mesh3d` (inert mesh) +
  `MeshMaterial3d<TextMaterial>` with hand-written records, beside the
  existing renderer. Gate: correct position/UV/atlas lookup, lit, OIT
  on/off, casts a shadow, prepass compiles **with the depth nudge applied in
  both vertex stages**; `ShaderType` round-trip + compile-time size
  assertions (40 / 96) pass; one capacity-doubling reallocation measured
  (the hitch number). No batching logic yet, nothing routed. Verifies the
  real unknowns (Material-framework vertex override +
  mesh-with-inert-vertices + `Mat4`-in-record) before structural work.
- **Step 2 — Batch store + routing.** `GlyphBatchStore` in `GlyphCache`;
  the transform-write, Aabb-union, and buffer-commit systems on the
  frame-flow anchors; all runs routed behind the decision-10 toggle (full
  batch key from day one — cascade *values* are read at insert; only
  re-keying on later changes waits for Step 3); `Aabb` or
  `NoFrustumCulling` scaffolding. **Proof counters land here too** (they are
  how the gate and the results table get their numbers):
  - *Library:* `DiegeticPerfStats` gains batch stats — batch count, total
    runs, total glyph records, buffer uploads this frame — published as
    diagnostics like `reconcile_ms`, so they read over BRP.
  - *Example:* the stress overlay gains a row showing them
    (`batches 2 · runs 100 · glyphs ~600 · uploads N`), and the example's
    render-app plugin counts per-view phase items (Transparent3d / OIT /
    shadow) after `RenderSystems::PhaseSort` into the same shared-atomics
    channel the waterfall uses — the draws-per-pass number on screen.
    Expected: ~130 phase items toggle-off → 2 toggle-on.

  Gate: **parity** = BRP screenshots at
  identical camera/window across a toggle flip for the stress test and one
  scroll example, compared visually plus a diff-image sanity check
  (byte-exact is not expected — OIT accumulation order and float paths
  differ; document any visible delta and treat it as a finding); a
  moving-label case shows no one-frame transform lag; scrolled-clip frames
  upload only the expected batch buffer; label add/remove/re-add under both
  toggle states leaks no storage (debug assertion from decision 4);
  slab-error log watch (memory: `project_text_alpha_slab_errors`);
  waterfall measured both ways in the same session, deltas recorded in the
  table below.
- **Step 3a — Batch-membership dynamics.** Cascade re-keying via the
  decision-8 `move_run` path; shadow modes. Gate: cascade/batch-move tests
  (alpha-mode change moves the run, value-only change stays in-batch) +
  shadow screenshots against baseline.
- **Step 3b — Per-record fields and shaders.** Punch-out verification; clip
  move; depth-nudge layering verified against `panel_rendering`; the
  acceptance-table examples sweep. Gate: acceptance checklist above, full
  test suite, clippy (nursery + pedantic).
- **Step 4 — Delete the per-run mesh path.** `RunMeshBuilder`,
  `RunRenderData`, per-run materials, the per-run mesh child spawn/despawn,
  the toggle, `NoFrustumCulling` scaffolding (real `Aabb` required from here
  on). `RunStorage` (the struct) is deleted; `RunStorageKey` stays — it is
  the run identifier in `GlyphBatchStore.run_index`. Gate: tests green,
  typography overlay compared before/after (it reads `PanelTextLayout` via
  `ComputedWorldText`, not meshes), final waterfall column recorded, doc
  updated, `emoji.md` annotated that color-glyph layers land as records
  with a brush field, not layer-quads in run meshes.

## Results table (fill as steps land)

| Measure | Baseline (2026-06-03) | After 2 (toggle on) | After 4 |
| ------- | --------------------- | ------------------- | ------- |
| `ms`    | 20.2                  |                     |         |
| `fps`   | 51                    |                     |         |
| `ms` paused (idle floor) | (capture at Step 2) |       |         |
| `wait`  | 17.65                 |                     |         |
| `render`| 19.7                  |                     |         |
| `assets`| 2.01 / 11.07          |                     |         |
| `prep`  | 8.70 / 18.31          |                     |         |
| `gpu wait` | 2.60 / 14.83       |                     |         |
| `graph` | 6.36 / 10.30          |                     |         |
| render entities (text) | ~100 world + overlay runs |       |         |
| draws per pass (text)  | ~100 world + overlay runs |       |         |
| batches / runs / glyphs | — (no batch store yet)   |       |         |

The last three rows come from the Step-2 proof counters: batch stats from
`DiegeticPerfStats` (BRP-readable, shown on the overlay), phase-item counts
from the example's render-app plugin.

Success = the render block stops pacing the frame: `assets` near zero steady,
`prep` and `graph` cut hard enough that `render` approaches the GPU-bound
floor (`gpu wait` + residual encode). Exact targets get set from Step 2's
measured deltas, not promised up front. `gpu wait`, `shaping`, `layout` are
expected not to move.

## Risks and open questions

- **Mesh with inert vertices** — bevy's mesh pipeline specializes on the
  vertex layout, not vertex values, and batching/preprocessing never
  introspect positions, so a zeroed-`POSITION` buffer should pass; Step 1
  exists to prove it. The entity-level `Aabb` is mandatory because nothing
  can be derived from those positions (decision 5).
- **Non-OIT transparent sort** — within one batched draw there is no per-run
  sort; OIT makes order irrelevant, the non-OIT path leans on the depth
  nudge (decision 3). If a real ordering case falls through, the batch key
  can grow a coarse layer field — measured, in Step 3.
- **Atlas interaction** — records reference atlas indices, so a mid-frame
  atlas append must commit before batch buffers (same ordering
  `update_panel_text_geometry` enforces today at `mesh_spawning.rs:88-95`;
  the frame-flow anchors preserve it — atlas commit in step 2, batch commit
  in step 5).
- **`text_alpha` slab history** — per-run churn produced slab spam before
  (memory: `project_text_alpha_slab_errors`); the batch store removes the
  per-run assets entirely, but Step 2 watches the log with the toggle on.
- **emoji.md color-glyph path** — the layered-color-glyph design in
  [`emoji.md`](emoji.md) assumes per-run meshes ("glyph → N quads, one per
  layer"). Under instancing a color layer becomes N records with a brush
  field. Not this plan's work; Step 4 adds the cross-reference note to
  `emoji.md` so the two plans don't diverge silently.

## Review log (team review — auto-recorded)

**Cycle 1.** Mechanical/determined findings incorporated without prompting:
push_glyph line ref fixed; partial-clip rect+UV shrink documented;
transform-write system added after propagation (was a contradiction with
`.before(Propagate)` ordering); prepass depth-nudge parity requirement
(decision 3); buffer-write granularity section + no-op property restated;
rebuild-don't-allocate range strategy + clipped-glyph emits-no-record
decided (decision 4); capacity headroom + doubling-cost measurement
(Step 1 gate); `Aabb` mandatory before Step 4, `NoFrustumCulling` dev-only
(decision 5); alpha-move operation named + specialization amortization
(decision 8); derived-ranges despawn safety (decision 9); toggle scope
defined whole-system + `DiegeticTextBatch` marker (decision 10); motion
vector conditional + future `previous_transform` column (Target model);
`Mat4` transform encoding + size assertions (Target model); parity gate made
operational (Step 2); acceptance checklist mapped to examples; results table
gained idle-floor / entity-count / draws rows; `RunStorage` end state
specified (Step 4); typography-overlay independence noted (Step 4); emoji.md
cross-doc risk added. Verified bets recorded in decision 1 (ExtendedMaterial
vertex/prepass routing, Metal vertex-stage storage buffers, OIT
compatibility, GPU-preprocessing compatibility).

**Cycle 2.** Adversarial verification confirmed the ShaderBuffer
whole-asset-upload claim, the `text_material`-by-value claim, the toggle
teardown safety (`On<Remove>` fires synchronously before flush), and every
cited line ref including the bevy `extended_material.rs` ones. Corrections
and refinements incorporated: GlyphInstanceRecord size corrected 48 → 40 B
(std430; the ~28 KB figures → 24 KB); frame flow gained explicit schedule
anchors (geometry write before Propagate, transform write after, Aabb union
between `CalculateBounds` and `CheckVisibility`, buffer commit after both
writers); the `CalculateBounds` zero-extent-Aabb-on-mesh-growth hazard
documented with the ordering that defeats it (decision 5); D1 resolved —
intern-by-value, evidence: ~30 `text_material` call sites all clone the
default and override at most `unlit`, no texture customization (decision 2);
Step 2 routing inconsistency fixed — all runs route under the toggle, full
batch key at insert, only re-keying dynamics defer to Step 3 (decision 10);
toggle resource named (`TextGeometryPath`); record structs placed in
`packing.rs`; `GlyphBatchStore` sketch added (decision 4); Step 1 proof
mechanism named (`glyph_batch_proof` example); `vertex_shader()` /
`prepass_vertex_shader()` additions and the shared-WGSL-file requirement
spelled out (decision 1); `RunStorageKey` kept as the batch-store run id,
`RunStorage` struct deleted (Step 4); typography-overlay claim turned into a
before/after comparison gate; emoji.md note assigned to Step 4.

## Proposed user decisions

### D1 — Base-material batch key strategy (status: resolved, cycle 2)

Converged on **intern by value** and recorded in decision 2. Evidence: ~30
`text_material` call sites, all clone the default and override at most
`unlit`; no texture customization anywhere; `Handle` fields compare by
`AssetId`; bitwise float comparison is safe for static defaults. The
alternatives (Handle-based authoring API change; silent field-subset hash)
solve a problem the codebase doesn't have. Not surfaced for user review.

### D2 — Split Step 3 into 3a / 3b (status: resolved — approved 2026-06-03)

User approved the split; the plan above now reads Step 3a (batch-membership
dynamics: cascade re-keying, `move_run`, shadow modes) and Step 3b
(per-record fields and shaders: punch-out, clip, depth-nudge layering,
examples sweep), each with its own gate. Rationale preserved: the two
clusters fail independently and have different validation targets
(batch-move correctness vs shader/depth parity).

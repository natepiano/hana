# Glyph instancing — one shared quad, per-glyph records

> **Status: COMPLETE 2026-06-06.** Steps 0–4b all landed. Step 4b deleted the
> per-run mesh path, so batched records are the only text geometry path.
> Stress-test frame time 20.2 ms → 7.3 ms; text draws per pass ~100 → 1 per
> batch. Archived as the implementation record.

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
[`slug_fx.md`](../slug_fx.md#render-run-instancing-is-not-a-lever-here), which
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
PathInstanceRecord (one per glyph, 40 B)         RunRecord (one per run, 96 B stride)
  rect_min:  vec2<f32>   // layout space, clipped   transform:   mat4x4<f32> // label world matrix
  rect_size: vec2<f32>                              fill_color:  vec4<f32>
  uv_min:    vec2<f32>   // padded quad UVs         render_mode: u32         // Text / PunchOut
  uv_size:   vec2<f32>                              depth_nudge: f32
  atlas_index: u32       // PathRecord index
  run_index:   u32       // RunRecord index
```

PathInstanceRecord is 40 B under std430 (vec2 alignment is 8; 4×8 + 2×4 =
40, already stride-aligned). RunRecord: 64 + 16 + 4 + 4 = 88, rounded to a
96 array stride by encase (struct alignment 16) — **no explicit pad field**;
encase owns the padding, same as the existing `CurveRecord` / `BandRecord` /
`PathRecord` declarations. Transform encoding: `Mat4` — glam has no
`Mat3x4` and the existing record structs only use `Vec4` / `UVec4` through
`ShaderType`, so `Mat4` is the no-surprises choice; pack to 3×`Vec4` only if
measurement justifies it. Both structs live in
`src/render/analytic_paths/packing.rs` next to `CurveRecord` / `BandRecord` /
`PathRecord`, deriving `ShaderType` the same way. Step 1 adds compile-time
layout assertions against the **GPU layout, not the Rust layout** —
`size_of` measures the wrong thing; assert via `ShaderSize::SHADER_SIZE`
(`PathInstanceRecord` 40, RunRecord 96 — encase rounds struct size to its
16-byte alignment per WGSL rules; confirmed against encase 0.12's
`METADATA.min_size()`), plus a write-and-readback round-trip check.

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
  grows) and a static `6 × capacity` index pattern, **`Indices::U32`** (u16
  caps a batch at 16 384 glyphs — a silent ceiling; rule it out at creation)
  and **`POSITION` + `UV_0` + `UV_1` as the attribute set** — all zeroed;
  the shader ignores the *values*, but the attributes must be present: bevy
  sets the `VERTEX_UVS_A` / `VERTEX_UVS_B` shader defs from the mesh layout
  in both pipelines (`bevy_pbr/src/render/mesh.rs:3309`,
  `prepass/mod.rs:470,475`), and `slug_text.wgsl`'s fragment reads `in.uv` /
  `in.uv_b` behind those defs (`:481-505`) — a POSITION-only mesh would
  discard every fragment.
  The vertex shader derives glyph and corner from the vertex index — but
  bevy's mesh allocator packs meshes into shared slab buffers and draws with
  a nonzero `base_vertex`, which `@builtin(vertex_index)` **includes** on
  wgpu, so the derivation must subtract the slab base:
  `glyph = (vertex_index - mesh[instance_index].first_vertex_index) / 4`,
  `corner = ... % 4` — the same correction bevy's own wireframe shader makes
  (`wireframe.wgsl:67-68`). The index buffer is capacity-sized while the
  instance buffer is live-sized, so the shader guards
  `glyph >= arrayLength(&instances)` and emits a degenerate quad: the
  capacity tail draws as no-ops, and out-of-bounds robustness clamping
  (which would re-read the *last* record and re-blend its glyph once per
  spare slot) can never fire. It then reads the two tables and computes
  world position = `run.transform × (rect corner)`. Records carry no normal: the
  shader rotates layout-space +z by the run transform's rotation for the lit
  fragment path.
- **What the fragment receives, and its one bounded change.** The vertex
  stage forwards per-glyph data through the *existing* def-gated outputs —
  `uv` = the glyph-local quad UVs, `uv_b.x` = atlas record index as f32
  (exactly today's `UV_1.x` mechanism), `uv_b.y` = the run index as f32
  (recovered with `u32(floor(..))` — the same recovery today's
  `glyph_index` uses, `slug_text.wgsl:76-78`; quad-uniform float values
  interpolate exactly,
  the same invariant today's `uv_b.x` already relies on; integers are exact
  below 2²⁴). The fragment's per-run uniform reads (`fill_color`,
  `render_mode` — `slug_text.wgsl:472,509,516`) become run-table reads:
  `run_records[u32(floor(in.uv_b.y))]`, which requires binding 105 to be
  `visibility(vertex, fragment)` (104 stays vertex-only). Below that lookup
  the fragment is unchanged — the prepass fragment reads no per-run values
  at all (it discards on coverage only, `:483-486`), and the AA
  screen-space derivative logic is untouched (same UV values, new source).
- **Extension uniform keeps the globals.** Binding 100 retains `supersample`
  and `aa_band` (global AA settings mirrored from the `AntiAlias`
  resource by `sync_anti_alias`) and the `oit_depth_offset` policy
  field (always 0.0) — per-batch uniform, unchanged. Only `fill_color`,
  `render_mode`, and the depth nudge leave the uniform for the per-run
  record.

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
   fields; atlas commit stays inside it (records only index the atlas);
   capacity growth is detected here — the replacement mesh/buffers are
   created, written, and swapped onto the batch entity in the same frame
   (D4: a same-frame-created asset is prepared before queue)
3. transform write — new system, PostUpdate
   `.after(TransformSystems::Propagate)`, copies each label's
   `GlobalTransform` into its `RunRecord` slot, gated on
   `Ref::is_changed` (decision 5)
4. Aabb union — new system `.after(transform write)`,
   `.after(VisibilitySystems::CalculateBounds)`,
   `.before(VisibilitySystems::CheckVisibility)` (decision 5)
5. buffer commit — new system (`commit_batch_buffers`) `.after(geometry
   write).after(transform write)`, one `ShaderBuffer` write per dirty
   buffer (instances and/or run table) per batch, still in PostUpdate so
   extraction sees this frame's data; this system also writes the
   `DiegeticPerfStats` batch counters
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
- One edited run dirties **its batch's instance buffer** (whole-buffer
  upload), never other batches, never any mesh asset.
- A scrolled clip rect changes that run's glyph records every scrolled
  frame — scroll frames upload the batch's instance buffer. Known, bounded,
  measured in Step 2.
- Sub-buffer range writes are a later optimization, justified only by a
  measured cost.

## Design decisions

### 1. Pipeline route — vertex pulling inside `ExtendedMaterial` (verified)

`TextExtension` grows two storage bindings: instances at 104,
`#[storage(104, read_only, visibility(vertex))]`, and runs at 105,
`#[storage(105, read_only, visibility(vertex, fragment))]` — the fragment
indexes the run table for `fill_color` / `render_mode` (Target model). No
binding collision: `StandardMaterial` stops far below 100, the extension
uses 100–103 today. `impl MaterialExtension for TextExtension` today
overrides only the two
fragment stages (`material.rs:81-85`); Step 1 adds `vertex_shader()` and
`prepass_vertex_shader()`, both returning a new
`src/text/slug/shaders/slug_text_vertex_pull.wgsl` (one file, one shared
nudge/expand function — the prepass and main stages must not drift). The
file follows `slug_text.wgsl`'s own dual-pipeline pattern (`:13-19`):
`#ifdef PREPASS_PIPELINE` selects the `prepass_io` vs `forward_io`
`VertexOutput` import, the vertex entry point is gated the same way, and
the shared helper does the pull/expand/nudge work for both.
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

**`BatchKey` is a dedicated struct, not a tuple of bevy types** — the bevy
types don't qualify as map keys: `AlphaMode` has a manual `Eq` impl but no
`Hash` (`Mask(f32)`; `alpha.rs:64`), `RenderLayers` derives `Eq` but not
`Hash`, and the cascade
wrappers (`TextAlpha` / `TextLighting` / `TextSidedness`) don't derive
`Hash`. The key stores encodings instead: alpha mode as a local enum whose
`Mask` carries `f32::to_bits`, lighting / sidedness / shadow as their
discriminants, `RenderLayers` behind a local newtype that hashes its blocks
(it is already `Eq`). `BaseMaterialId` is a `u32` newtype minted by the
interner below.

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
`BaseMaterialId`. Concretely: an `InternedMaterialKey` struct (the compared
fields — floats stored as `to_bits` u32s, textures as `AssetId` — so
`derive(Hash, Eq)` is mechanical) in a
`HashMap<InternedMaterialKey, BaseMaterialId>` plus a `Vec` for reverse
lookup, living as a `PathBatchStore` field; entries are never freed
(bounded by distinct text materials per session — a handful). Panels that
never customize share the default id (the
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
path, not duplicated logic. Step 3b verifies Geometry-mode layering against
the `panel_rendering` example before the old path is removed.

Pass reality in bevy 0.19, so the parity rule is precise about where it
fires: blend and OIT materials are **excluded from the depth prepass**
entirely — for today's text (always blend or OIT) prepass *depth* never
exists, so main-vs-prepass divergence cannot occur for it. The overridden
prepass vertex stage still executes for the **shadow pass** (blend casters
render as silhouettes via `MAY_DISCARD`), where the nudge rides along
through the shared function — harmless from a light's view, and it keeps
the one-code-path property. The parity requirement protects any future
mask/opaque text, where prepass depth is real and divergence would reject
fragments under depth-equal testing.

The nudge *value* is computed exactly as today's bias:
`depth_nudge = command_index as f32 × LAYER_DEPTH_BIAS` for the non-OIT
path, `0.0` under OIT. Naming kept straight: `depth_nudge` (per-run,
`RunRecord`, vertex-applied) is a different field from `oit_depth_offset`
(global policy in the binding-100 uniform, always 0.0, read by the OIT
fragment branch) — the two never merge. Punch-out is also unaffected in
the prepass: the prepass fragment discards on coverage only and never
reads `render_mode`; the punch-out inversion lives in the main fragment's
alpha check.

### 4. Buffer ownership and range allocation

The batch store is a **new field in the `GlyphCache` resource**, alongside
the outline cache and `run_storage`. Sketch (final field names are the
implementer's):

```rust
struct PathBatchStore {
    batches:   HashMap<BatchKey, PathBatch>,
    run_index: HashMap<RunStorageKey, BatchKey>,  // which batch a run is in
}
struct PathBatch {
    entity:        Option<Entity>,               // the batch render entity
    glyph_records: Vec<PathInstanceRecord>,
    run_records:   Vec<RunRecord>,
    runs:          Vec<(RunStorageKey, Range<u32>)>, // written ONLY by rebuild()
    instances:     Handle<ShaderBuffer>,
    run_table:     Handle<ShaderBuffer>,
    mesh:          Handle<Mesh>,                 // inert, capacity-sized
    capacity:      u32,
    instances_dirty: bool,                       // glyph records changed
    run_table_dirty: bool,                       // run records changed
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
  range in the CPU vector and marks `instances_dirty` — no rebuild.
- A fully clipped glyph **emits no record** (matching today's silent skip at
  `run_data.rs:134`); the count change triggers the rebuild path. No
  degenerate placeholder records in the first cut — revisit only if rebuild
  frequency measures hot.
- Capacity: the inert mesh and record buffers are allocated with headroom
  (start at the scene's initial glyph count rounded up, grow by doubling).
  A capacity crossing re-creates buffer + mesh and **swaps the entity's
  handles the same frame** (D4, resolved 2026-06-03 — no double-buffer):
  `PrepareAssets` (including the mesh allocator's
  `allocate_and_free_meshes`) is chained before `Queue` in the render
  schedule (`bevy_render/src/lib.rs:317-322`, `allocator.rs:201`), so an
  asset created in PostUpdate frame N is drawable in frame N — no blink,
  no swap protocol, no content latency. The no-blink requirement is
  binding, and the schedule assumption is *tested*, not trusted: Step 1's
  gate frame-steps a forced growth, and the same check re-runs on bevy
  upgrades. The crossing remains a hitch candidate — Step 1 measures one
  doubling so the cost is a known number; Step 2's label-add stress watches
  hitch frequency.
- Membership has a single mutation point: `insert_run` / `move_run` /
  `remove_run` update `run_index` and the batch's run set together, then
  trigger the rebuild — a despawn plus a batch-move in one frame is
  order-independent because every operation leaves both structures
  consistent.
- Batch-entity lifecycle: the geometry write's batch path spawns the batch
  entity (with `DiegeticTextBatch`, its `Mesh3d`, and the per-batch
  material) on the first run insert for a key, and despawns it + removes
  the store entry when the last run leaves (the batch analogue of the R10
  empty-run path). The per-batch `TextMaterial` is built once at batch
  creation: interned base material + the key's cascade values + the shared
  atlas handles; `sync_anti_alias` already iterates
  `Assets<TextMaterial>` generically, so batch materials pick up
  `AntiAlias` changes with no new code (one-frame latency — it runs in
  Update).
- Storage keys are never held by the batch path: `run_index` keys are
  routing entries, not `run_storage` allocations, so a toggle flip cannot
  double-free — the decision-4 debug assertion plus the Step-2
  flip-both-directions gate item verify it.
- Ranges have a single writer: `rebuild()` recomputes `runs` as it rebuilds
  the record vectors — no other code path writes ranges, so they cannot go
  stale relative to the buffers they index.
- Records are always fully stamped: `PathInstanceRecord` / `RunRecord`
  deliberately derive no `Default`, and `RenderMode` discriminants start
  at 1 (`Text = 1`, `PunchOut = 2`), so a forgotten `render_mode` stamp
  (0) would render as neither mode — `rebuild()` stamps every field from
  the run's prepared data.
- Two dirty flags, one per buffer: a transform-only frame uploads only the
  run table (96 B × runs), never the glyph instance buffer; a same-count
  text edit uploads only the instance buffer. The Phase D property and the
  Step-2 upload counter both read sharper for it.
- `RunStorageKey` is minted from `Entity::to_bits()`
  (`glyph_cache.rs:55-59`) — the entity allocator is the minting authority,
  untouched by Step 4's `RunStorage` deletion, and generational entity bits
  rule out reuse collisions when a label despawns and a new one lands in the
  same slot.
- A debug assertion guards against two writers claiming one storage key
  (toggle-era safety, decision 10).

### 5. Transforms and culling

Glyph records stay in layout space; world placement is the per-run `Mat4`.
`update_panel_text_geometry` runs `.before(TransformSystems::Propagate)`
(`panel_text/mod.rs:94-96`), so it cannot read this frame's
`GlobalTransform` — the **transform-write system** (frame-flow step 3) runs
`.after(TransformSystems::Propagate)` in PostUpdate, copies each routed
label's `GlobalTransform` into its `RunRecord` slot, and marks the batch's
`run_table_dirty` only when the matrix actually changed (`Ref::is_changed`
gating).
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

**Sort distance and the Aabb's frame.** `Transparent3d` sorts phase items
by view distance to the entity (rangefinder over the mesh instance's
translation / aabb center). A batch entity left at the origin sorts as if
*at the origin* — wrong against other transparent geometry in the scene. So
the Aabb-union system also writes the batch entity's translation to the
union's world center, and the `Aabb` component it installs is **local
space**: center zero, the union's half-extents (bevy interprets `Aabb` in
the entity's frame). The batch entity has no parent, so writing `Transform`
+ `GlobalTransform` directly after propagation is safe. The vertex shader
ignores the entity transform entirely (placement comes from run records;
the mesh uniform is read only for `first_vertex_index` — Target model), so
this translation is sort/culling metadata, not geometry. Ordering *within*
a batch under non-OIT remains the depth nudge's job (decision 3).

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
Step 2 measurements, don't fear it. Terminology kept straight: `TextAlpha`
carries an `AlphaMode`, not an opacity — its changes are *mode* changes and
take the batch-move path above (`update_panel_text_alpha` today mutates only
`base.alpha_mode`, never a color). Opacity *fades* are color-alpha edits:
they ride in the per-run record's `fill_color.a` and stay in-batch.

### 9. Empty runs and despawn

The R10 empty-text path (`mesh_spawning.rs:163-170`) and the
`On<Remove, DiegeticTextMesh>` storage-free observer translate to
remove-from-batch (the decision-4 rebuild). The observer moves to the label
entity (there is no per-run mesh child to observe anymore). Because ranges
are derived state, observer-vs-system ordering inside one frame cannot
corrupt them — the rebuild sees only the live run set.

### 10. The Step-2 toggle — what it gates

The toggle is a resource —
`enum TextGeometryPath { PerRunMeshes, BatchedRecords }`, deriving
`Resource, Reflect, Clone, Copy, Default, PartialEq` with
`#[reflect(Resource)]` and `#[default] PerRunMeshes` (the per-run path stays
the default through Step 3b; gates flip it explicitly, BRP can flip it
live) — gating **which geometry path runs, whole-system**: `PerRunMeshes` → today's
`update_panel_text_geometry` mesh path; `BatchedRecords` → the batch path
plus the transform-write / Aabb / commit systems. Exactly one path executes
per frame — never per-run toggling, so cascade observers, storage keys, and
despawn observers act on one world model at a time.
`update_panel_text_alpha` is gated to `PerRunMeshes` in Step 2 (it mutates
per-run materials that don't exist under batching); a runtime alpha-mode
change while `BatchedRecords` is active warns/debug-asserts until Step 3a's
`move_run` lands — the Step-2 gate examples don't change alpha modes at
runtime. Under `BatchedRecords`
**all** panel-text runs route through the batch store (world labels are
one-element panels; there is no separate kind to filter on) — what Step 2
defers to Step 3 is not routing but dynamics and verification: re-keying on
a later cascade change, the depth nudge, punch-out verification, and the
examples sweep. Until Step 3a lands, flip the toggle only in the Step-2
gate examples. Flipping at runtime tears down the inactive path's products
(per-run mesh children one way, batch entities the other — the existing
`On<Remove, DiegeticTextMesh>` observer handles the former, verified safe).
The batch direction is a system, not an observer: when the toggle leaves
`BatchedRecords`, the batch-path gating system despawns all
`DiegeticTextBatch` entities and clears `PathBatchStore` (batches +
`run_index`; the interner may persist — it is keying state only).
Batch entities carry a marker component (`DiegeticTextBatch`) so BRP
inspection can tell the two paths apart (Reflect-registered as of the
Step-2 review — a bare `Component` cannot be used as a BRP query filter).

## What must not regress (acceptance checklist)

| Feature | Verified by |
| --- | --- |
| Per-label `TextAlpha` / `TextLighting` / `TextSidedness` (memory: `project_labels_control_own_alpha` — deleted twice as "unused"; they are features) | `cascade` + `text_alpha` examples + Step 3a batch-move test |
| `GlyphShadowMode::None` / `Cast` + ghost text (alpha-0 cast) | `diegetic_text_stress` (casting labels) + shadow screenshot |
| Punch-out render mode | rg sweep of `GlyphRenderMode::PunchOut` call sites — all must read `render_mode` from records — plus a visual check in an example that renders punch-out (Step 3b) |
| Clip rects + scrolling | `bevy_lagrange` `showcase` (event-log panel scrolls via `scroll_y_from_end`, `event_log.rs:159`): glyphs clip at rect while scrolling, no stale/missing glyphs |
| OIT on (`StableTransparency`) and off; PBR lighting (panels are physical — never unlit-by-default) | `side_by_side` + stress test with OIT toggled |
| Geometry-mode coplanar layering (M2) | `panel_rendering` |
| Per-panel `RenderLayers` | `viewports_windows` (bevy_lagrange) + `screen_space` |
| Dynamic text edits route as range writes | `typography` arrow-key word scrubber + stress test + Step-2 upload counters (the split dirty flags prove the routing) |
| Typography debug overlay still draws (reads `PanelTextLayout` via `ComputedWorldText`, not meshes) | `typography` overlay compared before/after Step 4 |
| The Phase D no-op-no-work property, restated for batching: unchanged frame → zero buffer writes; one edited run → only its batch's record buffer uploads, no mesh assets ever | Step 2 instrumentation |

Rows not claimed by a 3a/3b/4 gate (clip+scroll, OIT/PBR, `RenderLayers`,
dynamic edits) are owned by **Step 2's parity gate** — they run with the
toggle flipped in the gate examples.

## Incremental plan (measure as you go; STOP for review at each gate)

- **Step 0 — Plan review.** DONE 2026-06-03 (two-cycle team review; see the
  Review log). Baseline column above is captured.
- **Step 1 — Vertex-pulling proof.** DONE 2026-06-03; gate evidence and
  implementation notes follow the gate description below.
  `slug_text_vertex_pull.wgsl` + the two
  storage bindings and vertex-stage overrides on `TextExtension`; the two
  record structs in `packing.rs`. Gate (operational): screenshots verify
  glyph placement/UVs; shading responds to a moved light; OIT toggled
  per camera both ways; shadow silhouettes match glyph outlines; the depth
  nudge lives in **one shared function called by both vertex entry points**
  (identical by construction) and its prepass-pipeline consumer — the
  shadow pass — renders correctly; nonzero-`first_vertex_index` derivation
  verified by a Metal frame capture of the draw's `base_vertex` (spawn a
  second mesh to encourage a nonzero base) or a debug-color emit of the
  recovered glyph index;
  GPU-layout assertions (40 / 96 via `ShaderSize::SHADER_SIZE`, not
  `size_of` — encase rounds struct size to alignment per WGSL rules, so 96
  is what it returns for RunRecord; verified against encase 0.12 source) +
  a CPU-side encase encode/decode round-trip; one
  capacity-doubling reallocation measured (the hitch number) **and** the
  no-blink requirement verified by frame-stepped screenshots at N / N+1 /
  N+2 around a forced growth (direct same-frame swap — D4; this gate also
  re-runs on bevy upgrades to keep the prepare-before-queue assumption
  tested). No batching logic yet, nothing routed. Verifies the
  real unknowns (Material-framework vertex override +
  mesh-with-inert-vertices + `Mat4`-in-record) before structural work.

  **Implementation note (mechanical deviation, single correct outcome).**
  `vertex_shader()` / `prepass_vertex_shader()` are material-type-wide, so
  overriding them would route *per-run* materials through the pull shader
  too — Step 1 requires coexistence. Instead `TextExtension` carries a
  `vertex_pull` flag in new
  `#[bind_group_data(TextExtensionKey)]` key data, and
  `MaterialExtension::specialize` swaps `descriptor.vertex.shader` to the
  pull shader (loaded behind a `uuid_handle!` via `load_internal_asset!`)
  when the flag is set — one WGSL file still serves main, prepass, and
  shadow pipelines through `#ifdef PREPASS_PIPELINE`, and `specialize` runs
  for all three. Per-run materials bind the atlas `glyphs` buffer as a
  placeholder at 104/105 (their pipelines never read those bindings, but
  the bind group must be preparable). The depth nudge is applied in clip
  space (`clip.z += nudge × 2e-6 × clip.w`, `#ifndef OIT_ENABLED`);
  magnitude is provisional until Step 3b's `panel_rendering` check.

  **Retrospective.**
  *What worked:* every Step-1 unknown held — `ExtendedMaterial` vertex
  override (via `specialize`), inert mesh through the pipeline, `Mat4`
  records, same-frame mesh swap (D4), shadow pass through the shared
  vertex function. Buffer growth (`set_data` to a larger size) propagated
  to the material bind group with no staleness — render-asset dependency
  tracking covers it, no manual invalidation needed.
  *What deviated:* per-material `specialize` swap instead of the planned
  `vertex_shader()` overrides (those are type-wide and would have broken
  per-run coexistence); records in `glyph/packing.rs` (the doc's
  `render/packing.rs` path was stale); record building reuses a new
  `glyph_quad_extents` extraction shared with `push_glyph` rather than
  duplicating the rect/UV math.
  *Surprises:* every `TextMaterial` must bind *something* at 104/105 —
  the bind-group layout is material-type-wide — so per-run materials carry
  placeholder handles for as long as both paths coexist;
  `TextExtensionKey` bind-group data now exists and re-specializes on
  material mutation.
  *Implications for remaining phases:* Step 2's batch-material
  construction must set `vertex_pull: true` + real record buffers (and
  `text_material()`'s placeholder wiring stays for the per-run path until
  Step 4); Step 2's record building should call `glyph_quad_extents`;
  Step 4 additionally deletes the placeholder bindings (with the per-run
  path gone, 104/105 are always real).

  **Step 1 review (architect re-evaluation of remaining phases).**
  - Fragment run-table read moved Step 3b → Step 2 (user-approved; D2
    amended) — Step 2's parity gate needs it for multi-color batches.
  - Step 2 gained the Step-1 carry-over block: the `inert_batch_mesh` +
    record-building loop, batch material sets `vertex_pull: true` with real
    buffers.
  - Decision 4 gained the record-stamping rule (no `Default` on records;
    `RenderMode` starts at 1, a zero `render_mode` renders as neither
    mode).
  - Step 2's parity gate now names the provisional depth-nudge constant:
    coplanar Geometry-mode stacks are excluded until Step 3b verifies the
    magnitude.
  - Step 2's `perf.rs` task greps for `render_world_text` instead of
    trusting the stale `:55-56` line ref.
- **Step 2 — Batch store + routing.** DONE 2026-06-03; gate results,
  implementation notes, and the retrospective follow the gate description
  below. `PathBatchStore` in `GlyphCache`;
  the transform-write, Aabb-union, and buffer-commit systems on the
  frame-flow anchors; all runs routed behind the decision-10 toggle (full
  batch key from day one — cascade *values* are read at insert; only
  re-keying on later changes waits for Step 3); `Aabb` or
  `NoFrustumCulling` scaffolding. The batch entity is composed of `Mesh3d`
  (the `inert_batch_mesh`) + `MeshMaterial3d` + the record-buffer/material
  trio, with the `glyph_quad_extents`-based record-building loop.
  Batch-material construction sets
  `vertex_pull: true` with real `instances` / `run_records` buffers;
  per-run `text_material()` keeps its placeholder 104/105 wiring untouched
  until Step 4. **The fragment run-table read lands here** (moved from Step
  3b, user-approved 2026-06-03): `specialize` pushes a def (e.g.
  `FRAGMENT_DATA_FROM_BATCHED_PATHS`) into the fragment defs when `vertex_pull` is set,
  and `slug_text.wgsl`'s fragment sources `fill_color` / `render_mode`
  from `run_records[u32(floor(in.uv_b.y))]` under that def (uniform path
  otherwise — per-run materials unchanged). Without it a batch renders
  every run with one material's color/mode and the parity gate below can
  only pass for single-color scenes; Step 1 proved per-run delivery of
  transform/nudge but could not prove color/mode (its runs share one
  color). Step 3b keeps the *verification* (punch-out visual check,
  examples sweep). **Proof counters land here too** (they are
  how the gate and the results table get their numbers):
  - *Library:* `DiegeticPerfStats` gains batch stats — batch count, total
    runs, total glyph records, buffer uploads this frame (instance and
    run-table uploads counted separately, matching the two dirty flags) —
    published as diagnostics like `reconcile_ms`, so they read over BRP.
    (While in `perf.rs`: correct the stale doc comment claiming a separate
    `render_world_text` path — grep for `render_world_text` rather than
    trusting a line number; that path was removed when fluent text became
    one-element panels, `world_text/mod.rs:7-9`.)
  - *Example:* the stress overlay gains a row showing them
    (`batches 2 · runs 100 · glyphs ~600 · uploads N`), and the example's
    render-app plugin counts per-view phase items (Transparent3d — OIT
    reuses that phase, the resolve pass adds none — and the shadow phases)
    after `RenderSystems::PhaseSort` into the same shared-atomics
    channel the waterfall uses — the draws-per-pass number on screen. The
    counter is path-agnostic (it counts phase items in whichever toggle
    state is active), so before/after comes from one session. Expected per
    counted pass: ~100+ items toggle-off → 1–2 toggle-on (the world batch in
    every pass; the overlay batch only in the screen-space view).

  Gate: **parity** = BRP screenshots at
  identical camera/window across a toggle flip for the stress test and one
  scroll example, compared visually plus a diff-image sanity check
  (ImageMagick `compare`, or the repo's screenshot-diff script if one
  exists; byte-exact is not expected — OIT accumulation order and float
  paths
  differ; document any visible delta and treat it as a finding; the
  depth-nudge constant is provisional until Step 3b's `panel_rendering`
  verification, so parity examples must avoid coplanar Geometry-mode
  stacks — a nudge-ordering delta there is a Step-3b item, not a Step-2
  parity failure); a
  moving-label case shows no one-frame transform lag (constant-velocity
  label, screenshots from both toggle states at the same timestamps,
  overlaid — lag shows as a one-frame positional offset); scrolled-clip
  frames
  upload only the expected instance buffer; label add/remove/re-add under
  both
  toggle states leaks no storage (debug assertion from decision 4); toggle
  flipped both directions (per-run → batched → per-run) with no storage
  leaks and no stale meshes; the screen-space overlay's batch key is stable
  from frame 1 of the overlay's life (`RenderLayers` present at run insert
  — `setup_screen_space_view` is an observer, so the panel has layers
  before its first run lands);
  slab-error log watch — grep the app log for the wgpu slab/allocation
  error spam from the `text_alpha` incident during a ~60 s toggled-on
  stress run (memory: `project_text_alpha_slab_errors`);
  waterfall measured both ways in the same session, deltas recorded in the
  table below. Verified-by-review notes that ride this gate: one
  transparent batch serves OIT and non-OIT views simultaneously (phase
  items are per-view); batch-entity spawn timing matches today's per-run
  child spawn (Commands in the same PostUpdate system) — no new first-frame
  visibility behavior; the idle-floor capture uses a stationary camera
  (screen-space overlay transforms are camera-relative — a moving camera
  legitimately dirties the overlay batch's run table even with the grid
  paused).

  **Step 2 gate results (2026-06-03, all items pass).** Same-session toggle
  flip on `diegetic_text_stress` (release, perf mode, M2 Max; window smaller
  than the doc baseline's — compare within the session). Artifacts in
  `/private/tmp/glyph_step2/`.
  - *Parity:* title bar, camera control panel, and the static grid index
    digits are **byte-identical** across the flip (ImageMagick AE = 0);
    glyph weight / AA / lighting indistinguishable at 3× zoom.
  - *Counters (batched):* batches 3 (world labels + two screen-panel
    groups), runs 185, glyph records ~958. Phase items: `Transparent3d`
    307 → 125 (Δ −182 ≈ 185 runs collapsing to 3 batch items; the
    remaining ~122 are SDF panel-backing meshes, not text), shadow
    370 → 8 (2 casters × 4 cascade views).
  - *Waterfall (same session, full mutation):* ms 18.9 → 15.2, fps
    53 → 66, `assets` 1.94 → 0.12, `prep` 8.94 → 1.41, `graph`
    6.51 → 2.21, `gpu wait` 0.96 → 10.98 — the render thread now blocks
    on swapchain acquire instead of pacing the frame. That is the success
    criterion (render at the GPU-bound floor); fragment cost was never
    this plan's lever, so the GPU's ~14 ms is the new floor.
  - *Idle floor (paused, stationary camera):* ms 14.0 / 72 fps, uploads
    0 / 0 across every sample — the Phase D no-op property holds.
  - *Upload split:* steady mutation = exactly 1 instance-buffer upload per
    frame (the world batch), 0 run-table uploads; screen-panel refreshes
    add their batch's upload only on their change frames.
  - *Toggle both directions:* flipped live with full restore each way; plus
    headless pipeline tests (`render/panel_text/batching.rs::tests`) cover
    routing, hand-written bounds, transform parity, both-direction flips
    with zero storage leaks (the decision-4 debug assertions run in every
    nextest pass), despawn-empties-batch, and in-batch text edits.
  - *Slab watch:* 60 s toggled-on soak — zero slab / allocation /
    validation errors in the app log.
  - *Scroll:* `bevy_lagrange` `showcase` flipped over BRP
    (`TextGeometryPath` is reflect-registered); the event log appends and
    clips correctly under batching; menu, camera panel, and world panels
    all render.

  **Implementation notes (Step-2 discoveries).**
  - **Buffer rebind hazard — the gate's one found-and-fixed bug.** bevy
    re-creates a `ShaderBuffer`'s wgpu buffer when `set_data` changes the
    byte length (`bevy_render/src/storage.rs`, `prepare_asset`: equal
    size/usage/label → `write_buffer` in place, otherwise a new buffer),
    and a material's bind group does not follow the new buffer — whether a
    same-frame material re-prepare sees it is a prepare-order race (a stale
    `RenderAssets` entry binds without retrying). The per-run path masks
    this everywhere by rewriting its material asset on every change; the
    batch path exposed it as screen panels frozen at flip-time content
    while the store and uploads stayed live. Fix: record buffers are
    **padded to capacity on every upload** (zero-size padding quads
    rasterize nothing; padding run slots are never referenced), so the
    byte length never changes between growths and every upload writes the
    existing buffer in place; a capacity growth creates new buffer assets
    and rewrites the material's `instances` / `run_records` handles, which
    re-prepares reliably (a *missing* render asset retries next frame; the
    old buffers keep drawing pre-growth content for at most one frame —
    no blink).
  - The above corrects a Step-1 gate hole: the growth check compared
    post-growth frames N / N+1 / N+2 to *each other* (no blink — still
    true) but not against expected post-growth content; the appended run's
    visibility was masked by the staircase toggle's material mutation. D4
    is amended accordingly: the mesh swap draws same-frame; the record
    buffers' rebind on growth may lag one frame with pre-growth content,
    never a blank.
  - The plan's "prepass fragment reads no per-run values" was stale:
    `render_coverage` applies the punch-out inversion and the prepass
    fragment calls it, so it does read `render_mode`. `render_coverage`
    now takes `render_mode` as a parameter, sourced from the run table
    under `FRAGMENT_DATA_FROM_BATCHED_PATHS` and from the uniform otherwise — shadow
    silhouettes of punch-out runs stay correct per run.
  - Batch entities carry `NoAutoAabb` (bevy 0.19) so `CalculateBounds`
    never installs a zero-extent box from the inert mesh; the union
    system's ordering defense remains as belt and braces.
  - `update_panel_text_batches` is self-healing — it processes runs that
    changed *or* are unrouted — which is the same mechanism a toggle flip
    and a not-yet-packed glyph use to retry.
  - The stress example gained a `B` shortcut for the flip and a separate
    top-right batch-stats panel (path, batches, runs, glyphs, split
    uploads, per-pass phase items via a render-app counter after
    `PhaseSort`).

  ### Step 2 Retrospective

  **What worked:** the store + four systems landed on the planned
  frame-flow anchors unchanged; parity is byte-identical on static
  content; counter predictions held exactly (185 runs → 3 batches; t3d
  Δ −182); the render-thread rows collapsed (`prep` 8.94 → 1.41, `assets`
  → ~0.1, `graph` 6.51 → 2.21) and the frame went GPU-bound at fps
  53 → 66 under full mutation.
  **What deviated from the plan:** record buffers are capacity-padded and
  growth rewrites the material's buffer handles (the rebind hazard — not
  in the plan); the padding constants exist solely for that (live records
  are still always fully stamped); `render_coverage` gained a
  `render_mode` parameter (the prepass punch-out path was not
  per-run-free); the batch counters live on a second screen panel instead
  of rows in the waterfall overlay.
  **Surprises:** the material-vs-buffer prepare-order race, and that the
  per-run path's material rewrites had been masking it everywhere;
  phase-item counts include non-text transparent items (~122 SDF
  backings), so "draws per pass (text)" reads as a delta; `NoAutoAabb`
  exists in bevy 0.19 and is exactly the right tool for the hand-written
  union Aabb.
  **Implications for remaining phases:** Step 3a's `move_run` mechanics
  already exist (`upsert_run` re-keys when a run arrives with a changed
  key) — 3a wires cascade-change detection into the batch path and adds
  the store-level tests; Step 4 must keep the padding/growth mechanism
  when it deletes the per-run path — with per-run material rewrites gone,
  nothing masks the rebind hazard anywhere, so the `BatchGpu` doc comment
  is the contract; Step 4's placeholder-binding removal (104/105 always
  real) is unaffected.

  ### Step 2 Review (architect re-evaluation of remaining phases)

  - Step 3a re-scoped to what actually remains: the store's re-keying
    landed in Step 2; 3a wires `Changed<Resolved<…>>` detection on routed
    runs into `upsert_run`, covers all three cascades, then deletes
    `warn_batched_alpha_change` (which covers only alpha today —
    lighting/sidedness changes are silently stale until 3a). Shadow- and
    render-mode changes already re-route (they ride `PreparedPanelText`).
  - Step 3a's gate gained the same-frame move+despawn consistency test —
    the decision-4/9 order-independence claim becomes exercisable only
    once mid-frame moves exist.
  - Step 3b's punch-out item scoped to visual check + rg sweep (the
    shader plumbing, including prepass punch-out via `render_coverage`'s
    `render_mode` parameter, landed in Step 2); depth-nudge protocol
    pinned — per-run reference of `panel_rendering` captured before any
    flip or magnitude tuning, since Step 2's parity deliberately excluded
    coplanar stacks.
  - Step 4 gained: an explicit dependency on 3a's re-routing (deleting
    `update_panel_text_alpha` removes the only alpha handler otherwise);
    a forced-growth gate comparing against expected post-growth content
    (closing the Step-1 gate hole permanently); the `NoFrustumCulling`
    deliverable corrected (production never used it); the scene-wide counter
    caveat on the final waterfall column.
  - `DiegeticTextBatch` is now `Reflect`-registered (decision-10's BRP
    -inspectability promise was unmet — a bare `Component` cannot filter a
    BRP query); applied as a Step-2 completion fix, tests green.
  - The "draws per pass (text)" acceptance row annotated as a delta
    reading in the 3b and 4 gates (the phase-item counter is scene-wide;
    SDF panel backings ride in `Transparent3d`).
  - D5 (split Step 4 into 4a flip-default / 4b delete) — approved
    2026-06-03 with flip-first ordering; Step 4 above now reads 4a (flip
    default + bake, depends on 3a) and 4b (delete per-run path + toggle).
- **Step 3a — Batch-membership dynamics.** DONE 2026-06-03; gate results
  and implementation notes below. The store's move mechanics
  landed in Step 2 (`upsert_run` re-keys when a run arrives with a changed
  key, unit-tested); what 3a adds is the **trigger**: detect
  `Changed<Resolved<TextAlpha / TextLighting / TextSidedness>>` on
  already-routed runs and feed them back through `upsert_run` — today a
  live cascade change on a routed run with unchanged text never re-routes
  (`update_panel_text_batches` short-circuits on
  `!prepared.is_changed() && is_routed`). Shadow-mode and render-mode
  changes already re-route (both ride in `PreparedPanelText`, so they pass
  the `is_changed` gate) — for those only the tests and screenshots
  remain. Delete `warn_batched_alpha_change` (and its registration) once
  all three cascades re-route; until then note it covers only alpha — a
  live lighting/sidedness change under `BatchedRecords` is silently stale.
  Gate: `PathBatchStore` + pipeline tests via cargo nextest — an
  alpha-mode change moves the run to the new key's batch; a value-only
  `fill_color` change stays in-batch as a record write; a shadow-mode
  change lands the run in the `NotShadowCaster`-keyed batch; a run that
  changes cascade key AND despawns in the same frame leaves both store
  maps consistent (the decision-4/9 order-independence claim, first
  exercisable when mid-frame moves exist) — plus shadow screenshots
  against baseline.

  ### Step 3a gate results (2026-06-03)

  - **Implementation:** `BatchKeyCascades` `SystemParam` in `batching.rs`
    bundles the three `Resolved<…>` queries + defaults + a
    `Changed<Resolved<…>>`-filtered run set (`Or<…>` over all three
    cascades; the bundling also kept `update_panel_text_batches` under
    bevy's 16-param system limit). The run loop's short-circuit now passes
    runs whose cascade value transitioned; `commit_glyph_atlas` fires when
    the changed set is non-empty so a re-key that spawns a new batch has
    atlas handles. The consumed-`Changed`-tick hazard (re-route skipped on
    atlas-miss, tick already spent) cannot trip: the glyph atlas is
    append-only, so an already-routed run's glyphs are always packed.
    `warn_batched_alpha_change` deleted with its registration.
  - **Tests:** 283 pass (5 new). Store: move-then-remove in one pass
    leaves both maps consistent. Pipeline: alpha override re-keys the run
    into a second batch (records conserved); fill-color edit stays
    in-batch on the same entity with the record rewritten; shadow-mode
    edit lands the run in the `NotShadowCaster`-keyed batch entity;
    same-frame override + panel despawn ends at `(0, 0, 0)` with no
    routing leak. Clippy (workspace lints) clean.
  - **Shadow screenshots:** per-run vs batched paused captures
    (`/private/tmp/glyph_3a/`), central grid + floor crop (960×1280@440+80,
    excludes the four overlay panels): **AE=0**, with glyph shadows
    visibly present on the floor in the compared region (not vacuous).
  - **Live BRP exercise:** `Override<TextAlpha>` = Opaque inserted on a
    routed grid label over BRP → batches 3 → 4, the overridden run
    renders opaque through its own batch, log clean. Caveat discovered:
    on the *animating* grid, reconcile re-authors label alpha from
    `TextStyle` each edit (style `alpha_mode: None` → inherit), so a
    BRP-inserted override is stripped on the next text edit — pause
    first (Space) to observe a persistent override on grid labels.

  ### Step 3a implementation notes (bugs found by the live exercise)

  - **Opaque batches crashed pipeline creation** (wgpu validation:
    `'pbr_prepass_pipeline'` — "Shader global ResourceBinding { group: 3,
    binding: 104 } is not available in the pipeline layout"). bevy's
    depth-only pipelines (camera depth prepass via
    `is_depth_only_opaque_prepass`, and shadow views for non-discard
    casters via `light.rs` `is_depth_only_opaque`) replace the material
    bind group with an empty layout — and the vertex-pull vertex stage
    reads bindings 104/105 from that group. Step 2 never hit this: every
    batch key so far was Blend (`MAY_DISCARD` set → material layout
    kept), and headless tests have no render world. Two fixes:
    1. `TextExtension::enable_prepass() -> false` — text contributes
       nothing to the camera depth prepass (main opaque pass writes its
       own depth, `GreaterEqual` + write; prepass was early-z only), and
       per-run materials never read material data in the prepass either.
    2. `batch_material` maps resolved `Opaque` → `Mask(0.0)` on the GPU
       material only (the `BatchKey` keeps the user's `Opaque` identity).
       Cutoff 0 never discards by alpha; the coverage discards cut the
       glyph outlines; depth writes, nothing blends — same main-pass
       pixels. `MAY_DISCARD` pipelines keep the material bind group, so
       shadow vertex pulling works.
  - **Parity note for the 3b/4a gates:** batched Opaque text casts
    glyph-silhouette shadows (the masked shadow pipeline runs the
    coverage fragment); per-run Opaque text casts full-quad rectangle
    shadows (depth-only shadow pipelines run no fragment at all). The
    batched output is the correct one — treat this as an accepted
    per-run/batched difference when comparing Opaque-text scenes (e.g.
    `text_alpha` mode 7), not a regression.

  ### Step 3a Retrospective

  **What worked:**
  - The trigger really was pure wiring: `BatchKeyCascades` (one
    `SystemParam`: three `Resolved<…>` queries + defaults + one
    `Or<Changed<…>>` run set); the short-circuit and `any_work` each
    gained one membership check. Step 2's store move mechanics needed
    zero changes.
  - The live BRP exercise (insert `Override<TextAlpha>` on a routed run)
    caught a render-world crash that headless tests structurally cannot
    see — worth repeating for every remaining gate.

  **What deviated from the plan:**
  - Two unplanned code fixes shipped with the phase:
    `TextExtension::enable_prepass() -> false` and `batch_material`
    mapping resolved `Opaque` → `Mask(0.0)` (GPU material only; the
    `BatchKey` keeps `Opaque`) — see implementation notes above.
  - `update_panel_text_batches` hit bevy's 16-system-param limit; the
    cascade inputs were going to be grouped anyway, the limit just made
    it mandatory.

  **Surprises:**
  - Opaque was a never-exercised batch key: every batch until 3a was
    Blend, and the per-run path masks the engine constraint (its standard
    vertex stage reads no material data, so depth-only pipelines with an
    empty material layout build fine).
  - Per-run Opaque text has been casting full-quad rectangle shadows all
    along (depth-only shadow pipelines run no fragment, so coverage never
    cuts the quad) — the batched path is the first to render Opaque text
    shadows correctly.
  - On the animating grid, reconcile re-authors label alpha from
    `TextStyle` on every text edit, so an externally-inserted override is
    stripped one frame later — live cascade experiments need the pause
    key first.

  **Implications for remaining phases:**
  - 3b/4a screenshot gates: Opaque-text scenes (`text_alpha` mode 7)
    legitimately differ per-run vs batched in shadows — annotated above
    as accepted, not a regression.
  - 4a's bake should drive `text_alpha` through every alpha mode live —
    its mode switcher is exactly the cascade re-key path 3a built.
  - Text no longer appears in the camera depth prepass at all
    (`enable_prepass = false`, both paths). Any future prepass consumer
    (SSAO, TAA motion vectors) will not see text until that decision is
    revisited.

  ### Step 3a Review (architect re-evaluation of remaining phases)

  - Step 3b gained a fix-first work item (user-approved 2026-06-03): the
    depth-only empty-material-layout guard in `TextExtension::specialize`
    — `Multiply` batches otherwise reproduce the Opaque shadow-pipeline
    crash (bevy's shadow path grants `MAY_DISCARD` to every non-opaque
    alpha mode except `Multiply`); batched Multiply casts no shadow
    (accepted parity difference — per-run casts full-quad rectangles),
    and the alpha remap consolidates into one named function.
  - Step 3b's depth-nudge protocol verifies at two camera distances (the
    constant is a fixed clip-space epsilon independent of view depth);
    the `text_alpha` sweep records the alpha-mode × MSAA validation
    matrix (`AlphaToCoverage` is MSAA-dependent).
  - Step 4a's gate: explicit path-assumption audit of every pipeline test
    (a default flip changes which path a no-explicit-set test exercises
    without failing it); `text_alpha`'s on-screen mode descriptions
    updated for batched behavior; prepass absence attributed to
    3a-on-both-paths, not to the flip.
  - Step 4b's gate gained a headless constant-byte-length test for the
    padding contract (previously guarded only by the `BatchGpu` doc
    comment and parity screenshots).
  - Confirmed, no change needed: punch-out is verification-only (shader
    plumbing fully landed); 4a's dependency on 3a's re-routing is
    satisfied in code; the remaining phases are otherwise correctly
    scoped.
- **Step 3b — Per-record fields and shaders.** DONE 2026-06-04; gate
  results and implementation notes follow the step description.
  Fix-first (3a review,
  user-approved 2026-06-03): guard `TextExtension::specialize` — when the
  descriptor's material bind-group slot is the empty layout (a depth-only
  pipeline), skip the vertex-pull shader swap, so alpha modes bevy routes
  depth-only (currently `Multiply`; Opaque is remapped to `Mask(0.0)`)
  build a valid pipeline and cast no shadow instead of failing wgpu
  validation. Annotate as an accepted parity difference: per-run Multiply
  casts full-quad rectangle shadows; batches have no real quad geometry,
  so no-shadow is the only achievable batched output. The guard is also
  the catch-all for any future mode or engine change that strips the
  material group. Rider: consolidate the alpha remapping into one named
  function with the invariant documented (`BatchKey.alpha` keeps the
  user's authored mode; the GPU material may differ). Then:
  punch-out verification; clip
  move; depth-nudge layering verified against `panel_rendering`; the
  acceptance-table examples sweep. (The fragment run-table read itself
  moved to Step 2 — user-approved 2026-06-03 — so this step verifies
  per-record `render_mode` / color behavior rather than implementing it.
  The shader plumbing is fully landed, including punch-out delivery
  through the prepass — `render_coverage` takes `render_mode` as a
  parameter — so the punch-out item reduces to the visual check in a
  punch-out example plus the rg sweep of `GlyphRenderMode::PunchOut` call
  sites.) Depth-nudge protocol: capture the per-run-path reference
  screenshot of `panel_rendering` **before** flipping the toggle or
  tuning the magnitude — Step 2's parity examples deliberately excluded
  coplanar Geometry-mode stacks, so no baseline exists yet; if the
  provisional `2e-6` clip-space constant changes, the shared
  `slug_text_vertex_pull.wgsl` constant is the single site (both entry
  points read it). The nudge is a fixed clip-space epsilon independent of
  view depth, so verify layering at two camera distances (near + far),
  not just `panel_rendering`'s default framing — or record the constant
  as calibrated for that depth range only (3a review). The `text_alpha`
  sweep records which of the 7 alpha modes were validated under batching
  and at what MSAA setting — `AlphaToCoverage` is MSAA-dependent
  (`Msaa::Off` routes it to `MAY_DISCARD`, MSAA-on to
  `BLEND_ALPHA_TO_COVERAGE`; the example runs `Sample4`) (3a review).
  Gate: acceptance checklist above, full test suite,
  clippy (nursery + pedantic). Counter caveat for the sweep: the
  phase-item counter is scene-wide — SDF panel backings ride in
  `Transparent3d` — so text draw counts read as deltas, not absolutes.

  **Step 3b gate results (2026-06-04, all items pass).** Artifacts in
  `/private/tmp/glyph_3b/`.
  - *Multiply guard:* `TextExtension::specialize` skips the vertex-pull
    swap when `descriptor.layout[MATERIAL_BIND_GROUP_INDEX]` has no
    entries (`material_group_is_stripped`) — bevy's empty-layout
    substitution for depth-only pipelines is detectable directly because
    rc.2 descriptors carry `BindGroupLayoutDescriptor` values. Live:
    Multiply (mode 5) on `text_alpha` under `BatchedRecords` builds its
    pipelines, renders correctly, casts no shadow, zero log errors — the
    pre-fix wgpu validation crash is gone. Rider landed:
    `batch_gpu_alpha_mode` is the one named remap site (invariant in its
    doc comment: `BatchKey.alpha` keeps the authored mode).
  - *Alpha-mode × MSAA matrix (`text_alpha`, `Msaa::Sample4`, batched):*
    all 7 modes validated live — Blend, Premultiplied, AlphaToCoverage
    (MSAA-on routing), Add, Multiply (no shadow — accepted parity
    difference), Mask, Opaque (renders glyph silhouettes via the
    `Mask(0.0)` material remap; glyph-silhouette shadows). `Msaa::Off`
    `AlphaToCoverage` (the `MAY_DISCARD` routing) not exercised — the
    example pins `Sample4`.
  - *Punch-out:* rg sweep clean — the batched path reads `render_mode`
    per record from `prepared.render_mode` (`batching.rs` record build);
    the material uniform is a placeholder under `FRAGMENT_DATA_FROM_BATCHED_PATHS`.
    Visual: `slug_text`'s PunchOut row byte-identical across the flip
    (ImageMagick AE = 0 on the row crop), inverted coverage rendering.
  - *Depth-nudge layering (`panel_rendering`):* per-run reference
    captured first at an identical pose per pair; compared at default
    framing plus near (`OrbitCam.radius` 0.08 via BRP mutation) and far
    (0.6). Layering and clip identical across the flip at all three
    distances — diffs are glyph-edge AA + TAA jitter speckle only, no
    inversions. The provisional `2e-6` clip-space constant stands
    unchanged. (Scroll/pinch over BRP did not reach the orbit camera;
    `.target_radius` mutation is the working pose control.)
  - *Tests / lints:* 400 nextest pass (new pin:
    `respawned_pinned_panel_keys_by_its_override_not_the_default` —
    same-frame default change + pinned-panel respawn keys by the pin),
    clippy (workspace nursery + pedantic) clean, fmt run.

  **Step 3b implementation notes (found-and-fixed).**
  - **Batch sort-bias bug — the gate's found-and-fixed bug.** Per-run
    text materials sort after their panel's SDF backing layers in
    `Transparent3d` via `command_depth × LAYER_DEPTH_BIAS`
    (`sort_distance = rangefinder.distance(mesh_center) + depth_bias`);
    `batch_material` zeroed `depth_bias`, so on sorted (non-OIT) views —
    the screen-space overlay camera has no OIT — a panel's translucent
    backing could composite over the whole batch: text uniformly dimmed
    behind smoked glass, history-dependent (the batch's union
    Aabb/translation shifts as runs re-route, flipping the equal-key
    sort). Symptom chain on `text_alpha`: pinned-Blend HUD panels went
    dim after a rebuild + mode change; resolved cascade values were
    confirmed correct over BRP (the dim was *not* a cascade or re-key
    failure — the headless respawn test passes). Fix:
    `BATCH_TEXT_DEPTH_BIAS` (64 × `LAYER_DEPTH_BIAS`) on every batch
    material — one bias for the whole batch, above every backing bias;
    within-batch order stays with the per-record depth nudge. Per-batch
    max-command-depth was rejected: it would rewrite the material asset
    on record changes, re-introducing the per-change material churn
    Step 2 eliminated.
  - **All-white renderer wedge — gone with the same fix.** Pre-fix,
    flipping `BatchedRecords → PerRunMeshes` while Multiply was the
    cascade default produced a permanent all-white frame (no log
    errors, BRP alive, unrecoverable by mode change or flip-back).
    Post-fix the exact sequence renders correctly and repeated flip
    bounces stay clean. Root cause not separately isolated; recorded in
    the attempts log (`project_3b_text_alpha_findings`) in case it
    resurfaces.
  - The example-driven dimming exposed that `text_alpha`'s controls bar
    and fairy_dust's camera panel intentionally follow the global
    default (only the two `.text_alpha_mode(Blend)` panels are pinned) —
    so under Multiply they legitimately render dark. Not a bug; noted so
    the sweep's expected imagery is documented.

  **Step 3b Retrospective.**
  *What worked:* the empty-layout detection is direct — rc.2
  `RenderPipelineDescriptor.layout` holds `BindGroupLayoutDescriptor`
  values with inspectable `entries`, so the guard is three lines and
  needs no engine patch; the alpha-mode sweep doubled as the bug-finder
  (the Multiply frames exposed the sort-bias bug); BRP cascade queries
  (`Resolved`/`Override` on `TextContent`) cleanly separated cascade
  state from render state during diagnosis; `.target_radius` mutation
  gave exact repeatable camera poses for the two-distance check.
  *What deviated:* the plan's depth-nudge item anticipated retuning the
  `2e-6` constant; instead the layering risk materialized one level up —
  the `Transparent3d` SORT (material `depth_bias`), which the nudge
  cannot influence. The fix is a batch-material constant
  (`BATCH_TEXT_DEPTH_BIAS`), not a shader change.
  *Surprises:* batched text had no sort relation to panel backings at
  all on non-OIT views — Step 2's parity gate never caught it because
  its examples ran OIT-on main cameras and the screen-space overlay's
  sort happened to win; the white flip-wedge appeared and disappeared
  with the sort bias without separate isolation; the old
  `force_update: Pending` OrbitCam BRP payload is stale (now
  `update_request: None/ForceUpdate`, and mutation beats full insert).
  *Implications for remaining phases:* 4a's bake runs more scenes on
  sorted views — watch for text-vs-backing composition regressions
  (anything dim-behind-glass is the sort, not the cascade); 4b deletes
  the per-run path that motivated `BATCH_TEXT_DEPTH_BIAS`'s
  parity framing, so its doc comment should survive as the only record
  of why 64; the `Msaa::Off` `AlphaToCoverage` routing
  (`MAY_DISCARD`) remains unexercised by any example.

  **Step 3b Review (architect re-evaluation of remaining phases).**
  - Step 4a's bake is now an explicit sorted-view composition checklist:
    only 2 of 21 examples run OIT main cameras and the screen-space
    overlay camera is always sorted, so the sorted-camera examples are
    enumerated in the gate, including a two-screen-panels-at-different-
    depths case (the batch's one bias rides its union-center distance).
  - Step 4a gained two while-the-toggle-exists items: flip-bounce stress
    on `text_alpha` under each alpha default (the unisolated white-wedge's
    repro context), and one `Msaa::Off` run so `AlphaToCoverage`'s
    `MAY_DISCARD` routing compiles the vertex-pull pipelines before 4b
    deletes the per-run A/B.
  - Step 4a's test-path audit narrowed to the non-`batching.rs` test
    modules (those never set `TextGeometryPath` and silently switch at
    the flip); `batching.rs` already pins the path in every test.
  - Step 4a's `text_alpha` copy update extended beyond Opaque: the Blend
    copy describes per-run meshes + depth_bias ordering, and the Multiply
    copy should explain the intentionally dark unpinned panels.
  - Step 4a code rider: state the <64-backing-layers-per-panel assumption
    on `BATCH_TEXT_DEPTH_BIAS`'s doc comment.
  - Step 4b's doc update records `enable_prepass() -> false` as a
    standing contract of the surviving path (re-enabling the prepass
    re-enters the `material_group_is_stripped`-guarded territory).
  - Risks updated: the non-OIT sort risk records its 3b outcome (the
    batch-vs-backing case fell through and is fixed; coarse-layer-key
    escalation still available), and translucent world geometry near text
    is recorded as an accepted limitation (per-run had the same window;
    no example exercises it; OIT views immune).
  - Confirmed, no change needed: 4a → 4b ordering stands; neither phase
    is redundant; the 4b padding-contract test remains necessary; the
    pinned-respawn test already locks the case the 4a flip makes default.
- **Step 4a — Flip the default to `BatchedRecords` and bake.** DONE
  2026-06-04; gate results and retrospective follow the step
  description. (Split per
  D5, approved 2026-06-03.) Flip `TextGeometryPath::default()` to
  `BatchedRecords`; the per-run path and the toggle stay as the one-key
  fallback through the bake window. **Depends on Step 3a's re-routing
  being complete:** under a batched default `update_panel_text_alpha`
  never runs, so cascade changes must already re-route through
  `upsert_run`. Gate: full test suite green batched-by-default — and an
  explicit audit of every pipeline test's path assumption, not just
  failures: the default flip silently changes which path a test exercises
  when it never sets `TextGeometryPath`, so each test should state its
  path (3a review). `batching.rs`'s tests already set the path
  explicitly on every test; the audit targets the other test modules
  that build a panel-text app and never set it (`mesh_spawning.rs`,
  `alpha.rs`, `diegetic_panel.rs`, `screen_space/mod.rs`,
  `batch_store.rs`, and the like) — those silently switch paths at the
  flip (3b review). The acceptance-table examples sweep run
  batched-by-default, waterfall captured under the new default. The
  sweep doubles as the **sorted-view composition check** (3b review):
  seven examples request OIT on the main camera via `StableTransparency`
  (`typography`, `slug_text`, `world_text`, `diegetic_text_stress`,
  `diegetic_panel_stress`, `panel_rendering`,
  `aa_text`) — the original "only `aa_text` and `panel_rendering`" claim
  was wrong — and OIT activation is now gated on the `oit_guard` shader
  patches (`render/oit_guard.rs`), falling back to sorted transparency
  when a guard fails. The screen-space overlay camera is always sorted
  (it never gets OIT —
  `render/transparency.rs`), so the bake explicitly checks
  text-over-backing composition in the sorted-camera examples
  (`cascade`, `side_by_side`, `world_text`, `slug_text`, `screen_space`,
  `typography`, `diegetic_panel_stress`) — anything dim-behind-glass is
  the `Transparent3d` sort (`BATCH_TEXT_DEPTH_BIAS`), not the cascade —
  including one case with two screen panels at clearly different depths
  (the batch's one bias rides its union-center distance, so per-panel
  depth separation is the untested axis). Also in the bake, while the
  toggle still exists (3b review): a flip-bounce stress on `text_alpha`
  under each alpha default — the original white-wedge repro context —
  and one `text_alpha` run forced to `Msaa::Off` so `AlphaToCoverage`'s
  `MAY_DISCARD` routing compiles the vertex-pull pipelines once before
  4b removes the per-run A/B. Code rider (3b review): state the
  "backing layers per panel stay below 64" assumption on
  `BATCH_TEXT_DEPTH_BIAS`'s doc comment.
  `text_alpha`'s on-screen mode descriptions updated for batched
  behavior — Opaque renders glyph silhouettes under batching, not the
  "colored rectangle, no silhouette" the per-run copy describes (3a
  review); the Blend copy's "one mesh per text run ordered with
  depth_bias" is per-run prose too, and the Multiply copy should say the
  unpinned HUD/camera panels legitimately go dark (3b review). Note:
  text's absence from the camera depth prepass is a 3a
  property of BOTH paths (`TextExtension::enable_prepass` is type-wide),
  not something this flip introduces. Bake = daily use across examples
  until confidence is established; any surprise still has the `B`-key
  fallback.

  ### Step 4a gate results (2026-06-04)

  - Full suite green batched-by-default: 401/401 + clippy + fmt. Exactly
    one test broke at the flip — `toggle_flips_both_directions_without_leaks`
    relied on the old default for its starting state (the audit's
    predicted failure mode); fixed with an explicit `set_path`.
  - Test-path audit: `batching.rs` sets the path explicitly in all its
    tests; `mesh_spawning.rs`'s `pipeline_app` hand-wires the per-run
    systems without the production `run_if` gate (doc comment states it
    deliberately tests `PerRunMeshes`); no other test module wires gated
    systems, so no test silently switched paths.
  - Sorted-view composition sweep, all seven: `cascade`,
    `side_by_side` (A/B identical), `world_text`, `slug_text` (punch-out
    intact), `screen_space` (including two screen panels at clearly
    different depths — composition correct), `typography` (all 7 font
    rows render; doubles as the atlas-rebind fix verification),
    `diegetic_panel_stress` (checked under BOTH sorted and OIT — the
    gate-deadlock run landed a sorted-composition data point for free).
  - Flip-bounce stress: `text_alpha`, three per-run↔batched bounces over
    BRP under each of the 7 alpha defaults — no white wedge, no stale
    meshes, no wgpu slab errors.
  - `Msaa::Off` + Coverage: `MAY_DISCARD` vertex-pull pipelines compiled
    and rendered (Coverage visibly degrades to its documented Mask(0.5)
    fallback); log clean.
  - Waterfall recorded in the results table (After 4a): ms 29.6 → 7.6,
    fps 34 → 135 same-session per-run vs batched (~3.9× frame time).
  - Unplanned work the bake surfaced (all landed in-phase):
    1. *bevy 0.19 OIT GPU faults* — bevy compiles shaders with
       `ShaderRuntimeChecks::unchecked()` and its OIT shaders index the
       heads/nodes buffers unguarded; a macOS live-resize lets the
       drawable outgrow the CPU-side buffer-size snapshot, and the OOB
       writes kernel-panicked the machine twice. Mitigation:
       `render/oit_guard.rs` patches `oit_draw.wgsl`/`oit_resolve.wgsl`
       in process with `arrayLength` guards, our vertex-pull shader
       gained a `run_index` clamp, and `StableTransparency` activation
       is gated on both patches being confirmed (guard failure = sorted
       fallback, never unguarded OIT).
    2. *Gate deadlock* — bevy only loads `oit_resolve.wgsl` when an OIT
       view's resolve pipeline specializes, which the gate prevented;
       every gated run silently fell back to sorted transparency until
       the guard now requests the embedded shader at startup
       (`RESOLVE_SHADER_PATH`, handle held in `OitGuardState`).
    3. *Atlas-growth rebind regression* — `commit_glyph_atlas` grew the
       shared atlas via `set_data` with a longer payload (the documented
       `ShaderBuffer` rebind hazard), so long-lived batch materials kept
       reading the dead buffer and late-loaded fonts rendered invisible;
       the per-run path had masked this by recreating materials on every
       change. Fixed: growth swaps in three new buffer assets and
       repoints every `TextMaterial` (`set_text_material_atlas`,
       placeholder-aware); pinned by
       `commit_glyph_atlas_growth_swaps_buffers_and_keeps_handles_fresh`.
  - User-side manual window-resize test with OIT genuinely active:
    PASSED 2026-06-04 (typography; the earlier resize verification
    predated the deadlock fix and only exercised the sorted fallback).

  ### Step 4a Retrospective

  **What worked:**
  - The flip itself was one line plus one predicted test fix; the
    explicit path-assumption audit named the exact failure before it
    happened.
  - The bake did its job twice over: batched-by-default put long-lived
    materials and transparent coverage in every example, exposing two
    latent defects (atlas rebind, bevy's unguarded OIT) that the
    per-run path's constant material recreation had masked for weeks.

  **What deviated from the plan:**
  - A machine-crashing GPU fault investigation ran inside the bake; the
    three-layer OIT guard infrastructure (`render/oit_guard.rs`,
    vertex-pull clamp, gated activation) was not planned anywhere.
  - The sweep's premise was wrong: eight examples carry
    `StableTransparency`, not the two the plan claimed (corrected in
    the step body above).

  **Surprises:**
  - bevy compiles ALL shader modules unchecked
    (`create_shader_module_trusted` in `render_device.rs`) — bounds
    safety in any shader we or bevy ship is purely textual. Upstream
    bevy main is still unguarded (issue filing deferred per user).
  - bevy's two OIT shaders load by different mechanisms
    (`load_shader_library!` eager vs `load_embedded_asset!` lazy inside
    pipeline specialization) — anything gating on the lazy one must
    request it explicitly.
  - `commit_glyph_atlas` carried the known rebind hazard from day one;
    only the flip's long-lived materials could reveal it.
  - The waterfall gap at this window size (~3.9×) is much larger than
    the After-2 capture suggested; idle floor is within 0.2 ms of the
    moving-labels cost — per-frame text cost is now GPU/render-bound,
    not update-bound.

  **Implications for remaining phases:**
  - Step 4b is unblocked (all 4a gates green), pending the user-side
    OIT-active resize re-test.
  - Step 4b must NOT delete the atlas growth-swap + repoint — it is
    shared-atlas plumbing, not per-run plumbing. After 4b makes
    bindings 104/105 always-real, `set_text_material_atlas`'s
    placeholder branch (`instances == glyphs`) becomes dead and should
    be simplified *in 4b*, not before.
  - `render/oit_guard.rs` is standing infrastructure independent of
    this plan; it outlives 4b and is removed only when upstream bevy
    guards its OIT accesses.
- **Step 4b — Delete the per-run mesh path.** DONE 2026-06-06 — all
  gates green (code gates + three live visual gates; see "Step 4b
  status"). Entry precondition (4a
  review): the user-side window-resize test with OIT genuinely active
  has passed — SATISFIED 2026-06-04 (typography, manual resize, guarded
  OIT). Delete: `RunMeshBuilder`,
  `RunRenderData`, per-run materials, the per-run mesh child spawn/despawn,
  the toggle. `run_data.rs` is a **partial** delete (4a review):
  `RunMeshBuilder`, `RunRenderData`, `RunRenderError`,
  `build_run_render_data`, `commit_run_storage` go, but
  `glyph_quad_extents` + `GlyphQuadExtents` stay — the batch path's
  `build_glyph_records` calls them — along with their re-export chain
  (`render/mod.rs`, `slug/mod.rs`, `text/mod.rs`). The OIT guard wiring
  (`request_oit_resolve_shader` / `guard_oit_shaders` /
  `activate_stable_transparency` in `RenderPlugin::build`) is not part
  of the toggle and is untouched by this phase (4a review). With
  bindings 104/105 always-real after the per-run materials go,
  `set_text_material_atlas`'s placeholder branch
  (`instances == glyphs`) becomes dead — simplify it in this phase; the
  atlas growth-swap + repoint in `commit_glyph_atlas` is shared-atlas
  plumbing and stays (4a review). (The production batch path never used
  `NoFrustumCulling` —
  batch entities carry a real hand-written `Aabb` + `NoAutoAabb` from
  Step 2.) `RunStorage` (the struct) is deleted;
  `RunStorageKey` stays — it is the run identifier in
  `PathBatchStore.run_index`. Deleting the per-run path also deletes the
  per-change material rewrites that masked the buffer rebind hazard — the
  capacity-padding / growth-handle-rewrite mechanism (Step 2, `BatchGpu`
  doc contract) must survive untouched, and the contract gains a headless
  test: padded commit payloads keep a constant byte length between
  growths (`padded_glyph_records` / `padded_run_records` length equals
  capacity regardless of record count), so a future refactor that drops
  the padding fails a test rather than a parity screenshot (3a review).
  Gate: tests green,
  typography overlay compared before/after (it reads `PanelTextLayout` via
  `ComputedWorldText`, not meshes), final waterfall column recorded
  (phase-item counts read as deltas — the counter is scene-wide), a
  forced-growth frame-step compared against **expected post-growth
  content** (not just frame-to-frame identity — closing the Step-1 gate
  hole now that growth has no per-run fallback), doc
  updated, `emoji.md` annotated that color-glyph layers land as records
  with a brush field, not layer-quads in run meshes. Doc-update rider
  (3b review): record that `TextExtension::enable_prepass() -> false`
  becomes a standing contract of the only path — re-enabling the camera
  depth prepass re-enters the depth-only empty-layout territory that
  `material_group_is_stripped` guards, so any future prepass change must
  keep (or consciously extend) that guard. The doc update must not
  re-assert the Target model's "prepass fragment reads no per-run
  values" phrasing — Step 2 overturned it (`render_coverage` takes
  `render_mode`) (4a review).

  ### Step 4b status (2026-06-06)

  **Done (code gates green):**
  - Deleted: `mesh_spawning.rs` wholesale (`DiegeticTextMesh`,
    `free_run_storage_on_mesh_removal`, `update_panel_text_geometry`,
    `update_panel_text_alpha`, per-run material builders + tests);
    `run_data.rs` partial per the 4a review (`RunMeshBuilder`,
    `RunRenderData`, `RunRenderError`, `build_run_render_data_with_clip`
    gone; `glyph_quad_extents` + `GlyphQuadExtents` survive with their
    clip tests re-pointed at `glyph_quad_extents` directly);
    `GlyphCache`'s `run_storage` map, `RunStorage`,
    `build_run_render_data`, `commit_run_storage`, `remove_run_storage`,
    and the `upsert_batch_run` wrapper (callers use
    `batch_store_mut().upsert_run` — both toggle-era cross-path
    debug_asserts went with their fields); `TextGeometryPath` +
    `apply_text_geometry_path` + every `run_if` gate (batch systems run
    unconditionally); `PathBatchStore::drain_all`; `text_material` +
    `TextMaterialInput` + `text_material_fill_color` and the re-export
    chains; dead `FontKey::value` / `GlyphKey` accessors;
    `diegetic_text_stress`'s `B` toggle and stats `path` row.
  - Kept, untouched as required: atlas growth-swap + repoint in
    `commit_glyph_atlas`; OIT guard wiring in `RenderPlugin::build`;
    `RunStorageKey` (now plain run identifier — `Component` derive
    dropped, nothing inserts it).
  - `set_text_material_atlas` placeholder branch
    (`instances == glyphs`) removed — bindings 104/105 are always real.
  - New headless test
    `commit_payloads_keep_a_constant_length_between_growths` pins the
    `BatchGpu` padding contract (payload length == capacity for 0..=8
    records).
  - Tests 403/403 workspace, clippy clean (workspace, all targets), fmt.
  - Doc riders: `emoji.md` annotated (color-glyph layers land as N glyph
    records with a brush field, not layer-quads in run meshes);
    `enable_prepass() -> false` recorded as a standing contract at the
    definition site (any future prepass change must keep or consciously
    extend the `material_group_is_stripped` guard); `perf.rs` /
    `alpha.rs` / `glyph_cascade.rs` docs re-pointed at
    `update_panel_text_batches`.
  - State parity under genuine guarded OIT (BRP, typography): 2 batches /
    53 runs / 429 glyph records, identical before and after the
    deletion; all 7 fonts loaded (7 atlas-growth panel rebuilds).

  **Live gates (passed 2026-06-06 after the locked-session block
  cleared):**
  - Typography overlay before/after visual compare: PASSED — the
    after-shot is byte-identical (`cmp` clean) to the pre-deletion
    before-shot (`/private/tmp/4b_typography_after.png` vs
    `4b_probe_relaunch.png`, 2,863,525 bytes), captured under genuine
    guarded OIT.
  - Forced-growth vs expected post-growth content: PASSED — all 7 fonts
    render in their own faces after the 7 atlas growths (visual half;
    the BRP state half passed above). Covered by the same byte-identical
    after-shot.
  - Final waterfall column: recorded in the results table (After 4b).

  **Findings logged during the bake (pre-existing, not 4b):** one
  launch came up with the world camera solid black under OIT (panels
  intact); the state persisted 4+ minutes and cleared instantly when
  `OrderIndependentTransparencySettings` was BRP-removed, rendering
  correctly on re-insert. The launch's log showed a window-restore
  settle-timeout mismatch, but that is correlation only — three forced
  reproductions on 2026-06-06 (stale-width windows.ron, the observed
  off-screen/scale-1.0 stale tuple, live post-activation
  `scale_factor_override` flips both directions) all rendered fine.
  Black state is real and OIT-resident; trigger unknown — see the
  attempts-log memory.

  ### Step 4a Review (architect re-evaluation of remaining phases)

  - Step 4b's deletion list corrected: `run_data.rs` is a partial
    delete — `glyph_quad_extents` + `GlyphQuadExtents` survive (the
    batch path's `build_glyph_records` depends on them); only the
    per-run items (`RunMeshBuilder`, `RunRenderData`, `RunRenderError`,
    `build_run_render_data`, `commit_run_storage`) go.
  - Step 4b gained an entry precondition: the user-side OIT-active
    window-resize test (left open by 4a — every pre-deadlock-fix run
    exercised the sorted fallback).
  - Step 4b gained two riders: the OIT guard wiring is not part of the
    toggle (do not touch it during deletion); simplify
    `set_text_material_atlas`'s now-dead placeholder branch in-phase
    while keeping the atlas growth-swap + repoint.
  - Step 4b's doc-update rider extended: do not re-assert the
    overturned "prepass fragment reads no per-run values" claim.
  - Confirmed, no change needed: the padding/growth survival
    requirement and its headless byte-length test are correctly scoped;
    4a→4b ordering stands; no merge or split needed.

## Results table (fill as steps land)

| Measure | Baseline (2026-06-03) | After 2 (toggle on) | After 4a ⁴ | After 4b ⁵ |
| ------- | --------------------- | ------------------- | ------- | ------- |
| `ms`    | 20.2                  | 15.2 ¹              | 7.6     | 7.3     |
| `fps`   | 51                    | 66 ¹                | 135     | 134     |
| `ms` paused (idle floor) | (capture at Step 2) | 14.0 / 72 fps | 7.4 / 130 fps | 7.5 / 128 fps |
| `wait`  | 17.65                 | 13.37               | 2.04    | 5.14    |
| `render`| 19.7                  | 14.7                | 6.7     | 6.9     |
| `assets`| 2.01 / 11.07          | 0.12 / 0.24         | 0.15 / 0.29 | 0.12 / 0.30 |
| `prep`  | 8.70 / 18.31          | 1.41 / 1.97         | 1.39 / 2.63 | 1.35 / 2.50 |
| `gpu wait` | 2.60 / 14.83       | 10.98 / 26.13 ²     | 2.21 / 11.94 | 3.41 / 9.18 |
| `graph` | 6.36 / 10.30          | 2.21 / 2.32         | 0.91 / 2.56 | 1.97 / 3.30 |
| render entities (text) | ~100 world + overlay runs | 3 batches (185 runs) | 3 batches (185 runs) | 3 batches (183 runs) |
| draws per pass (text)  | ~100 world + overlay runs | t3d items 307 → 125 ³; shadow 370 → 8 | t3d 125; shadow 8 | t3d 125; shadow 8 |
| batches / runs / glyphs | — (no batch store yet)   | 3 / 185 / ~958 | 3 / 185 / 954 | 3 / 183 / 943 |

¹ Captured in a smaller window than the doc baseline; the same-session
per-run reference was ms 18.9 / 53 fps (`assets` 1.94, `prep` 8.94,
`gpu wait` 0.96, `graph` 6.51) — compare After-2 against that column, not
the baseline row.
² `gpu wait` is swapchain-acquire blocking, not work: with the render
thread's CPU rows collapsed, the GPU's ~14 ms (unchanged by this plan)
is exposed as visible waiting instead of hiding inside `prep`/`graph`.
³ The phase-item counter is scene-wide; the surviving ~122 items are SDF
panel-backing meshes, not text. The text delta is −182 ≈ 185 runs → 3
batch items.
⁴ Captured 2026-06-04 at Step 4a (batched default; per-run path still
present behind the toggle), debug build, smaller window than the doc
baseline. Same-session per-run reference: ms 29.6 / 34 fps (`assets`
3.62, `prep` 11.42, `gpu wait` 13.75, `graph` 8.96; t3d 307, shadow
379) — a ~3.9× frame-time gap at this window size. This run had OIT
genuinely active (gated activation, `render/oit_guard.rs`).
⁵ Captured 2026-06-06 after the per-run deletion (batch-only, no
toggle), release build, 3440×2104 window — different build profile and
window than the 4a column, so compare loosely: frame totals match 4a
within noise (`ms` 7.3 vs 7.6, `render` 6.9 vs 6.7), confirming the
deletion changed no steady-state work. Run/glyph counts (183/943 vs
185/954) differ because the mutating scene varies frame to frame.
`wait`/`gpu wait`/`graph` shifts reflect the profile/window change, not
the deletion. Paused now-column: `layout`/`reconcile`/`shaping`/`mesh`
all ≤0.01 ms; uploads i/rt both 0 (no per-frame asset churn at idle).
Screenshots: `/private/tmp/4b_waterfall_moving.png` / `_paused.png`
(paused 5s-max rows include BRP screenshot readback stalls).

The last three rows come from the Step-2 proof counters: `render entities
(text)` = run count (per-run path) vs batch count (batched path), both from
`DiegeticPerfStats`; `draws per pass (text)` = the render-app plugin's
phase-item count; `batches / runs / glyphs` = the `DiegeticPerfStats` batch
stats shown on the overlay. The table deliberately drops the baseline
overlay's main-thread sub-rows (`layout`, `reconcile`, `shaping`, `mesh`,
`other`) — instancing is expected not to move them; it keeps the render
rows it attacks.

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
  can grow a coarse layer field — measured, in Step 3. *Step 3b outcome:*
  the batch-vs-backing case DID fall through (one sort key per batch, bias
  was 0) and is fixed by `BATCH_TEXT_DEPTH_BIAS` on every batch material;
  the coarse-layer-key escalation stays available if a per-panel ordering
  case surfaces in the 4a bake.
- **Translucent world geometry near text (accepted limitation, 3b
  review)** — a Blend world mesh within `BATCH_TEXT_DEPTH_BIAS` (64 world
  units, view-distance terms) of a text batch can mis-sort against it. The
  per-run path had the same wrong-window (its text biases were
  `command_depth × LAYER_DEPTH_BIAS`), no example places translucent world
  geometry near text, and OIT views are immune — so this is recorded, not
  gated. If a real scene hits it, the fix space is the coarse layer key
  above or OIT on that camera.
- **Atlas interaction** — records reference atlas indices, so a mid-frame
  atlas append must commit before batch buffers (same ordering
  `update_panel_text_geometry` enforces today at `mesh_spawning.rs:88-95`;
  the frame-flow anchors preserve it — atlas commit in step 2, batch commit
  in step 5).
- **`text_alpha` slab history** — per-run churn produced slab spam before
  (memory: `project_text_alpha_slab_errors`); the batch store removes the
  per-run assets entirely, but Step 2 watches the log with the toggle on.
- **emoji.md color-glyph path** — the layered-color-glyph design in
  [`emoji.md`](../emoji.md) assumes per-run meshes ("glyph → N quads, one per
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
and refinements incorporated: PathInstanceRecord size corrected 48 → 40 B
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
`packing.rs`; `PathBatchStore` sketch added (decision 4); `vertex_shader()` /
`prepass_vertex_shader()` additions and the shared-WGSL-file requirement
spelled out (decision 1); `RunStorageKey` kept as the batch-store run id,
`RunStorage` struct deleted (Step 4); typography-overlay claim turned into a
before/after comparison gate; emoji.md note assigned to Step 4.

**Review 2, cycle 1.** Verified: all text routes through the panel-text
pipeline (the standalone world-text path was removed when fluent text became
one-element panels — `world_text/mod.rs:7-9`; `perf.rs:55-56` comment is
stale, fix queued in Step 2); bindings 104/105 collision-free; decision-1
fallback wording adequate; phase counter usable in both toggle states.
Determined fixes incorporated: `Indices::U32` + POSITION-only inert mesh +
in-shader normal from run rotation (Target model); `first_vertex_index`
subtraction for slab-allocated meshes — `@builtin(vertex_index)` includes
`base_vertex` (Target model + Step 1 gate); `visibility(vertex)` on the new
storage bindings (decision 1); `BatchKey` dedicated struct — `AlphaMode` not
`Eq`/`Hash`, `RenderLayers` and cascade wrappers not `Hash` (decision 2);
interner concretized (`InternedMaterialKey`, map + vec, never freed)
(decision 2); prepass-parity scope made precise — blend/OIT text skips the
depth prepass; the overridden prepass vertex stage serves the shadow pass;
parity protects future mask/opaque text (decision 3); RunRecord explicit
`_pad` dropped — encase owns padding; layout assertions via `ShaderType`
metadata, not `size_of` (Target model + Step 1); split dirty flags
(instances vs run table) + single-writer rule for ranges + `RunStorageKey`
minting authority = `Entity::to_bits` (decision 4); batch-entity translation
written to the Aabb-union center for `Transparent3d` sort distance, `Aabb`
component is local-space (decision 5); decision-8 alpha terminology
corrected — `TextAlpha` is an `AlphaMode` cascade, mode changes move
batches, opacity fades ride `fill_color.a`; toggle derives + default
`PerRunMeshes` named (decision 10); uniform split pinned — `supersample` /
`aa_band` / `oit_depth_offset` stay in binding 100 (Target model); Step 1
atlas-population recipe (one real panel seeds the atlas); acceptance-table
example names corrected to artifacts that exist (`cascade`, `text_alpha`,
`bevy_lagrange` `showcase` event log for scroll, `viewports_windows`,
`screen_space`); punch-out sweep made operational; results-table rows mapped
to measurement sources; per-pass phase-item expectation restated.

**Review 2, cycle 2.** Adversarial verification against bevy 0.19.0-rc.2
source confirmed: the `first_vertex_index` pattern (`wireframe.wgsl:67-68`,
`mesh_types.wgsl:19`); blend/OIT prepass exclusion (`prepass/mod.rs:
1023-1030`) and shadow-via-prepass-specialize with `MAY_DISCARD`
(`light.rs:2401-2458`); `AlphaMode`/`RenderLayers` derive gaps (with one
correction: `AlphaMode` has a manual `Eq`, `alpha.rs:64`); `Transparent3d`
sort = world-space mesh aabb center, so the batch-translation fix is the
right lever (`core_3d/mod.rs:485-491`, `material.rs:1228-1238`); all
frame-flow schedule anchors real and orderable (`visibility/mod.rs:504-522`;
`TransformSystems::Propagate` naming correct); PostUpdate asset writes
extracted same-frame (proven by the existing atlas commit); auto-inserted
command flushes cover same-frame spawn visibility. **One cycle-1 claim
refuted and corrected**: a POSITION-only inert mesh would discard every
fragment — `VERTEX_UVS_A`/`VERTEX_UVS_B` defs come from the mesh layout
(`mesh.rs:3307-3316`) and `slug_text.wgsl` reads `in.uv`/`in.uv_b` behind
them; the mesh must carry POSITION + UV_0 + UV_1, all zeroed. New
determined fixes: live-count vs capacity draw resolved with an
`arrayLength(&instances)` degenerate-quad guard (capacity-sized index
buffer + live-sized instance buffer; robustness clamping would otherwise
re-blend the last glyph per spare slot); capacity growth documented as a
one-frame visual gap (new mesh not yet prepared at queue time) — measured
at Step 1, mitigation surfaced as D3; batched→per-run toggle teardown named
(gating system despawns `DiegeticTextBatch` entities, clears the store);
membership single-mutation-point rule (insert/move/remove update
`run_index` + run set together); `SHADER_SIZE` named as the assertion API
with write-readback as the stride backstop; split-dirty-flag wording
propagated through frame-flow step 5, buffer-write granularity, decisions
4/5, and the Step-2 gate; "Until Step 3" → "Until Step 3a"; Step-2 gate
gained per-view OIT, first-frame parity, and stationary-camera idle-floor
notes. Verified safe, no doc change: per-view OIT phase routing for one
batch; frame-1 spawn parity with the per-run path.

**Review 3, cycle 1.** Adversarial verification: arrayLength guard
implementable (atlas bindings are already runtime-sized `array<T>`);
double-buffer handle-drop safe (wgpu defers destruction past in-flight
work); shadow-via-prepass re-confirmed; `SHADER_SIZE` settled definitively
(encase 0.12 `METADATA.min_size()` rounds to alignment → 40 / 96, hedges
removed); UV-def cite kept (`bevy_pbr/src/render/mesh.rs:3309` — an agent's
"refutation" looked in the wrong crate) and extended with the prepass site
(`prepass/mod.rs:470,475`). **One refutation, orchestrator-verified: the D3
blink premise is wrong** — `PrepareAssets` (incl. the mesh allocator) is
chained before `Queue` (`bevy_render/src/lib.rs:317-322`,
`allocator.rs:201`), so a same-frame-created mesh draws that frame; decision
4 corrected, re-decision surfaced as D4. Shader data-flow completeness
(the pass's largest find): the fragment reads `fill_color` / `render_mode`
per run (`slug_text.wgsl:472,509,516`) — delivery committed: binding 105
becomes `visibility(vertex, fragment)`, vertex forwards atlas index in
`uv_b.x` (today's mechanism) and run index in `uv_b.y` (quad-uniform f32,
round to recover), fragment's uniform reads become run-table reads, all
recorded in the new Target-model bullet; prepass fragment confirmed
per-run-free (coverage-only discard); `depth_nudge` formula pinned
(`command_index × LAYER_DEPTH_BIAS`, 0 under OIT) and disambiguated from
`oit_depth_offset`; punch-out is prepass-blind (noted in decision 3).
Integration: `update_panel_text_alpha` gated to `PerRunMeshes` until 3a
(decision 10); batch-entity lifecycle pinned (spawn on first insert,
despawn on empty — the batch R10 analogue); per-batch material construction
+ generic `sync_anti_alias` coverage recorded (decision 4); batch path
never holds storage keys (no flip double-free) + flip-both-directions and
overlay-key-stable-from-frame-1 gate items (Step 2);
`commit_batch_buffers` named, owns pending-set writes, the D3 swap, and
the perf counters (frame flow step 5); `PendingBuffers` defined at the
sketch. Consistency: acceptance rows not owned by 3a/3b/4 assigned to the
Step-2 parity gate; results-table row aggregation explained; "per-run
routing" → "per-run toggling". Deferred-by-design confirmations: emoji.md
note stays a Step-4 task; slug_fx.md terminology already covered by this
doc's Terminology paragraph; perf.rs stale comment already queued in
Step 2.

**Review 3, cycle 2.** Adversarial verification of cycle-1 additions:
prepass IO carries `uv_b` under the same mesh-layout defs
(`prepass_io.wgsl:13-14,49-50`, `prepass/mod.rs:473-476`) — the
POSITION+UV_0+UV_1 inert mesh switches the defs on in both pipelines;
`visibility(vertex, fragment)` syntax confirmed; `setup_screen_space_view`
is an `On<Add, DiegeticPanel>` observer inserting `RenderLayers` at spawn
(`screen_space/mod.rs:49,196-222`) — the Step-2 frame-1 key-stability claim
holds; neither gate example mutates alpha modes at runtime; the
one-WGSL-file dual-vertex plan confirmed implementable via
`slug_text.wgsl`'s own `#ifdef PREPASS_PIPELINE` import pattern (`:13-19`),
now spelled out in decision 1. **One correction**: index recovery is
`u32(floor(..))` (today's `glyph_index`, `slug_text.wgsl:76-78`), not a
round — Target-model bullet fixed. **One phase-naming correction**: OIT
reuses `Transparent3d` (no separate OIT phase; the resolve pass adds no
items) — Step-2 counter description fixed. D3/D4 coherence repaired:
decision 4 no longer asserts the double-buffer as settled ("D3 chose...
D4 pending decides... no-blink binding either way"); pending-set creation
point pinned to the geometry write (frame-flow step 2), swap in step 5 one
frame later. Gates operationalized: Step 1 (screenshot criteria, shared
nudge function by construction + shadow-pass render check, Metal-capture
or debug-color first_vertex verification, CPU-side encase encode/decode
round-trip named, N/N+1/N+2 frame-stepped no-blink capture, hand-written
RunRecord transforms); Step 2 (ImageMagick `compare` for the diff,
constant-velocity overlay for transform lag, slab-watch grep recipe);
Step 3a (store-level unit tests via cargo nextest with the three named
cases); dynamic-edits acceptance row wired to the split-dirty-flag
counters.

## Proposed user decisions

### D1 — Base-material batch key strategy (status: resolved, cycle 2)

Converged on **intern by value** and recorded in decision 2. Evidence: ~30
`text_material` call sites, all clone the default and override at most
`unlit`; no texture customization anywhere; `Handle` fields compare by
`AssetId`; bitwise float comparison is safe for static defaults. The
alternatives (Handle-based authoring API change; silent field-subset hash)
solve a problem the codebase doesn't have. Not surfaced for user review.

### D4 — D3's factual premise was refuted (status: resolved — approved 2026-06-03)

User chose **direct same-frame swap — no double-buffer**. Background: D3
chose double-buffer based on review 2's claim that a capacity growth
blinks the batch for one frame. Review 3 refuted the claim,
orchestrator-verified: `PrepareAssets` — including the mesh allocator — is
chained before `Queue` in the render schedule
(`bevy_render/src/lib.rs:317-322`, `allocator.rs:201`), so a mesh +
buffers created in PostUpdate frame N are prepared and drawable in frame
N; there is no blink to prevent on the current schedule. Recorded in
decision 4 and the frame flow: growth creates, writes, and swaps the
replacement assets in the same frame; no `pending` slot, no swap protocol,
no content latency. The **no-blink requirement remains binding**: Step 1's
gate frame-steps a forced growth (N / N+1 / N+2 screenshots) and re-runs
on bevy upgrades, so the prepare-before-queue assumption is tested, not
trusted. The rejected alternative (keep the double-buffer as insurance
against schedule reordering) is preserved here so a future review doesn't
relitigate it.

### D3 — Capacity-growth one-frame gap (status: superseded by D4, 2026-06-03)

User chose **double-buffer from day one** on the premise that a capacity
growth blinks the batch for one frame. Review 3 refuted that premise and
D4 (above) reverted the mechanism to a direct same-frame swap. D3's
requirement — any blink is incorrect rendering — survives as the binding
no-blink gate in Step 1. The originally rejected alternative
(measure-first, mitigate only if visible) stays rejected: the requirement
is verified by gate, not deferred to measurement.

### D5 — Split Step 4 into 4a (flip default) / 4b (delete per-run path) (status: resolved — approved 2026-06-03)

User approved the split, with the flip-first ordering explicit: "first flip
it to batch default to see how it works." The plan above now reads Step 4a
(flip `TextGeometryPath::default()` to `BatchedRecords` and bake — full
suite + examples run batched-by-default while the toggle still offers the
per-run fallback) and Step 4b (delete the per-run path, the toggle, and the
scaffolding). Rationale preserved: bundling both into one change means the
default switch and the fallback's removal are never validated separately —
and a default still reading `PerRunMeshes` while the per-run systems are
deleted would point at removed code. Alternative (rejected by the review,
preserved here): keep Step 4 atomic and rely on the Step-2/3 gates having
proven the batch path.

### D2 — Split Step 3 into 3a / 3b (status: resolved — approved 2026-06-03)

User approved the split; the plan above now reads Step 3a (batch-membership
dynamics: cascade re-keying, `move_run`, shadow modes) and Step 3b
(per-record fields and shaders: punch-out, clip, depth-nudge layering,
examples sweep), each with its own gate. Rationale preserved: the two
clusters fail independently and have different validation targets
(batch-move correctness vs shader/depth parity).

Amended 2026-06-03 (Step-1 phase review, user-approved): the fragment
run-table read (`fill_color` / `render_mode` from
`run_records[u32(floor(in.uv_b.y))]`) moved from 3b's scope into Step 2,
because Step 2's parity gate cannot pass for multi-color batches without
it. The 3a/3b split itself stands; 3b keeps the verification work.

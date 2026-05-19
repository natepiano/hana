# GPU Glyph Rasterizer

## Status

Phase 1 close-out in progress on `feat/gpu-rasterizer`. End-to-end
GPU pipeline works: typography example launches; pressing G triggers
the parallel-atlas swap to GPU (completes in ~400ms); pressing G
again swaps back to CPU cleanly. Initial render under GPU is visually
correct (no spillover, no wrong-character substitution). The
swap-back regression that produced "To2oer42To" — caused by stale
completions and a shared global page-handle resource — is fixed by a
type-system rewrite that gives each `GlyphAtlas` its own GPU pipe
(channels, completion sink, and pending dispatch queue); ownership
now enforces "completions can only land on the atlas that issued
them." See **Phase 1 — Architecture rewrite (per-atlas async
plumbing)** below.

**O4 fixed (2026-05-18):** Incremental glyph adds under GPU mode now
land correctly. Root cause was `allocate_gpu_region` marking the page
`Dirty` unconditionally, which made `sync_to_gpu` blit the empty CPU
`page.pixels` mirror over the storage texture on the next frame —
wiping every previously dispatched GPU glyph on that page. Fix: only
mark `Dirty` when the page has no `image_handle` yet (genuinely new
page). Verified by cycling words in the typography example (Ångström,
fjord, etc. all render in full, and the CONTROLS / FONTS / QUALITY /
CAMERA panels stay intact across cycles).

Remaining before Phase 1 closes: **O5** (GPU render quality lower
than CPU) and the **(Msdf, Gpu) validator** inconsistency.

Phase 2 and Phase 3 not started.

Adds a GPU compute-shader rasterization path as a peer of the existing
CPU rasterizer (`fdsm`-backed MSDF / SDF). Routes glyph rasterization
through wgpu when the atlas is configured with `RasterBackend::Gpu`,
eliminating the per-glyph CPU wall time that dominates atlas warm-up
today.

**Long-term direction:** the GPU path is not required to match the CPU
path byte-for-byte and is allowed (encouraged, even) to diverge where
the GPU can do better — apex fidelity, thin strokes, fractional
boundary coverage. The first goal is "good enough and a lot faster."
If that lands, the GPU path becomes the sole path and the `fdsm` CPU
rasterizer retires. Until then, both backends coexist and the user
picks via `RasterBackend::{Cpu, Gpu}`.

## Motivation

The benchmark `crates/bevy_diegetic/benches/glyph_rasterization.rs`
established baseline per-glyph rasterization cost on the current CPU
path:

| Workload | Wall time (8 threads, MSDF) |
|---|---|
| JetBrains Mono ASCII-94 @ canonical 128 | 90 ms |
| JetBrains Mono ASCII-94 @ canonical 256 | 285 ms |
| EB Garamond ASCII-94 @ canonical 128 | 248 ms |
| EB Garamond ASCII-94 @ canonical 256 | **838 ms** |

The EB Garamond `V` at canonical 256 alone takes **75 ms on a single
worker thread**. The cost is dominated by `fdsm`'s per-pixel
"nearest-edge distance" search inside `generate_msdf`, which is O(pixels
× edges) per glyph.

The Rust SIMD ecosystem has not produced a vectorized MSDF generator.
The CPU lever space is exhausted: caching `Face::parse` saves ~30 µs out
of hundreds of ms (proved by the `face_parse` micro-bench), reusing the
`Rgb32FImage` buffer saves <1 ms out of 800+, and adding worker threads
past 8 hits diminishing returns on the test machine.

GPU compute is the next workable lever. Each output pixel is independent;
the nearest-edge search vectorizes naturally across thousands of GPU
threads. Bevy ships with `wgpu` already wired through every render
pipeline in the crate, so this requires no new dependency and no FFI.

The expected wins, drawn from published GPU SDF generation results:

- 10–100× per-glyph speedup for SDF (single channel, no edge coloring).
- 5–50× per-glyph speedup for MSDF (extra cost for the median /
  edge-coloring logic in the shader).
- **Zero CPU → GPU atlas upload** — the compute pass writes the bytes
  directly into the atlas page's storage texture, eliminating the
  per-page `texture_to_buffer` upload that today's dirty-page mechanism
  performs.

## Non-goals

- Replacing the CPU path. `RasterBackend::Cpu` remains the default.
  GPU is opt-in per atlas, switchable at runtime through the same
  `AtlasSlot` swap machinery that already exists for `DistanceField`
  changes.
- Per-glyph backend selection within one atlas. An atlas is fully CPU
  or fully GPU; mixing within a single atlas would require dual upload
  paths and dual ready-state tracking for no real benefit.
- Mobile / WebGL backend parity. Storage textures require
  `compute_shader` support, which excludes WebGL2 and some older mobile
  GPUs. On unsupported backends the atlas falls back to
  `RasterBackend::Cpu` automatically.
- Implementing MSDF on the GPU in the first PR. Phased rollout starts
  with SDF on GPU (much simpler shader, byte-equivalent to CPU SDF for
  validation), then layers MSDF.
- Replacing `fdsm`. The CPU path keeps `fdsm` and `fdsm-ttf-parser`
  exactly as they are today.

## Architecture

### Backend orthogonality

The current `DistanceField` enum describes what each atlas texel
encodes (Msdf = 3 channels of pseudo-distance, Sdf = 1 channel of true
distance). The new axis describes who computes it:

```rust
// existing — what is stored per pixel
pub enum DistanceField {
    Msdf,
    Sdf,
    // Mtsdf,  ← reserved
}

// new — who produces the bytes
pub enum RasterBackend {
    Cpu,
    Gpu,
}
```

These are independent. `RasterBackend` lives in `text/atlas_config.rs`
alongside `RasterQuality` and `GlyphWorkerThreads`. `AtlasConfig`
gains the `backend` field:

```rust
pub struct AtlasConfig {
    pub quality:              RasterQuality,
    pub glyphs_per_page:      u16,
    pub glyph_worker_threads: GlyphWorkerThreads,
    pub distance_field:       DistanceField,
    pub backend:              RasterBackend,  // NEW
}
```

`AtlasConfig::new` validates the `(backend, distance_field)` pair and
rejects unsupported combinations with a clear error. The supported
matrix is:

| Backend | Sdf | Msdf |
|---|---|---|
| Cpu | ✓ | ✓ |
| Gpu | ✓ (Phase 1) | ✓ (Phase 2) |

`(Gpu, Msdf)` returns `Err("MSDF on GPU is not yet implemented (Phase 2)")`
until Phase 2 lands. After Phase 2 the same validator stays as
forward-protection for any future backend that does not yet support
all distance-field variants.

Device-feature loss (WebGL2, mobile without `compute_shader`) is
handled separately at `GpuRasterizerPlugin::build` time: an atlas
configured `RasterBackend::Gpu` on an unsupported device is downgraded
to `Cpu` with a warning log. Config-creation validation rejects
combinations; plugin-init validation handles device capability.

The existing parallel-atlas swap machinery (`AtlasSlot::Single` /
`AtlasSlot::Swapping`) already handles "user changed configuration →
spin up pending atlas → swap when ready". It generalizes to backend
changes with no structural edits — the swap key just widens from
`(distance_field, canonical_size)` to `(distance_field, canonical_size,
backend)`.

### The atlas remains backend-agnostic at the insert layer

`GlyphAtlas::insert_completed(key, bitmap, metrics)` already accepts a
fully-rasterized bitmap and packs it into a page. The CPU path goes
through this. The GPU path goes through a sibling method:

- **Cpu**: spawn `MsdfRasterizer::rasterize(...)` on the worker pool
  (today's behavior). The worker returns bytes via channel; the atlas
  drains the channel each frame and calls `insert_completed`.
- **Gpu**: build a `GpuGlyphRequest`, push to the render-world dispatch
  queue. The dispatch system encodes a compute pass that writes directly
  into the atlas page storage texture. A `GpuGlyphCompleted` event
  triggers `atlas.insert_completed_gpu(key, metrics)`. There is no
  `RasterizedBitmap` on this path — the pixels are already in the
  texture.

`insert_completed_gpu` signature:

```rust
pub fn insert_completed_gpu(
    &mut self,
    key: GlyphKey,
    metrics: GlyphMetrics,
);
```

It removes `key` from `in_flight` and inserts `metrics` into the
glyphs map. It is backend-agnostic by signature: nothing in its body
references wgpu or GPU types.

The `Rasterizer` trait stays CPU-only. Its synchronous
`fn rasterize(&self, ...) -> Option<RasterizedBitmap>` contract does
not fit the async render-schedule-bound GPU path, so the GPU
dispatcher is a system, not a `Rasterizer` implementation.

## Module structure

```
crates/bevy_diegetic/src/text/
├── atlas.rs                          (existing — add backend dispatch in get_or_insert)
├── atlas_config.rs                   (existing — add `backend` field)
├── atlas_slot.rs                     (existing — no edits needed)
├── constants.rs                      (existing — add GPU-specific constants)
├── msdf_rasterizer/                  (existing — unchanged)
│   ├── mod.rs
│   ├── sdf.rs
│   └── parity.rs
└── gpu_rasterizer/                   (NEW)
    ├── mod.rs                        — `GpuRasterizerPlugin`, public types
    ├── pipeline.rs                   — wgpu compute pipeline + bind group layouts
    ├── edges.rs                      — bezier contour → flat edge buffer (CPU prep)
    ├── request.rs                    — `GpuGlyphRequest`, `GpuGlyphCompleted` event
    ├── dispatch.rs                   — render-schedule dispatch system
    ├── extract.rs                    — main world ↔ render world bridge
    ├── readback.rs                   — optional CPU mirror for debug PNG dumps
    └── shaders/
        ├── sdf_gen.wgsl              — single-channel SDF generation
        └── msdf_gen.wgsl             — multi-channel SDF generation (phase 2)
```

### File-by-file responsibilities

#### `mod.rs`

Public surface for the GPU rasterizer. Declares `GpuRasterizerPlugin`,
implemented as a single `Plugin::build` (Bevy 0.18 does not use a
`Plugin::finish` two-phase pattern in this crate). `build` performs:

- Main-app side: inserts `GpuGlyphRequestQueue` and `GpuGlyphBudget`
  resources; registers the `GpuGlyphCompleted` event; registers the
  atlas-side observer that calls `insert_completed_gpu` on each event
  (`app.add_observer(...)`, matching the existing `AtlasSwapStarted` /
  `FontRegistered` pattern in the crate).
- Calls `load_internal_asset!` to embed the WGSL shaders with stable
  handles (e.g., `GPU_RASTERIZER_SDF_SHADER_HANDLE`) referenced by
  `pipeline.rs`. Asset paths are relative to the crate root:
  `"text/gpu_rasterizer/shaders/sdf_gen.wgsl"`.
- Calls `app.sub_app_mut(RenderApp)` to register the render-world
  resources (`RenderGlyphQueue`, `GpuGlyphCompletionBuffer`) and adds
  `dispatch_glyph_compute` into the `Render` schedule, in
  `RenderSystems::PrepareBindGroups`. The system needs `RenderAssets<GpuImage>`
  populated (which happens in `PrepareAssets`) and the `PipelineCache` to have
  resolved the bind-group layout (which happens during `Prepare`). The Bevy
  0.18 render-schedule ordering is `ExtractCommands → PrepareAssets →
  PrepareMeshes → CreateViews → Specialize → PrepareViews → Queue →
  QueueMeshes → QueueSweep → PhaseSort → Prepare → … → PrepareBindGroups →
  Render → Cleanup → PostCleanup` — note that `Queue` runs *before*
  `Prepare`, so the dispatch system cannot live in `Queue.after(Prepare)`.
- Validates wgpu device limits (see "wgpu limits validation"). On
  failure, logs a warning, skips dispatch-system insertion, and any
  GPU-backed atlas constructed later falls back to CPU.

Public types:

```rust
pub struct GpuRasterizerPlugin;

/// Per-frame budget for GPU glyph dispatch.
#[derive(Resource, Clone, Copy, Debug)]
pub struct GpuGlyphBudget {
    /// Maximum number of glyph dispatches per frame across all
    /// queued glyphs. Default 16. See "Frame budget management" for
    /// tuning guidance.
    pub per_frame: u32,
}

impl Default for GpuGlyphBudget {
    fn default() -> Self {
        Self { per_frame: 16 }
    }
}
```

Compute pipelines require explicit `App::sub_app_mut(RenderApp)`
registration; the crate's existing text rendering goes through
`MaterialPlugin`, so the GPU rasterizer is the first consumer of this
pattern. Match the Bevy 0.18 reference at
`examples/shader/compute_shader_game_of_life.rs`.

#### `pipeline.rs`

`ComputePipeline` setup. Bind group layout (WGSL syntax):

```wgsl
@group(0) @binding(0) var<storage, read>       edges:   array<EdgeSegment>;
@group(0) @binding(1) var<storage, read>       glyphs:  array<GlyphHeader>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform>             params:  RasterParams;
```

All four bindings are `wgpu::ShaderStages::COMPUTE` visibility. The
output is `write`-only (the shader never reads back from it).

`RasterParams` (std140 layout, 16 bytes total, 4-byte aligned fields):

| Offset | Field | Type |
|---|---|---|
| 0 | `sdf_range` | f32 |
| 4 | `padding_texels` | u32 |
| 8 | `distance_field` | u32 (0 = MSDF, 1 = SDF, 2 = MTSDF reserved) |
| 12 | `glyph_count` | u32 |

`sdf_range` is converted from the config's f64 `DEFAULT_SDF_RANGE` to
f32 at dispatch-encoding time.

`GlyphHeader` (per-glyph, indexed by `workgroup_id.z`; 32 bytes):

| Offset | Field | Type |
|---|---|---|
| 0  | `edge_offset` | u32 (first index into `edges`) |
| 4  | `edge_count` | u32 |
| 8  | `atlas_origin` | vec2&lt;u32&gt; (where in the page to write) |
| 16 | `bitmap_size` | vec2&lt;u32&gt; (width, height in texels) |
| 24 | `_padding` | [u32; 2] (keeps array stride a multiple of 16) |

Edges are pre-baked into pixel space by `build_edge_buffer` (see
`edges.rs`), so the kernel does not need an em→px scale. The 8 trailing
bytes of pad keep storage-buffer array alignment intact and reserve
room for Phase 2 fields without an ABI break.

`EdgeSegment`: 8 floats covering up to four control points (cubic
beziers: P0, P1, P2, P3), plus a `kind` u32. Total 36 bytes per edge.

Bit layout of `kind`:

| Bits | Field |
|---|---|
| 0–1  | discriminant: 0 = linear (uses P0, P1), 1 = quadratic (P0, P1, P2), 2 = cubic (P0, P1, P2, P3), 3 = reserved |
| 2–4  | MSDF channel mask: 0 = none, 1 = R, 2 = G, 3 = B, 4 = RG, 5 = GB, 6 = RB, 7 = RGB |
| 5–31 | reserved |

Phase 1 (SDF) ignores the channel-mask bits. Phase 2 (MSDF) sets
them from `fdsm::edge_coloring_simple`.

A single 8-float layout keeps the buffer contiguous and avoids
divergent indirect reads.

`GpuRasterizerPipeline` resource (render-world):

```rust
#[derive(Resource)]
pub(super) struct GpuRasterizerPipeline {
    pub pipeline: wgpu::ComputePipeline,
    pub layout:   wgpu::BindGroupLayout,
}
```

Constructed once in `GpuRasterizerPlugin::build`. The bind group itself
is rebuilt per dispatch (it references per-frame buffers).

Pipeline shader loaded via `load_internal_asset!` so the WGSL ships
embedded in the crate binary with a known `Handle<Shader>` reachable
at plugin init.

#### `edges.rs`

CPU-side translation from fdsm's bezier contour list into the flat
`EdgeSegment` array the GPU expects. Runs on the same worker pool as
the CPU path today, called from the main-world `get_or_insert` dispatch
(so request building doesn't block the main thread).

Functions:

```rust
pub(super) fn build_edge_buffer(
    font_data: &[u8],
    glyph_index: u16,
    canonical_size: u32,
    sdf_range: f32,
    padding: u32,
) -> Option<GpuGlyphRequestBody>;

pub(super) struct GpuGlyphRequestBody {
    pub edges:       Vec<EdgeSegment>,
    pub bitmap_size: UVec2,
    pub bearing:     Vec2,
}
```

`bitmap_size` comes from `compute_bitmap_size` in `text/constants.rs`
(see "Atlas integration → Formula bifurcation risk") — the same `const
fn` the CPU path calls — so the two paths cannot drift.

For MSDF (phase 2), this is also where edge coloring runs (the same
fdsm edge-coloring routine the CPU path uses today), with the channel
mask packed into the `kind` field of each `EdgeSegment`.

#### `request.rs`

The main-world request queue and the completion event:

```rust
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct GpuGlyphRequestQueue {
    pending: Vec<GpuGlyphRequest>,
}

#[derive(Clone)]
pub(super) struct GpuGlyphRequest {
    pub key:            GlyphKey,
    pub body:           GpuGlyphRequestBody,
    pub canonical_size: u32,
    pub sdf_range:      f32,
    pub distance_field: DistanceField,
    pub atlas_origin:   UVec2,
    pub page_index:     u32,
}

#[derive(Event)]
pub struct GpuGlyphCompleted {
    pub key:           GlyphKey,
    pub bitmap_size:   UVec2,
    pub bearing:       Vec2,
    pub atlas_origin:  UVec2,
    pub page_index:    u32,
}
```

Public types: `GpuGlyphRequestQueue`, `GpuGlyphCompleted`,
`GpuGlyphBudget`. Internal: `GpuGlyphRequest`, `GpuGlyphRequestBody`
(pub(super) for cross-module access within `gpu_rasterizer/`; not
re-exported).

The atlas allocates the page region (the same shelf-allocator code it
uses today) at `get_or_insert` time, synchronously, before any GPU
work. `atlas_origin` and `page_index` are stored on the request so the
shader knows where to write and `GpuGlyphCompleted` carries everything
the atlas needs to register the glyph as ready without re-running
allocation logic.

If `build_edge_buffer` returns `None` (zero-dimension glyph, oversized
glyph), `get_or_insert` inserts the existing CPU-path sentinel
`GlyphMetrics::INVISIBLE` and does not enqueue, matching CPU behavior
and preventing re-queue spam on the next lookup.

#### `dispatch.rs`

The render-schedule system that drains the queue and encodes compute
passes. Lives in the render world (extracted from main world each
frame).

```rust
pub(super) fn dispatch_glyph_compute(
    pipeline:        Res<GpuRasterizerPipeline>,
    budget:          Res<GpuGlyphBudget>,
    render_device:   Res<RenderDevice>,
    render_queue:    Res<RenderQueue>,
    mut queue:       ResMut<RenderGlyphQueue>,
    atlas_pages:     Res<RenderAtlasPages>,
    mut completions: ResMut<GpuGlyphCompletionBuffer>,
) {
    let take = budget.per_frame.min(queue.pending.len() as u32) as usize;
    let dispatched = queue.pending.drain(..take).collect::<Vec<_>>();
    if dispatched.is_empty() { return; }

    // 1. Partition by page_index.
    let mut by_page: HashMap<u32, Vec<GpuGlyphRequest>> = HashMap::new();
    for req in dispatched { by_page.entry(req.page_index).or_default().push(req); }

    // 2. For each page, build one edge buffer + one header buffer
    //    (concatenated) and encode one compute pass that dispatches
    //    `(ceil(bitmap.x/8), ceil(bitmap.y/8), 1)` per glyph header
    //    (one workgroup grid per glyph, indexed by workgroup_id.z).
    // 3. render_queue.submit(...) the encoder.
    // 4. Append completion records into `completions`. The main-world
    //    extract-back system drains and fires `GpuGlyphCompleted`
    //    events via `commands.trigger()`.
}
```

`RenderDevice` and `RenderQueue` are pre-inserted by Bevy's
`RenderPlugin` and are available to any system in the render schedule.
`RenderQueue` is `Res<RenderQueue>` (not `ResMut`) because the
underlying `Arc<wgpu::Queue>` is internally synchronized — mutable
borrow blocks other systems for no reason.

**Per-page grouping.** One edge buffer and one header buffer are built
per compute pass (per atlas page). The edge buffer concatenates every
glyph's edges targeting that page; each glyph header's `edge_offset`
points into the concatenated buffer. The total edge-buffer size per
pass is bounded by `wgpu limits validation` (see below).

**Render-to-main completion bridge.** Render-world `EventWriter<T>`
events do not auto-extract back to the main world in Bevy 0.18, and
`ExtractResourcePlugin<T>` only copies main→render. The completion
bridge uses an `Arc<Mutex<Vec<GpuGlyphCompletedRecord>>>` wrapped in a
`GpuGlyphCompletionBuffer` resource: both worlds hold a clone of the
same `Arc`, so the render-world dispatcher's `push(record)` and the
main-world drain system's `drain()` operate on the same inner `Vec`.
The crate's existing pattern (`AtlasSwapStarted` / `FontRegistered` at
`crates/bevy_diegetic/src/text/mod.rs:186, 273`) is
`commands.trigger(...)` from a main-world system, observed via
`app.add_observer`. The main-world `drain_gpu_completions` system
reads the shared buffer and calls
`commands.trigger(GpuGlyphCompleted { ... })` for each record.

`RenderAtlasPages` resource (render-world):

```rust
#[derive(Resource, Default, Clone, ExtractResource)]
pub(super) struct RenderAtlasPages {
    /// page_index → cloned `Handle<Image>` for that page's storage texture.
    pub pages: Vec<Handle<Image>>,
}
```

Extracted each frame from the main-world atlas. Dispatch uses these
handles to look up the wgpu texture in `RenderAssets<Image>` and bind
it as a storage write target.

**Workgroup sizing.** 8×8 = 64 threads per workgroup. The dispatch is
**one grid for the whole page**: `(max_x, max_y, glyph_count)` where
`max_x = max(ceil(bitmap.x / 8))` and `max_y = max(ceil(bitmap.y / 8))`
across all glyphs in the page. The shader reads `glyphs[workgroup_id.z]`
to find its glyph header, then bounds-checks
`global_invocation_id.xy < header.bitmap_size` before writing — over-
dispatch is bounded by that per-glyph check. This trades a small
amount of wasted threads on the largest dimension for a single compute
pass per page instead of N. Each thread writes one output pixel; not
all glyph dimensions are multiples of 8 (padding produces 130×130
etc.) so the bounds check is also the standard WGSL idiom for
over-dispatched grids.

**Queue backpressure.** `RenderGlyphQueue.pending` is unbounded. If
the user enqueues faster than the budget drains (e.g., streaming a CJK
font with thousands of glyphs at the default `per_frame = 16`), the
queue grows. The dispatch system logs a warning when `pending.len()`
crosses a high-water mark (default 4096 = ~4× a typical full-font
warm-up). The recommended response is to pre-warm during loading or
raise `GpuGlyphBudget.per_frame`. See "Limitations → Unbounded request
queue".

**Compute-pass coexistence.** The dispatch encodes into the same
`wgpu::Queue` as other render-schedule compute passes. wgpu serializes
queue submissions but does not order independent compute passes
relative to each other. Apps that introduce custom compute passes
sampling from the atlas page texture in the same frame must order them
after `RenderSystems::Render` or accept undefined relative ordering. The
`GpuRasterizerPlugin` docstring spells this out.

#### `extract.rs`

Bevy 0.18 cross-world plumbing in both directions:

- **Main → render**: `ExtractResourcePlugin<GpuGlyphRequestQueue>` and
  `ExtractResourcePlugin<RenderAtlasPages>`. Both resources derive
  `ExtractResource + Clone + Default`. Extraction clones the resource
  from main world to render world each frame. After extract, the
  dispatch system drains the cloned queue in the render world; a
  separate main-world system clears the original queue post-extract.
- **Render → main**: the dispatch system pushes records into a
  `GpuGlyphCompletionBuffer` resource. An
  `Extract<Res<GpuGlyphCompletionBuffer>>` system on the main world
  reads the records and calls `commands.trigger(GpuGlyphCompleted { ... })`
  for each, then clears the render-world buffer for the next frame.

The crate uses `commands.trigger` rather than `mpsc` channels because
Bevy's render schedule is synchronous on a single thread per frame —
`mpsc` would add lock contention with no parallelism gained.

**Extract-window ordering.** Extract runs once per frame, before
dispatch. A request enqueued on the main world after extract but before
dispatch is queued for the next frame's extract. Apps that need
synchronous per-frame submission should pre-warm during
`FontRegistered` rather than enqueue mid-frame.

#### `readback.rs`

The GPU path mirrors each rasterized page on the CPU by encoding a
`copy_texture_to_buffer` after the compute pass, with a `map_async`
callback that populates the page's `pixels` field on completion. The
`page_pixels` accessor and `dump_atlas_png` test work identically
under both backends.

Per-glyph cost: one extra GPU copy command plus a buffer mapping —
microseconds at 128², tens of microseconds at 256².

The `pixels` field lags the GPU texture by one frame (the `map_async`
callback fires next frame at earliest) and may lag longer under driver
backpressure. It is a debug aid — anything that needs the authoritative
glyph bytes should sample from the GPU texture directly. Read failures
are logged but not fatal; the next frame's copy retries.

#### `shaders/sdf_gen.wgsl`

Per output pixel:

1. **Bounds check.** `if any(global_invocation_id.xy >= bitmap_size) { return; }`
2. Compute the sample point's em-space coordinate from
   `global_invocation_id`, `bitmap_size`, and `em_to_px_scale`.
3. **Per-edge distance.** Loop over the glyph's edges
   (`edge_offset..edge_offset + edge_count`). For each edge, compute the
   *Euclidean* signed distance — *not* the MSDF pseudo-distance:
   - Project the sample point onto the edge's parametric curve
     (linear: line projection; quadratic/cubic: numerical Newton or
     analytic root).
   - Clamp the parameter `t` to `[0, 1]`. If the foot of the
     perpendicular falls outside the segment, the distance is to the
     nearest endpoint.
   - Track the absolute-smallest distance seen and the sign at that
     point.
4. **Sign correction (per-pixel parallelization, *not* fdsm's scanline
   algorithm).** The CPU `fdsm::correct_sign_msdf` is a scanline fill
   that mutates state row by row; that is not directly parallelizable
   per-pixel. The GPU equivalent is a per-pixel ray test: cast a
   horizontal ray from the sample to +∞, count signed edge crossings
   (using each edge's y-monotone subdivisions for quadratics/cubics),
   and apply the non-zero winding rule (a TrueType requirement —
   composite glyphs may self-intersect, so even-odd parity is wrong).
   The signed-sum sign gives inside (positive) / outside (negative).
   The result agrees with the CPU scanline pass on every glyph the
   parity test exercises; self-intersecting composite glyphs are
   covered by including a known composite in the parity test's
   sample set.
5. **Normalize** the signed distance to `[0, 1]`:
   `d = clamp(signed_distance / sdf_range + 0.5, 0.0, 1.0)`.
6. **Write** `vec4(d, d, d, 1.0)` to
   `output[atlas_origin + global_invocation_id.xy]`.

#### `shaders/msdf_gen.wgsl` (phase 2)

MSDF differs from SDF in two ways:

1. **Pseudo-distance, not Euclidean distance.** For each edge, compute
   the perpendicular distance to the *infinite line through the edge's
   tangent at the closest parameter `t`*, then clamp `t` to `[0, 1]`.
   If the perpendicular foot falls outside the segment, fall back to
   Euclidean distance to the nearest endpoint. This is what preserves
   sharp corners that single-channel SDF rounds off.
2. **Per-channel nearest-edge search.** Each edge carries a channel
   mask (R / G / B / RG / GB / RB). The shader loops three times —
   once per output channel `C ∈ {R, G, B}` — and for each channel
   considers only the subset of edges whose mask includes `C`. The
   absolute-smallest pseudo-distance among that subset is written to
   channel `C`. After all three channels are computed, write the RGBA
   texel (alpha = 1.0).

Error correction (the third `fdsm` pass) is a separate compute pass
that scans for pixels where the median of the three channels would
mis-classify and patches them. Implemented in Phase 3.

## Data flow

### Glyph request lifecycle (GPU backend)

```
main world
  enqueue_gpu_glyph(slot, queue, key, font_data, ...)
    │
    ├── if backend == Cpu: caller routes through atlas.get_or_insert instead
    │
    └── if backend == Gpu:
          1. build_edge_buffer (synchronous in Phase 1; worker-pool
             integration planned for Phase 1.5 — see "Worker pool
             relevance")
          2. allocate page region (existing shelf allocator)
          3. mark key as in_flight
          4. push GpuGlyphRequest into queue

extract schedule (every frame)
  copy GpuGlyphRequestQueue from main → RenderGlyphQueue in render world
  clear main-world queue

render schedule (RenderSystems::PrepareBindGroups)
  dispatch_glyph_compute system:
    1. drain up to `budget` requests from RenderGlyphQueue
    2. partition by target atlas page (one compute pass per page)
    3. upload edge buffer + header buffer to storage buffers
    4. bind atlas page texture as storage write target
    5. encode compute pass, dispatch one workgroup grid per glyph
    6. append completion records into GpuGlyphCompletionBuffer

extract back (render → main)
  shared Arc<Mutex<Vec<...>>> inside GpuGlyphCompletionBuffer: render-side
  dispatcher pushed records during PrepareBindGroups; main-side
  drain_gpu_completions reads them and fires GpuGlyphCompleted events

main world (next frame)
  observer on GpuGlyphCompleted:
    atlas.insert_completed_gpu(event.key, event.metrics)
      └── remove from in_flight
      └── insert metrics into glyphs map
```

### Worker pool relevance

Today's CPU path uses a `TaskPool` of 8 worker threads to parallelize
glyph rasterization. The GPU path does not need this pool for the
rasterization itself — the GPU is the parallel device. But it does need
it for two CPU-side prep tasks per glyph:

- `Face::parse` (~300 ns, trivial)
- `build_edge_buffer` (loads the glyph outline via `load_shape_from_face`
  and transforms it to a flat array — comparable to the existing call cost,
  ~hundreds of µs per glyph)

The pool is reused: the atlas owns one `TaskPool` regardless of backend,
GPU-backed atlases just spawn edge-buffer-build tasks on it instead of
fdsm-rasterize tasks. The `worker_pool()` accessor and pool-sharing
machinery for the swap path stay unchanged.

## Atlas integration

### Storage texture format

Atlas pages already use `TextureFormat::Rgba8Unorm` (confirmed in
`crates/bevy_diegetic/src/text/atlas.rs:444, 477`), so no format
migration is required. Storage textures require non-sRGB formats on
most backends — `Rgba8Unorm` satisfies this. Distance values are
written and read linearly; the text fragment shader treats texel
channels as distance scalars, not colors, so there is no gamma issue.

`Image` usage flags: `GlyphAtlas::upload_to_gpu` and `sync_to_gpu` set
`texture_descriptor.usage |= STORAGE_BINDING | COPY_DST |
TEXTURE_BINDING` on every page image before insertion into
`Assets<Image>`. Without `STORAGE_BINDING`, wgpu's validation layer
rejects the compute-pass bind group at runtime. `COPY_DST` keeps the
existing dirty-page upload path working; `TEXTURE_BINDING` lets the
text fragment shader sample the page.

### Page allocation timing

Today the page allocator runs inside `insert_completed` — the atlas
allocates a region only after the bitmap arrives. The GPU path inverts
this: the shader needs to know `atlas_origin` before it dispatches, so
allocation must happen at `get_or_insert` time.

The shelf allocator (`etagere`) is stable and doesn't depend on bitmap
contents — only on dimensions, which `build_edge_buffer` produces
synchronously from the glyph bounding box.

**Formula bifurcation risk.** The bitmap-size formula must produce
identical results between CPU `rasterize_msdf_bitmap` (in
`msdf_rasterizer/mod.rs`) and GPU `build_edge_buffer` (in
`gpu_rasterizer/edges.rs`). To prevent silent divergence, extract the
size computation to a shared `pub(super) fn compute_bitmap_size`
(non-const because the existing CPU formula uses f64 `ceil`, which is
not stable in `const` context)
in `text/constants.rs` (or a small `text/bitmap_dims.rs` helper module)
and call it from both. Add a parity test that asserts CPU and GPU
dimension computations match for a sample glyph set.

**Edge cases at allocation time** (synchronous, before dispatch):

- **Zero-dimension glyphs** (space, certain combining marks): the
  function returns `None`, the key is not queued, and the lookup
  reports unrenderable — same as today's CPU behavior in
  `rasterize_msdf_bitmap` (mod.rs:180–182).
- **Oversized glyphs** (bitmap larger than page dimensions): page
  allocation fails; the function returns `None` and logs once per
  font. No retry. Matches CPU behavior.
- **Page reuse synchronization.** Compute dispatches are async; page
  regions are not reallocated until the dispatching glyph is marked
  complete (see "Synchronization").

### Multi-page atlases

When a glyph won't fit on the current page, the atlas allocates a new
page. The GPU path's dispatch system already groups dispatches by page,
so a single frame can write to multiple pages in parallel (one compute
pass per page).

## Frame budget management

GPU rasterization runs in the render schedule, sharing wgpu queue time
with the main render passes. A 94-glyph warm-up dispatched all in one
frame would stall the GPU for a few ms before any text-bearing draws —
visible as a frame hitch.

Mitigation: `GpuGlyphBudget.per_frame` caps the number of glyph
dispatches per frame (default 16). A 94-glyph warm-up spreads over
~6 frames at 60 fps = 100 ms — still vastly better than the 838 ms CPU
EBG@256 case, with no perceptible hitch in the steady state.

Tuning table:

| Use case | Budget |
|---|---|
| Steady-state interactive (default) | 16 |
| Loading screen / batch pre-warm | `u32::MAX` (drain in one frame) |
| Low-end GPU / mobile | 4–8 |
| First-frame visible text (large paragraph) | dispatch synchronously via pre-warm before frame |

**Latency characteristic.** A GPU-backed glyph becomes visible roughly
3–4 frames after `get_or_insert`: frame N enqueue → frame N+1 extract +
dispatch → frame N+2 GPU completes + extract back → frame N+3
`insert_completed_gpu` and atlas marks ready. At 60 fps that is
~50 ms — imperceptible for interactive typing but visible if a large
paragraph appears on-screen all at once (text "pops in" glyph-by-glyph
as the budget drains). Apps that need instant first-frame visibility
must pre-warm during loading, not lean on GPU rasterization mid-frame.

## Phased rollout

### Phase 1 — SDF on GPU, no MSDF

Goal: prove the architecture pays off before committing to MSDF shader
complexity.

Files added:
- `gpu_rasterizer/mod.rs` (plugin; AtlasConfig validator rejects (Gpu, Msdf) until Phase 2)
- `gpu_rasterizer/pipeline.rs` (SDF pipeline only)
- `gpu_rasterizer/edges.rs` (no edge coloring)
- `gpu_rasterizer/request.rs`
- `gpu_rasterizer/dispatch.rs` (SDF code path only)
- `gpu_rasterizer/extract.rs`
- `gpu_rasterizer/readback.rs` (always-on, no cfg gate)
- `gpu_rasterizer/shaders/sdf_gen.wgsl`

Files edited:
- `text/atlas_config.rs` — add `backend: RasterBackend` field + a
  validator (`AtlasConfig::new` rejects (Gpu, Msdf) until Phase 2).
- `text/atlas.rs` — dispatch on backend in `get_or_insert`; add
  `insert_completed_gpu` method.
- `text/constants.rs` (or new `text/bitmap_dims.rs`) — extract the
  bitmap-size formula to a shared `const fn` callable from both CPU
  and GPU paths.
- `text/mod.rs` — add `GpuRasterizerPlugin` to default plugin set.
- `lib.rs` — re-export `RasterBackend`, `GpuGlyphBudget`,
  `GpuGlyphCompleted`. `DistanceField` stays re-exported as today.

Acceptance:
- `cargo bench glyph_rasterization -- warmup_burst/jbm_ascii_128_sdf`
  shows GPU backend ≥ 5× faster than CPU backend.
- The typography example's `G` key toggles backend at runtime via
  the `AtlasSlot` swap, no visible flicker. (Originally proposed as
  `B`; `B` was already `RasterQuality::Small`.)
- Snapshot test on the software adapter: render a fixed glyph set
  through the GPU path, save the atlas page PNG, fail if it ever
  changes without a deliberate `--update-snapshots` opt-in. The GPU
  output is its *own* golden reference — no byte-for-byte comparison
  to the CPU rasterizer. Divergence from CPU is allowed and may be
  desirable (e.g. better apex / thin-stroke fidelity, fractional
  coverage at boundaries). Visual review covers what snapshots
  can't.
- `gpu_cpu_bitmap_size_parity` test: CPU and GPU dimension
  computations match exactly for a sample glyph set. (Bitmap-size
  parity stays a hard requirement — the bitmap dimensions are
  upstream of the rasterizer and a mismatch would break atlas
  packing regardless of kernel.)

### Phase 1 Retrospective

**What landed:**
- All Phase 1 files added; module wiring, `RasterBackend` validator,
  `insert_completed_gpu` path, mpsc channel for worker→main edge-build
  results, Arc<Mutex> bridge for render→main completions, atlas-side
  `GpuGlyphDispatcher` trait + `ChannelGpuDispatcher` impl.
- `drive_atlas_swap` includes backend in the swap-trigger comparison;
  dispatcher handle is inherited by the pending atlas.
- `parity.rs` Rust port of the SDF kernel; 5 test cases pass (JBM A/W/O,
  EB Garamond A/V).
- Typography example wired with a runtime backend toggle and a status
  chip.
- wgpu limits validated at pipeline init.

**What deviated from the plan:**
- Edge-build was moved off the main thread onto the atlas worker pool
  via mpsc (D1=B decision). The plan originally described synchronous
  edge build inside `enqueue_gpu_glyph`. The async path is documented
  inline in the lifecycle section.
- Render-schedule set is `RenderSystems::PrepareBindGroups`, not the
  originally-proposed `Queue.after(Prepare)` — Queue runs before Prepare
  in Bevy 0.18 so that ordering was impossible.
- `GlyphHeader` lost its `em_to_px_scale: f32` field (kernel never used
  it; edges are pre-baked in pixel space). Replaced by `[u32; 2]`
  padding to keep the 32-byte stride.
- Completion bridge is `Arc<Mutex<Vec<...>>>` rather than
  `ExtractResourcePlugin` — extract only travels main→render.
- Backend toggle keybinding is `G`, not `B` (B was already taken by
  `RasterQuality::Small`).

**Surprises:**
- WGSL ray-cast winding vs. fdsm's scanline winding disagree by
  sub-pixel signed distance on glyph boundaries — most visible as a thin
  halo on closed contours (e.g. the 'O' counter ring, ~10% of pixels
  differ at sdf_range=4). Not a kernel bug; inherent algorithmic
  difference. Parity tolerance had to be relaxed.

**Acceptance gaps (Phase 1 not fully complete):**
1. **GPU bench acceptance unmet.** Plan called for
   `cargo bench glyph_rasterization -- warmup_burst/jbm_ascii_128_sdf`
   to show GPU ≥ 5× faster than CPU. What landed
   (`warmup_burst_gpu_main_thread`) measures only the main-thread cost
   of bitmap-size + region allocation + worker-pool edge build. Actual
   GPU compute time is not in the bench. Closing this needs a Bevy-app
   bench harness that runs the render schedule headlessly.
2. **Parity tolerance relaxed.** Plan said ±1 distance unit per pixel on
   the software adapter. `parity.rs` runs with `TOLERANCE = 64` (≈1 px
   of signed distance at sdf_range=4) and `MAX_BAD_FRACTION = 0.08`.
   Root cause is the boundary halo above. Either the criterion needs to
   be rewritten to "no sign inversions outside an N-pixel boundary
   band," or the kernel needs to switch to fdsm-style scanline winding
   to match.
3. **No headless wgpu integration test.** `parity.rs` exercises the
   Rust port of the kernel math, not actual WGSL compilation +
   dispatch against a real adapter. Software-adapter parity (as
   originally specified) still needs writing.
4. **`readback.rs` is a stub.** Plan lists it as a Phase 1 deliverable.
   It exists but does nothing, so `dump_atlas_png` does not work under
   the GPU backend.
5. **Backend toggle never exercised end-to-end.** ~~The G-key toggle
   compiles and wires through `AtlasPreference` → `drive_atlas_swap`
   → `set_backend`, but the typography example has not been launched
   to confirm a GPU-rasterized glyph actually appears on screen.~~
   **Closed:** Launched the typography example, hit S then G; the
   GPU path successfully rasterizes and renders. Atlas swap completes;
   the renderer reads the GPU-written texture. Two real bugs surfaced
   from doing so (see Phase 1 — Open Issues below).
6. **Keybinding diverged from plan.** Plan proposes `B`; code uses `G`.
   Trivial doc fix above already records the rebinding; update plan if
   anything else assumes `B`.

**Deferred — Phase 1 gaps land in the later phase that needs them:**
- Gap 1 (real-GPU bench) → folded into Phase 2 prerequisites. The same
  Bevy-app bench harness that closes the Phase 1 5× claim is the only
  way to measure Phase 2's `warmup_burst/ebg_ascii_256_msdf` ≥ 10×
  acceptance, so the harness ships at the head of Phase 2.
- Gap 3 (headless wgpu integration test) → folded into Phase 2
  prerequisites. Phase 2's 3-glyph parity test cannot run without it,
  and it also retroactively satisfies Phase 1's software-adapter
  parity acceptance.
- Gap 4 (`readback.rs`) → folded into Phase 3. Phase 3's permanent CPU
  fallback for affected glyphs is what exercises readback; until then
  the stub is harmless because nothing calls it.
- Gap 5 (live end-to-end run) → not a plan deliverable; will happen
  the next time the typography example is launched. No code change
  needed.
- Gap 2 (parity tolerance) → dropped. Byte-for-byte parity with the
  CPU rasterizer is no longer a goal. The GPU path is allowed (and
  expected) to diverge from CPU where the GPU can do better — apex
  fidelity, thin strokes, boundary anti-aliasing. Parity tests
  become snapshot tests against the GPU's own golden output. See the
  Phase 1 and Phase 2 acceptance bullets for the rewritten test
  contract. The existing `parity.rs` Rust port of the kernel math
  stays useful as a regression smoke test (does the kernel still do
  *something* sensible?) but its tolerances are no longer load-relevant.
- Gap 6 (G-key keybinding) → already corrected in the Phase 1
  acceptance bullet above.

### Phase 1 Review

Outcome of the `/phase-review` cycle (Plan subagent returned 10
findings; user walked through the 3 that needed a decision):

**Applied to the plan without user gate (mechanical):**
- F2 — Phase 2 `edges.rs` line clarified: `edge_coloring_simple` lands
  *inside* the spawned worker task, not on the main thread.
- F4 — Phase 2 names `EDGE_CHANNEL_MASK_SHIFT = 2` and
  `EDGE_CHANNEL_MASK_BITS = 0b111` alongside the existing
  `EDGE_KIND_*` constants.
- F8 — Phase 1 acceptance corrected: `G` key, not `B`.
- F9 — Phase 3 names the `readback.rs` dependency for the CPU
  fallback path.
- F10 — Phase 2 file list adds `text/atlas_config.rs` to delete the
  `(Gpu, Msdf)` validation rejection.

**Decided by user, applied to the plan:**
- D1 (covers F1 + F6 + F7): no standalone "Phase 1.5." Each Phase 1
  acceptance gap was folded into the later phase that needs it —
  bench harness and headless wgpu test become Phase 2 prerequisites,
  `readback.rs` becomes a Phase 3 deliverable.
- D2 (covers F3): `GpuGlyphRequest` becomes an enum
  (`Sdf { … } | Msdf { … }`) instead of a struct with a runtime
  `distance_field` tag. The dispatcher matches on the variant. Wrong-
  pipeline dispatch becomes a compile error, not a runtime assert.
- D3 (covers F5): byte-for-byte parity with the CPU rasterizer is
  dropped as a goal. Parity tests become snapshot tests against the
  GPU's own golden output. The GPU path is allowed to diverge from
  CPU where the GPU can do better. Long-term: the CPU path retires
  once the GPU is "good enough and a lot faster."

**Rejected:** none.

### Phase 1 — Architecture rewrite (per-atlas async plumbing)

The original Phase 1 design routed GPU requests/completions through
**shared world resources** (one `GpuGlyphRequestSender`, one
`GpuGlyphCompletionBuffer`, one `RenderAtlasPages` resource). That
design cannot survive a parallel-atlas swap: when the old atlas
drops, in-flight worker tasks still send into the shared channels,
their completions arrive after `complete_swap()`, and the
`finalize_gpu_completion` observer applies them to the new active
atlas with the old atlas's coordinates — silent corruption.

A first patch added a monotonic `AtlasGeneration(u64)` tag to every
request and completion, dropping mismatches at the observer. That
patch fixed GPU→CPU swap-back but **broke CPU→GPU**: requests issued
against `pending` arrived back at an observer that consulted
`slot.active_mut()` (which is the *old* active during a swap), so
generations never matched, completions were dropped, and `pending`
never finished warming. Swap hung forever.

The fix (designed via Codex consultation, implemented end-to-end):
**each `GlyphAtlas` owns its own GPU pipe.** Ownership replaces id
checks; Rust drop semantics enforce the invariant.

- Added `AtlasGpuPipe { built_tx, built_rx, completions:
  GpuCompletionSink, pending_dispatch: Vec<GpuRenderJob> }` to
  `GlyphAtlas` (lazy-initialised when a `GpuGlyphDispatcher` is
  installed).
- Workers spawned by an atlas clone *that atlas's* `built_tx`. When
  the atlas drops, sends become silent `SendError`.
- Render jobs carry the exact `Handle<Image>` of the target page
  plus a clone of *that atlas's* `GpuCompletionSink`. The render-
  world dispatcher writes completions directly into the job's sink.
  No global "current pages" resource.
- Completion **application** moved into `GlyphAtlas::poll_gpu`,
  called from `AtlasSlot::poll_async_glyphs`, which already polls
  both `active` and `pending` during swap. `pending` drains itself.
- Deleted: `AtlasGeneration`, `GpuGlyphRequestSender`,
  `GpuGlyphRequestReceiver`, `GpuGlyphCompletionBuffer`,
  `RenderAtlasPages`, `GpuGlyphCompleted` event,
  `finalize_gpu_completion` observer, the
  `clear_main_request_queue` / `drain_request_channel` /
  `drain_gpu_completions` / `sync_render_atlas_pages` systems.
- Added: `GpuRenderJobExtract` (main→render ExtractResource that
  Codex's `collect_gpu_render_jobs` system fills each frame by
  draining every atlas's `pending_dispatch`), `GpuRenderJobQueue`
  (render-world persistent queue).
- `reconcile.rs::poll_atlas_glyphs` now `sync_to_gpu`s dirty pages
  **before** `poll_async_glyphs_stats` so when an atlas builds a
  `GpuRenderJob`, the page's `image_handle` is `Some`.

Verified live (typography example, S→G→G round trip):
- CPU→GPU swap completes in ~400ms.
- GPU→CPU swap completes in ~2s.
- No swap-back corruption.
- Initial GPU render of "Typography" is correct.

### Phase 1 — Open Issues

**O1, O2, O3 — closed.**
- O1 (compute kernel over-dispatch) — fixed by per-glyph dispatch in
  `dispatch.rs::encode_image`.
- O2 (heavy default config) — applied: defaults are
  `RasterQuality::Small` + `DistanceField::Sdf`.
- O3 (G chip highlight) — fixed by rewiring the M/S/G chips to
  observe `AtlasSlot.active()` instead of `AtlasPreference`. The
  chip now lights only after the swap completes (the truth-source
  semantic the user wanted). Plus a backend-toggle `info!` log
  line gives a deterministic textual signal.

**O4. Incremental glyph adds under GPU produce only ~1 character
(closed 2026-05-18).**

- *Root cause:* `GlyphAtlas::allocate_gpu_region` unconditionally
  marked the page `AtlasPageState::Dirty`. On the next frame,
  `reconcile.rs::poll_atlas_glyphs` saw `total_dirty_page_count >
  0` and called `sync_to_gpu`, which executes
  `image.data = page.pixels.clone()` for any page that already has
  an `image_handle`. In GPU mode `page.pixels` is the empty CPU
  mirror — the GPU compute pass writes directly to the storage
  texture, not to `page.pixels` — so the blit wiped every
  previously dispatched GPU glyph on that page. The only glyphs
  visible after a cycle were the ones the compute pass wrote that
  same frame, which was usually just the one new glyph the cycled
  word added (because most other glyphs were already cached and
  didn't dispatch again).
- *Fix:* `allocate_gpu_region` now marks `Dirty` only when the page
  has no `image_handle` yet (genuinely new page that needs
  `sync_to_gpu` to allocate its `Image`). Pages that already have a
  handle keep their state alone, so subsequent syncs don't overwrite
  the GPU-rasterized texels. One-block change in
  `crates/bevy_diegetic/src/text/atlas.rs` around line 880.
- *Why warm-up worked but incremental didn't:* during the swap
  warm-up, every allocation happens in one synchronous burst on
  `shape_panel_text_children`'s first frame after the swap. The
  first `sync_to_gpu` on the next frame blits empty pixels over an
  empty texture (no-op), then the compute pass writes glyphs into
  the now-allocated texture. No later allocation re-marks the page
  Dirty in that same window, so no later sync wipes anything.
  Incremental adds are a different story: a stable page already has
  GPU texels, then a fresh allocation Dirties it, and the next sync
  wipes the existing texels before the new dispatch even runs.
- *Verified:* typography example, GPU mode, cycle through Ångström,
  fjord, "EB Garamond Test", and the small description labels —
  every word renders in full, and the surrounding panels (CONTROLS,
  FONTS, QUALITY, CAMERA, anatomy diagram labels) stay intact
  across cycles.

**O5. GPU glyph render quality is visibly worse than CPU
(noted but lower priority).**

- *Symptom:* "GPU versions look bad right now" — initial render is
  structurally correct (no garbage, correct character identity) but
  visibly lower fidelity than the CPU rasterizer's MSDF/SDF output.
  Edges look softer, possibly with banding or quantisation
  artifacts.
- *Probable causes:* the SDF kernel's per-pixel distance estimate
  (`distance_quadratic`/`distance_cubic` using a 5-seed Newton
  refinement with `NEWTON_ITER = 4`) is less accurate than fdsm's
  swept-region search; the winding subdivision (8 segments for
  quadratic, 12 for cubic) is coarse; and the `sdf_range` is
  passed through whole but the kernel's normalisation may not
  match fdsm's exactly.
- *When to address:* after O4 lands. The user is on board with GPU
  diverging from CPU as long as it's "good enough and a lot
  faster" — define "good enough" by snapshot comparison once the
  Phase 2 bench/snapshot harness exists.

**Phase 1 close-out checklist** (revised):

- [x] Per-atlas async plumbing rewrite (replaces O1/O3 architecture
      issues at the root).
- [x] O1 — kernel per-glyph dispatch.
- [x] O2 — defaults are Small + Sdf.
- [x] O3 — chips wired to `AtlasSlot.active()`.
- [x] Both swap directions verified clean.
- [x] **O4** — incremental glyph adds under GPU land correctly
      (page-Dirty fix in `allocate_gpu_region`).
- [ ] **O5** — improve render quality (or commit to "good enough"
      with a snapshot baseline).
- [ ] Confirm `(Msdf, Gpu)` combo handled gracefully. Validator
      currently warns but doesn't downgrade; runtime toggle to
      `(Msdf, Gpu)` routes through the SDF kernel and writes a
      grayscale SDF into the MSDF atlas, which the MSDF shader
      reads as a degraded SDF render. Either: (a) validator
      downgrades at startup AND runtime toggle clamps, OR (b) the
      G chip disables when M is active.

### Phase 1 — Resumption Context (post-compact handoff)

For a fresh-context Claude picking up after compaction. Skim before
resuming.

**Branch:** `feat/gpu-rasterizer`. Uncommitted changes only — nothing
has been committed for this work yet. The user is handling unrelated
`docs/*` deletions in a separate session — leave doc files alone
except `docs/bevy_diegetic/gpu_rasterizer.md` (this doc).

**O4 status (2026-05-18): closed.** The page-Dirty fix in
`allocate_gpu_region` resolved incremental glyph adds. See O4 entry
under "Phase 1 — Open Issues" for the root cause writeup. Do NOT
revert that conditional — it is the entire fix.

**Last big change (don't redesign — extend it):** the per-atlas
async plumbing rewrite is in place. Each `GlyphAtlas` owns its own
`AtlasGpuPipe` (mpsc channels + a cloneable `GpuCompletionSink` +
`pending_dispatch: Vec<GpuRenderJob>`). Workers send via the
atlas's own `built_tx`. Render jobs carry `Handle<Image>` and a
clone of the atlas's completion sink. `GlyphAtlas::poll_gpu`
(called from `poll_async_glyphs_stats`) drains the atlas's own
pipe; `AtlasSlot::poll_async_glyphs` polls both `active` and
`pending`, so the pending atlas self-drains during swap. Generation
tag is deleted. Observer is deleted. `RenderAtlasPages` is deleted.

**Live state after rewrite:** Typography example launches; press G
to swap to GPU (~400ms to complete); initial "Typography" renders
correctly; press G again to swap back to CPU cleanly. **No swap-back
corruption.** Both directions of the swap work.

**What works (do not re-debug):**
- Atlas swap state machine (both directions, both backends).
- Per-atlas channels: worker → built_tx → atlas's own poll
  → GpuRenderJob with `Handle<Image>` + sink clone.
- Render-world dispatcher reads the job's `image_handle` directly
  (no global page-handle resource).
- Completions land in the atlas's own sink and are applied by
  the atlas's `poll_gpu`.
- Pipeline cache compiles `sdf_gen.wgsl`.
- GPU writes land in the atlas texture and the renderer reads them
  (verified by initial "Typography" rendering correctly).
- Per-glyph workgroup dispatch (no over-dispatch corruption).
- Chip wiring (M/S/G) reflects `AtlasSlot.active()`, lights only
  after the swap completes.
- Backend toggle log line: `backend toggled → {Cpu|Gpu}` fires on
  every G keypress.
- **Incremental glyph adds under GPU mode (O4)**: cycle ←/→,
  panel updates, camera updates all render in full without
  wiping previously-cached glyphs. Verified with Ångström,
  fjord, EB Garamond Test and all anatomy diagram labels.

**What doesn't work yet (in priority order):**
- **O5 — GPU render quality is visibly worse than CPU.** Initial
  render is structurally correct (no garbage, correct character
  identity) but visibly lower fidelity than CPU MSDF/SDF output.
  Edges look softer, possibly banded. Probable causes documented
  in the O5 entry under "Phase 1 — Open Issues".
- `(Msdf, Gpu)` validator-vs-toggle inconsistency — still warns,
  doesn't downgrade.

**File map for the GPU rasterizer** (current state on
`feat/gpu-rasterizer`):
- `crates/bevy_diegetic/src/text/gpu_rasterizer/mod.rs` — plugin
  wiring, stateless `ChannelGpuDispatcher`, `enqueue_gpu_glyph`
  public entry, `enqueue_on_atlas` (internal). NO observer.
- `crates/bevy_diegetic/src/text/gpu_rasterizer/pipeline.rs` —
  `GpuRasterizerPipeline` resource + SDF pipeline only. Unchanged.
- `crates/bevy_diegetic/src/text/gpu_rasterizer/dispatch.rs` —
  `dispatch_glyph_compute` (render-world system),
  `partition_by_image` (groups jobs by `Handle<Image>`),
  `encode_image` (per-glyph dispatch). Re-queues jobs into the
  render-side `GpuRenderJobQueue.pending` when
  `gpu_images.get(handle)` returns `None`.
- `crates/bevy_diegetic/src/text/gpu_rasterizer/edges.rs` —
  outline → edge-buffer conversion; `glyph_bitmap_size` helper.
  `GpuGlyphRequestBody` and `EdgeSegment` are `pub(crate)`.
- `crates/bevy_diegetic/src/text/gpu_rasterizer/extract.rs` —
  ONE system left: `collect_gpu_render_jobs` (drains each atlas's
  `pending_dispatch` into `GpuRenderJobExtract`).
- `crates/bevy_diegetic/src/text/gpu_rasterizer/request.rs` —
  `AtlasGpuPipe`, `BuiltGpuRequest::{Built, Invisible}`,
  `GpuRenderJob` (carries `image_handle` + `completions` sink),
  `GpuCompletionSink`, `GpuRenderJobExtract` (main→render),
  `GpuRenderJobQueue` (render-world). `GpuGlyphRequest` is still a
  struct with a runtime `distance_field` tag (Phase 2 plans to
  refactor into an enum per D2).
- `crates/bevy_diegetic/src/text/gpu_rasterizer/parity.rs` — Rust
  port of WGSL kernel for smoke testing.
- `crates/bevy_diegetic/src/text/gpu_rasterizer/readback.rs` — STUB
  (Phase 3 deliverable).
- `crates/bevy_diegetic/src/text/gpu_rasterizer/shaders/sdf_gen.wgsl`
  — the kernel. Per-glyph dispatch already in place.

**Files outside the rasterizer touched in Phase 1:**
- `crates/bevy_diegetic/src/text/atlas.rs` — `RasterBackend` field,
  `GpuGlyphDispatcher` trait, `set_backend` / `set_gpu_dispatcher`,
  `gpu_dispatcher_handle`, `lookup_or_queue` GPU branch, NEW
  `gpu_pipe: Option<AtlasGpuPipe>` field with `ensure_gpu_pipe`,
  `gpu_pipe_handles`, `drain_gpu_render_jobs`, `poll_gpu`.
- `crates/bevy_diegetic/src/text/atlas_config.rs` — `backend` field,
  defaults are `RasterQuality::Small` + `DistanceField::Sdf`,
  `validate()` rejects `(Gpu, Msdf)` (warning only).
- `crates/bevy_diegetic/src/text/atlas_slot.rs` — `AtlasPreference`
  gained `backend: RasterBackend`; new helpers
  `total_dirty_page_count` and `drain_gpu_render_jobs` (drains both
  halves during swap).
- `crates/bevy_diegetic/src/text/mod.rs` — `drive_atlas_swap`
  includes backend in trigger comparison; pending inherits
  dispatcher handle from active.
- `crates/bevy_diegetic/src/text/msdf_rasterizer/mod.rs` —
  `DistanceField::Sdf` is now the `#[default]`.
- `crates/bevy_diegetic/src/render/text_renderer/reconcile.rs` —
  `poll_atlas_glyphs` syncs dirty pages BEFORE polling so
  `GpuRenderJob` builds find the page's `image_handle` populated.
- `crates/bevy_diegetic/examples/typography.rs` — `G` key toggle
  with `info!` log; M/S/G chips wired to `AtlasSlot.active()` not
  `AtlasPreference`; `log_steady_state_perf` was gutted to a no-op
  during prior debugging.
- `crates/bevy_diegetic/src/lib.rs` and
  `crates/bevy_diegetic/src/text/mod.rs` — removed exports for
  deleted shared types (`GpuGlyphCompleted`,
  `GpuGlyphRequestQueue`, `GpuGlyphRequestSender`).

**How to smoke-test the full pipeline (post-O4):**
1. `cargo build -p bevy_diegetic --example typography`
2. Launch (BRP MCP `brp_launch typography`). Initial render shows
   "Typography" cleanly under CPU.
3. Press G. ~400ms later the swap completes; "Typography" still
   renders cleanly under GPU.
4. Press → (ArrowRight) several times to cycle through specimen
   words. Each cycled word (Ångström, fjord, etc.) should render
   in full, and the surrounding panels (CONTROLS, FONTS, QUALITY,
   CAMERA, anatomy diagram labels) should stay intact.
5. Press G again. Swap back to CPU; cycling still works.

**O5 next steps (when starting work):**
- The SDF kernel in `crates/bevy_diegetic/src/text/gpu_rasterizer/
  shaders/sdf_gen.wgsl` uses a per-pixel distance estimate with a
  5-seed Newton refinement (`NEWTON_ITER = 4`) and subdivides
  quadratic curves into 8 segments / cubic into 12 for winding.
  Likely sources of quality loss: (a) seed count + iteration count
  too low for thin strokes, (b) `sdf_range` normalisation may not
  match fdsm's, (c) coarse winding subdivision missing thin
  segments at apexes.
- Before touching the kernel, run the typography example side by
  side with CPU and GPU on the same specimen and screenshot the
  difference — having concrete before/after images helps judge
  whether kernel tweaks moved fidelity in the right direction.
- The parity test at `gpu_rasterizer/parity.rs` is the existing
  comparison harness; extending it with snapshot diffs is the
  natural next move.

**Don't:**
- Re-add the generation tag. It was rejected for cause.
- Re-introduce a shared global page handle resource.
- Add a `finalize_gpu_completion` observer that reaches into
  `slot.active_mut()`. That was the original bug.
- Revert the `image_handle.is_none()` guard around the
  `state = Dirty` assignment in `allocate_gpu_region`. That guard
  IS the O4 fix; removing it brings back the texture-wipe bug.
- Treat the chip not lighting as a wiring bug — `wire_chip_to_state`
  works; the source of truth was wrong before (preference vs slot
  state).

### Phase 2 — MSDF on GPU

**Prerequisites (deferred from Phase 1, must ship at the head of
Phase 2):**
- Bevy-app bench harness: `LazyLock<App>` with `RenderPlugin` and
  `synchronous_pipeline_compilation: true` (already spec'd under
  "Bench device initialization" below). Closes Phase 1's
  `warmup_burst/jbm_ascii_128_sdf` ≥ 5× acceptance and is the harness
  Phase 2's MSDF bench bullet reuses.
- Headless wgpu integration test: compile `sdf_gen.wgsl` against the
  software adapter, dispatch a single glyph, read the result back,
  hash it, and check against a committed snapshot hash. First-run
  produces the snapshot; subsequent runs catch unintended kernel
  changes. Closes Phase 1's snapshot-test acceptance and is the
  scaffold the Phase 2 3-glyph MSDF snapshot test extends.

Files added:
- `gpu_rasterizer/shaders/msdf_gen.wgsl`

Files edited:
- `gpu_rasterizer/request.rs` — convert `GpuGlyphRequest` from a
  struct with a runtime `distance_field: DistanceField` tag into an
  enum with `Sdf { … }` and `Msdf { channel_masks: …, … }` variants.
  Common fields (`key`, `bitmap_size`, `bearing`, `atlas_origin`,
  `page_index`) stay shared in each variant or extract to an inner
  `Common` struct. SDF-only and MSDF-only fields live only on their
  variant — the MSDF channel-mask data cannot accidentally appear on
  an SDF request, and vice versa. `BuiltRequest::Built(Box<…>)` and
  the mpsc channel sender carry the enum. The runtime
  `distance_field` field is deleted; the variant tag replaces it.
- `gpu_rasterizer/edges.rs` — call fdsm's `edge_coloring_simple`
  *inside* the spawned worker task (the Phase 1 async closure that
  builds `BuiltRequest::Built`), pack the channel mask into the
  `kind` field of `EdgeSegment`. Add `EDGE_CHANNEL_MASK_SHIFT: u32 =
  2` and `EDGE_CHANNEL_MASK_BITS: u32 = 0b111` next to the existing
  `EDGE_KIND_*` constants; reuse them in `sdf_gen.wgsl` /
  `msdf_gen.wgsl` for the unpack. The MSDF arm of the worker task
  constructs `GpuGlyphRequest::Msdf`; the SDF arm constructs
  `GpuGlyphRequest::Sdf`.
- `gpu_rasterizer/pipeline.rs` — second pipeline variant for MSDF; or
  shader pipeline-constant for SDF vs MSDF dispatch. The pipeline
  resource holds both compiled pipelines; the dispatcher picks via
  the request variant `match`.
- `gpu_rasterizer/dispatch.rs` — `dispatch_glyph_compute` matches on
  the `GpuGlyphRequest` variant: `Sdf { … }` arm calls the SDF
  pipeline, `Msdf { … }` arm calls the MSDF pipeline. The compiler
  refuses to let an MSDF request reach the SDF pipeline (or vice
  versa) — mixed-mode kernel dispatch is structurally impossible
  rather than a runtime assert. Per-page grouping still happens after
  the variant match (group `Vec<SdfReq>` by page, then group
  `Vec<MsdfReq>` by page).
- `text/atlas_config.rs` — delete the `(Gpu, Msdf)` rejection branch
  in `AtlasConfig::validate` (added in Phase 1 as a Phase-2 gate).

Acceptance:
- `warmup_burst/ebg_ascii_256_msdf` GPU backend ≥ 10× faster than CPU.
- 3-glyph snapshot test (A, W, V across both fonts) on the software
  adapter: the GPU-rendered MSDF page is its own golden image. The
  EB Garamond `V` apex is reviewed visually for corner sharpness; if
  the GPU MSDF beats the CPU MSDF on apex preservation, that is a
  win, not a parity failure.
- `edge_coloring_matches_cpu` unit test: calls `edge_coloring_simple`
  on a known glyph outline through both the CPU and GPU edge-buffer
  builders and asserts channel masks match exactly. (Edge-coloring
  is a deterministic graph-partition algorithm — it must match.
  Distance-field output need not.)
- The `examples/msdf_font_audit.rs` diagnostic tool (see
  "Diagnostics") triages font-specific MSDF artifact reports
  post-launch.

### Phase 3 — MSDF error correction

Built when a customer report of MSDF artifacts shows error-correction
would resolve them. The compute pass scans for pixels where the
channel-median disagrees with the true distance and patches them. The
CPU fallback covers affected glyphs in the interim.

The CPU fallback stays permanently. Memory cost is a `Vec<u8>` mirror
per atlas page (~4 MB per 1024² page; typically 1–3 pages).

**Phase 3 also ships `gpu_rasterizer/readback.rs` (deferred from
Phase 1)** — `copy_texture_to_buffer` + `map_async` mirror of the GPU
atlas page back into a `Vec<u8>`. The Phase 1 stub is harmless until
something calls it; Phase 3's permanent CPU fallback is the first
caller. The error-correction compute pass itself runs in-place on the
GPU page texture and does not need readback.

## Testing strategy

### Unit tests

- `gpu_rasterizer/edges.rs::tests` — given a known glyph outline,
  `build_edge_buffer` produces the expected `EdgeSegment` array (count,
  control point values, channel masks for MSDF).
- `gpu_rasterizer/pipeline.rs::tests` — pipeline initializes without
  error against the test wgpu device.

### Parity tests

Headless wgpu test (`pollster` block-on, **software adapter**):
rasterize 'A', 'W', and 'V' (from EB Garamond) through both CPU and GPU
paths at the same canonical size, compare pixel arrays. Tolerance: ±1
on signed-distance values (quantization floor). Lives in
`gpu_rasterizer/parity.rs`, parallel to the existing
`msdf_rasterizer/parity.rs`.

The software adapter is deterministic and byte-matches CPU output.
Real-hardware GPU drift is described under "Limitations → GPU vendor
drift".

### Bench coverage

Extend `benches/glyph_rasterization.rs` with a `backend` axis:

```rust
const BACKENDS: &[(&str, RasterBackend)] = &[
    ("cpu", RasterBackend::Cpu),
    ("gpu", RasterBackend::Gpu),
];
```

Each `WarmupCase` runs twice. The bench already supports the criterion
baseline workflow, so a single `cargo bench --baseline before` shows
the delta per config.

GPU benches require an active wgpu device; the bench harness will need
a once-per-process wgpu instance setup (use Bevy's
`bevy::render::RenderDevice` resource extraction in a minimal
`MinimalPlugins`-style app, captured into a `OnceLock`).

### Visual regression

The typography example already supports A/B comparison via the M/S
(MSDF/SDF) toggle. Add a B (backend) toggle for CPU/GPU. Visual
inspection on the FONTS panel at canonical 128 and 256 across both fonts.

## Synchronization

A glyph is marked complete when `queue.submit()` returns for its
compute pass. The compute-pass write into the atlas page storage
texture becomes visible to subsequent render passes via wgpu's
documented command-buffer ordering — no explicit fence, no
`map_async` wait, no one-frame defer. The CPU mirror readback (see
`readback.rs`) is queued in the same submission and resolves
asynchronously one frame later.

The parity test (`parity.rs`) covers the rasterize-then-immediately-
sample case. If a vendor-specific barrier bug surfaces (Apple Silicon
has the strictest synchronization model), insert an explicit `Barrier`
between the compute pass and the first sampling render pass.

`in_flight_count` semantics by backend:
- **CPU**: includes glyphs currently rasterizing on worker threads.
- **GPU**: includes glyphs whose request is queued, dispatched, or
  awaiting the extract-back completion event.

## Backend mismatch on swap

When the user toggles backend mid-render, the existing
`AtlasSlot::Swapping` code constructs a new pending atlas with the new
config and waits for it to warm up before swapping. Both atlases own
their own texture, both use `Rgba8Unorm`, and the swap completion
check (`pending.in_flight_count() == 0`) applies uniformly. The text
shader reads only the distance channel(s) from the active atlas;
cross-format sampling does not occur even during the swap window.

The pool-sharing argument (`Some(active.worker_pool())`) remains
valid because both backends use the pool for prep work (CPU: full
rasterization; GPU: edge-buffer build).

## Limitations

The following are accepted limits of the design, not bugs.

### GPU device loss

If the OS terminates the GPU context (driver crash, power management,
resource exhaustion), in-flight glyph dispatches are lost. The atlas
has no detection or recovery — affected glyphs stay in `in_flight`
until the app restarts. A side effect: an atlas swap that started
before the loss never completes (the swap completion check
`pending.in_flight_count() == 0` never fires), so the swap remains
mid-flight indefinitely. Apps that need to survive device loss use
`RasterBackend::Cpu`.

### Per-glyph dispatch timeout

A compute dispatch on a heavily loaded GPU runs to completion or
hangs. The atlas has no per-glyph timeout. `GpuGlyphBudget.per_frame`
bounds per-frame work so pathological single-glyph stalls do not
freeze the app, but the affected glyph itself remains pending.

### Unbounded request queue

`RenderGlyphQueue.pending` has no hard cap. Sustained enqueue rates
above the per-frame budget grow the queue without bound (4096 pending
triggers a warning log). Apps that enqueue thousands of glyphs at
runtime must pre-warm during loading or raise `GpuGlyphBudget`.

### GPU vendor drift

Different GPU vendors (NVIDIA, AMD, Apple Silicon, Intel) implement
floating-point math slightly differently. The same glyph rasterized
on two vendors may produce distance values that differ by 1–3
quantization units — minor edge softness differences at extreme zoom,
rarely perceptible at normal text sizes. The parity test in
`parity.rs` uses wgpu's software adapter (deterministic, byte-matches
CPU); real-hardware variance is not CI-tested. Vendor-specific
reports reproduce via `examples/msdf_font_audit.rs`.

### Synchronization barrier on Apple Silicon

The design trusts wgpu's documented command-buffer ordering to make
compute-pass writes visible to subsequent sampling passes without an
explicit barrier. Apple Silicon (Metal) has the strictest
synchronization model and is the most likely candidate for a barrier
bug. If real-hardware testing surfaces stale or inverted samples on
Apple Silicon, an explicit `Barrier` between compute and the first
sampling render pass resolves it. The parity test (software adapter)
does not catch this class of bug.

## Diagnostics

### `examples/msdf_font_audit.rs`

Standalone binary that loads a curated font set, rasterizes every BMP
codepoint through CPU MSDF and GPU MSDF, and reports per-glyph
disagreement counts (pixels where the GPU median would mis-classify
relative to CPU). Lands alongside Phase 2. Used to triage reports of
MSDF artifacts on specific fonts.

## wgpu limits validation

At `GpuRasterizerPlugin::build`, after `RenderApp` is reachable, query
`RenderDevice::limits()` and validate:

- `limits.max_storage_buffer_binding_size` ≥ the worst-case edge
  buffer (estimate: 2000 edges × 36 bytes ≈ 72 KB — trivially under
  any desktop/console limit; mobile may be 64 MB).
- `limits.max_storage_buffers_per_shader_stage` ≥ 3 (edges, glyphs,
  output texture).
- `limits.max_compute_workgroup_size_x` ≥ 8 and `_y` ≥ 8.
- Storage-texture write access for `Rgba8Unorm`. Read/write storage
  bindings are core wgpu (no `Features` flag needed); per-format support
  is queried via `adapter.get_texture_format_features(TextureFormat::Rgba8Unorm)`
  and the returned `TextureFormatFeatureFlags` must contain
  `STORAGE_READ_WRITE` (or `STORAGE_ATOMIC` is unneeded — write-only is
  sufficient). Confirm the exact flag name during Phase 1 setup.

On any failed check: log a warning, do not insert the dispatch
system, force any GPU-backed atlas to fall back to CPU. Same path as
the WebGL2 / mobile fallback.

## Pipeline parameters

The wgpu compute pipeline is built once per process at plugin init.
Per-atlas runtime parameters (`sdf_range`, etc.) pass through the
uniform buffer (`RasterParams`), not as shader constants, so the
pipeline stays single-instance and never re-compiles.

## Bench device initialization

The GPU bench (`benches/glyph_rasterization.rs`, extended) shares one
wgpu device across iterations via a `LazyLock<App>` that adds
`bevy::render::RenderPlugin { render_creation: RenderCreation::Automatic(WgpuSettings::default()), synchronous_pipeline_compilation: true }`
on top of `MinimalPlugins`. `MinimalPlugins` alone does not include
rendering; combined setup is ~10 ms one-time and amortized across
all iterations.

## Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| WGSL distance algorithm has subtle bug vs CPU reference | High | Parity test against software adapter (deterministic); byte-level diff; fix iteratively. SDF first catches most bugs before MSDF lands. |
| `Image` asset upload doesn't propagate `STORAGE_BINDING` usage | Medium | Phase 1 spike confirms; if it doesn't, use a custom wgpu texture init path bypassing the asset loader. |
| Frame budget default (16) is wrong | Medium | Tune after bench data. User-overridable via `GpuGlyphBudget.per_frame`. Tuning table in "Frame budget management". |
| Bench environment can't initialize wgpu device cleanly | Low-medium | Pre-build a shared device + `RenderPlugin` in a `LazyLock`. CI runners with no GPU fall back to software adapter. |
| WebGL2 / mobile backend lacks compute support | Certain on those backends | Detect at plugin init via `RenderDevice::features()`, force `RasterBackend::Cpu` and log warning. Validate `AtlasConfig.backend` at config-creation time so the downgrade happens before any glyph is queued. |
| MSDF error-correction artifacts appear on real fonts | Low (unmeasured) | Phase 3 adds the correction pass reactively. CPU fallback covers affected glyphs in the interim. `examples/msdf_font_audit.rs` triages on demand. |
| GPU device loss strands in-flight glyphs | Low (vendor-dependent) | Phase 1 accepts this; see "Known limitations → GPU device loss". |
| Single-glyph runaway compute stalls forever | Very low | Phase 1 accepts; see "Known limitations → Per-glyph dispatch timeout". |
| GPU vendor floating-point drift breaks parity test | Medium | Software-adapter parity test in CI; real-hardware drift documented and accepted. See "Known limitations → GPU vendor drift". |
| Subtle wgpu barrier bug on Apple Silicon | Low-medium | Parity test catches if compute-pass writes aren't visible to subsequent sampling. If real failure appears, add explicit `Barrier` between compute pass and sampling. |

## Relationship to other docs

- `sdf_text.md` — describes the SDF/MSDF distance-field axis. This doc
  adds the orthogonal CPU/GPU axis.
- `roadmap/` — none of the in-flight roadmap items conflict; the GPU
  rasterizer is purely additive.

# GPU glyph rasterizer

## Status

GPU SDF rasterization on `feat/gpu-rasterizer`. End-to-end pipeline
works: typography example launches; the `G` key swaps between CPU and
GPU rasterizer backends (~400 ms swap); GPU initial render is
visually correct and substantially faster than CPU. Closed-form
analytic distance for quadratic beziers + curvature-corrected Newton
with iteration-min tracking for cubics brings GPU edge quality to
near-CPU parity; remaining differences are sub-grayscale-level edge
speckle from algorithmic divergence with fdsm. Glyph metrics carry
the font-defined bearing and the atlas-specific bitmap pad as
separate fields, so on-screen text position is invariant under
canonical-size changes — a prerequisite for the per-element atlas
binding plan in Phase 2.5.

Active work queue. **"Continue to the next phase" = work the
next unfinished item in this list, top-to-bottom.** Phase 2
stays at #1 until its acceptance work is done, then Phase 2.1
starts.

1. **Phase 2 — MSDF on GPU.** Code landed: enum-tagged
   `GpuGlyphRequest::{Sdf,Msdf}`, edge-coloring in
   `build_edge_buffer`, `msdf_gen.wgsl` with signed-pseudo-distance
   and winding-rule sign reconciliation, second cached compute
   pipeline. `edge_coloring_matches_cpu` parity test passes.
   **Acceptance work remaining (do these in order):**
   a. CPU/GPU visual comparison via the G key toggle in
      `examples/typography.rs` — switch atlas config to `(Gpu,
      Msdf)`, take screenshots before/after toggling G,
      pixel-diff in the Typography text region, confirm GPU
      result is visually clean and diff concentrates on letter
      edges.
   b. 3-glyph software-adapter snapshot test (A, W, V across
      JetBrains Mono and EB Garamond). GPU-rendered MSDF page is
      its own golden image.
   c. Build the wgpu-device-backed bench harness (`LazyLock<App>`
      + `RenderPlugin` per "Bench device initialization"), delete
      the SDF gate in `bench_gpu_main_thread`, confirm
      `warmup_burst/ebg_ascii_256_msdf` GPU ≥ 10× faster than CPU.
2. **Phase 2.1 — GPU-only quality exploration.** Candidate tweaks
   that fdsm doesn't or can't do (supersampling, in-shader error
   correction, tangent pseudo-distance, hybrid f32/f64). Each one
   measured against the Phase 2 baseline; the ones that move the
   needle land, the ones that don't get dropped.
3. **Phase 2.5 — Per-element atlas binding.** Replace the single
   global `GlyphAtlas` with a `HashMap<RasterQuality, GlyphAtlas>`
   so a panel of small overlay text and a large headline can each
   pick a canonical size appropriate to their display size.
4. **Phase 3 — MTSDF + text effects.** A-channel true distance for
   outlines, shadows, glows, bevels. Generator side is a one-line
   `#ifdef MTSDF` in `msdf_gen.wgsl` plus a third cached pipeline;
   the consumer-side effects pipeline (parameter components, shader
   uniforms, fragment-shader extension) is the actual remaining
   work.
5. **Phase 4 — Retire the runtime CPU rasterizer.** Leave it
   `#[cfg(test)]` for the parity test reference; remove the
   `RasterBackend` enum and the CPU/GPU swap branch. The CPU path
   and the G key toggle stay through Phases 2 / 2.1 / 2.5 / 3 so
   the CPU/GPU comparison stays available throughout.

## Motivation

The benchmark `crates/bevy_diegetic/benches/glyph_rasterization.rs`
established baseline per-glyph rasterization cost on the CPU path:

| Workload | Wall time (8 threads, MSDF) |
|---|---|
| JetBrains Mono ASCII-94 @ canonical 128 | 90 ms |
| JetBrains Mono ASCII-94 @ canonical 256 | 285 ms |
| EB Garamond ASCII-94 @ canonical 128 | 248 ms |
| EB Garamond ASCII-94 @ canonical 256 | **838 ms** |

The EB Garamond `V` at canonical 256 alone takes ~75 ms on a single
worker thread. The cost is dominated by fdsm's per-pixel
nearest-edge search inside `generate_msdf`, which is O(pixels ×
edges) per glyph.

The Rust SIMD ecosystem has not produced a vectorized MSDF
generator. The CPU lever space is exhausted: caching `Face::parse`
saves microseconds; reusing buffer allocations saves <1 ms; adding
worker threads past 8 hits diminishing returns.

GPU compute is the next workable lever. Each output pixel is
independent; the nearest-edge search vectorizes naturally across
thousands of GPU threads. Bevy ships with `wgpu` already wired
through every render pipeline in the crate, so this requires no new
dependency and no FFI.

Expected wins (now empirically confirmed on the SDF path):

- 10–100× per-glyph speedup for SDF.
- 5–50× per-glyph speedup for MSDF (the extra cost is the median /
  edge-coloring logic in the shader).
- Zero CPU → GPU atlas upload — the compute pass writes the bytes
  directly into the atlas page's storage texture, eliminating the
  per-page `texture_to_buffer` upload the dirty-page mechanism would
  otherwise perform.

## Non-goals

- Supporting devices without compute-shader storage textures. The
  crate targets desktop and modern mobile; WebGL2 / older mobile is
  out of scope. If the validator detects an unsupported device the
  app logs a warning and text rendering will not work — there is no
  fallback path.
- Per-glyph backend selection within one atlas. An atlas is fully
  CPU or fully GPU; mixing within a single atlas would require dual
  upload paths and dual ready-state tracking for no real benefit.
  Will become a non-issue when Phase 4 retires the runtime CPU
  path.
- Byte-for-byte matching with the CPU fdsm rasterizer. The GPU path
  is allowed (and encouraged) to diverge where it can do better —
  apex fidelity, thin strokes, fractional boundary coverage. The
  bar is "good enough and a lot faster."

## Architecture

### Two orthogonal axes

| Axis | Variants | Where it lives |
|---|---|---|
| Distance-field encoding | `DistanceField::Sdf` / `Msdf` (Phase 3 adds `Mtsdf`) | `text/msdf_rasterizer::DistanceField` |
| Rasterizer backend | `RasterBackend::Cpu` / `Gpu` (Phase 4 removes runtime Cpu) | `text/atlas_config::RasterBackend` |

Both axes are atlas-level. An atlas is `(distance_field, backend)`
end-to-end; mixing within an atlas is rejected at config time.

### Per-atlas async plumbing

Each `GlyphAtlas` owns its GPU pipe:

```
GlyphAtlas
├── gpu_pipe: Option<AtlasGpuPipe>
│   ├── built_tx:   mpsc::Sender<BuiltGpuRequest>      // worker → main
│   ├── built_rx:   mpsc::Receiver<BuiltGpuRequest>
│   ├── completions: GpuCompletionSink                  // render → main
│   └── pending_dispatch: Vec<GpuRenderJob>             // main → render
```

Workers send through the atlas's own `built_tx`. Render jobs carry
the target image handle and a clone of the atlas's
`GpuCompletionSink`. The render-world dispatcher writes completion
records into whichever sink came with the job, so completions
physically cannot reach a different atlas — ownership enforces "a
completion only lands on the atlas that issued the request."

During an atlas swap, both `active` and `pending` self-drain via
`AtlasSlot::poll_async_glyphs`, which polls both halves. Dropping
an atlas naturally drops anything in flight against it (workers
holding `Sender` clones discover the channel is closed; render
jobs holding sink clones drop them on the next dispatch).

### Glyph metric decomposition

`GlyphMetrics` stores two independent quantities:

| Field | Source | Atlas-invariant? |
|---|---|---|
| `bearing_x`, `bearing_y` | `bbox.x_min / units_per_em`, `bbox.y_max / units_per_em` | yes — font-defined |
| `pad_x_em`, `pad_y_em` | `actual_pad / canonical_size` (em conversion of the integer-rounded bitmap padding) | no — atlas-specific |

The bitmap has `pad_em` em-units of padding on each side of the
ink. Quad-builders combine the two:

```
quad_left   = (bearing_x - pad_x_em) * size + layout_x
quad_top    = -(bearing_y + pad_y_em) * size + layout_y
quad_width  = pixel_width  * em_scale       // includes 2 × pad
quad_height = pixel_height * em_scale
```

The ink lands at `(layout_x + bearing_x × size, layout_y -
bearing_y × size)` — atlas-invariant. The quad expands outward by
`pad_em` on every side to cover the SDF gradient; the outward
expansion shrinks or grows as the canonical size changes the
ceil-rounded pad residual, but the visible ink position does not
move.

World-text anchor measurement (`measure_anchor_offset`) uses **ink
extent**, not quad extent — `quad_right - pad_x_em × size` — so
right- and center-anchored text does not slide between atlases
either.

This decomposition is what makes dynamic atlas swaps possible
without on-screen jitter and is a hard prerequisite for Phase 2.5.

### Storage texture format

Atlas pages use `TextureFormat::Rgba8Unorm`. Storage textures
require non-sRGB formats on most backends; `Rgba8Unorm` satisfies
this. Distance values are written and read linearly; the text
fragment shader treats texel channels as distance scalars, not
colors, so there is no gamma issue.

`Image` usage flags: `STORAGE_BINDING | COPY_DST | TEXTURE_BINDING`
on every page image. Without `STORAGE_BINDING`, wgpu rejects the
compute-pass bind group; without `COPY_DST` the CPU-mirror sync
path (used during the warm-up burst) doesn't work; without
`TEXTURE_BINDING` the fragment shader can't sample the page.

### Page allocation timing

Allocation happens at `get_or_insert` / `enqueue_gpu_glyph` time,
synchronously, before any GPU work. The shelf allocator (`etagere`)
is stable and depends only on bitmap dimensions, which
`build_edge_buffer` (or `glyph_bitmap_size`) produces synchronously
from the glyph bounding box.

`allocate_gpu_region` marks the page `Dirty` only when the page has
no `image_handle` yet (newly created page that needs `sync_to_gpu`
to allocate the underlying `Image`). Pages that already have a
handle keep their state alone — re-marking them would cause
`sync_to_gpu` to blit the empty CPU mirror over the storage texture
and wipe previously dispatched GPU texels.

### Multi-page atlases

When a glyph won't fit on the current page, the atlas allocates a
new page. The dispatcher groups GPU render jobs by target image
handle, so a single frame can write to multiple pages in parallel
(one compute pass per page).

## Module structure

```
crates/bevy_diegetic/src/text/
├── atlas.rs                    — GlyphAtlas resource; per-atlas pipe; insert paths
├── atlas_config.rs             — RasterBackend, RasterQuality, AtlasConfig + validator
├── atlas_slot.rs               — AtlasSlot::{Single, Swapping} state machine
├── bitmap_dims.rs              — shared bitmap-size formula (CPU + GPU)
├── constants.rs                — DEFAULT_* constants
├── font.rs                     — Font, FontMetrics
├── font_loader.rs              — Bevy asset loader for fonts
├── font_registry.rs            — FontId allocator
├── measurer.rs                 — parley-backed text measurement
├── mod.rs                      — plugin wiring, public exports
├── msdf_rasterizer/            — CPU rasterizer (will move to dev-deps in Phase 4)
│   ├── mod.rs                  — MsdfRasterizer, RasterizedBitmap, DistanceField
│   ├── sdf.rs                  — single-channel SDF rasterizer
│   └── parity.rs               — fdsm reference parity (CPU side)
└── gpu_rasterizer/             — GPU compute rasterizer
    ├── mod.rs                  — GpuRasterizerPlugin, dispatcher trait
    ├── pipeline.rs             — wgpu compute pipeline + bind group layouts
    ├── edges.rs                — outline → flat EdgeSegment buffer (CPU prep)
    ├── request.rs              — AtlasGpuPipe, GpuRenderJob, GpuCompletionSink
    ├── dispatch.rs             — render-schedule dispatch system
    ├── extract.rs              — main → render world bridge
    ├── readback.rs             — STUB; CPU mirror for debug PNG (Phase 3)
    ├── parity.rs               — Rust port of WGSL kernel; fdsm comparison
    └── shaders/
        └── sdf_gen.wgsl        — single-channel SDF generation
```

## Data flow

### GPU glyph request lifecycle

```
main world
  enqueue_gpu_glyph(atlas, key, font_data, ...)
    1. glyph_bitmap_size  — synchronous, cheap (bbox + ceil)
    2. allocate_gpu_region — synchronous, shelf allocator
    3. spawn worker task on atlas's TaskPool:
         build_edge_buffer (outline load + flatten)
         → BuiltGpuRequest::{Built | Invisible}
         → atlas.gpu_pipe.built_tx.send(...)

main schedule (PostUpdate)
  poll_atlas_glyphs:
    atlas.poll_gpu drains atlas's own built_rx:
      Built     → push to gpu_pipe.pending_dispatch
      Invisible → cache GlyphMetrics::INVISIBLE
    drain atlas's completion sink → insert_completed_gpu

main → render bridge (extract schedule)
  collect_gpu_render_jobs:
    drain each atlas's pending_dispatch into GpuRenderJobExtract
    (jobs carry image_handle + sink clone)

render schedule (RenderSystems::PrepareBindGroups)
  dispatch_glyph_compute:
    drain extract → queue.pending (with high-water warning)
    take up to GpuGlyphBudget.per_frame
    partition by image_handle  (one compute pass per image)
    encode per-glyph dispatch  (one workgroup grid per glyph)
    on each job: push GpuGlyphCompletedRecord into job.completions
    if image not yet uploaded: re-queue the job for next frame
    submit
```

Completions are visible to the main world on the next main-world
poll because `GpuCompletionSink` is `Arc<Mutex<Vec<_>>>`.

### Worker pool

Each `GlyphAtlas` owns one `TaskPool` (8 worker threads). CPU
rasterizer atlases spawn fdsm rasterization tasks. GPU rasterizer
atlases spawn `build_edge_buffer` tasks. Both prep tasks are
~hundreds of µs per glyph and amortize across the pool.

## Frame budget management

`GpuGlyphBudget.per_frame` defaults to `u32::MAX` — drain every
pending dispatch each frame. The async pipeline (worker pool →
main → render → GPU) handles backpressure naturally; realistic
font workloads encode in sub-millisecond GPU time per glyph, so
hundreds of glyphs per frame is well within frame budget.

The knob stays as an opt-in cap for apps that observe specific
frame-pacing issues:

| Use case | Budget |
|---|---|
| Default (drain everything) | `u32::MAX` |
| Low-end GPU / mobile worried about encode-time spikes | 8–32 |
| Pathological CJK warmups (thousands of glyphs at once) | finite cap to amortize over frames |

**Latency:** a GPU-backed glyph becomes visible ~3–4 frames after
enqueue (worker → main → extract → render → submit). At 60 fps
that is ~50 ms — imperceptible for interactive typing. Apps that
need instant first-frame visibility pre-warm during loading.

## Atlas swap mechanics

Atlas changes (font, distance-field, raster backend, or canonical
size) go through `AtlasSlot::Swapping { active, pending }`. The
swap completes when `pending.in_flight_count() == 0`. During the
window:

- Sampling reads from `active`.
- New glyph requests route to `pending`.
- `AtlasSlot::poll_async_glyphs` polls both halves so the pending
  atlas self-drains.

Once Phase 4 lands and the runtime CPU rasterizer is gone, the
`(Cpu, Gpu)` swap branch dies. `(Quality, Quality)` and `(Font,
Font)` swaps remain.

## Phased rollout

### Phase 1 — SDF on GPU (closed)

Architecture, per-atlas plumbing, kernel quality, position
invariance — all done. Reference for the closed state:

- `sdf_gen.wgsl` uses closed-form analytic distance for quadratics
  (depressed-cubic solver) and curvature-corrected Newton with
  iteration-min tracking for cubics. 9 Newton seeds for cubics.
  Edge quality is within sub-grayscale-level speckle of fdsm.
- `parity.rs` mirrors the kernel in Rust and runs against fdsm's
  reference SDF for 5 test glyphs.
- `GlyphMetrics` carries font-bearing and atlas-pad separately;
  quad placement combines them so ink position is atlas-invariant.

The `(Msdf, Gpu)` validator warning becomes obsolete once Phase 2
lands. Until then the validator rejects the combo.

### Phase 2 — MSDF on GPU (code landed; visual acceptance pending)

The data path is ready: the storage texture is already RGBA8, the
EdgeSegment `kind` field reserves bits 2–4 for the channel mask,
and the text-renderer fragment shader already handles MSDF reads.

**Files added:**
- `gpu_rasterizer/shaders/msdf_gen.wgsl`

**Files edited:**
- `gpu_rasterizer/request.rs` — convert `GpuGlyphRequest` from a
  struct with a runtime `distance_field: DistanceField` tag into an
  enum with `Sdf { … }` and `Msdf { channel_masks: …, … }` variants.
  Common fields (`key`, `bitmap_size`, `bearing`, `pad_em`,
  `atlas_origin`, `page_index`) stay shared in each variant or
  extract to an inner `Common` struct. The MSDF channel-mask data
  cannot accidentally appear on an SDF request, and vice versa.
- `gpu_rasterizer/edges.rs` — call fdsm's `edge_coloring_simple`
  inside the spawned worker task; pack the channel mask into bits
  2–4 of `EdgeSegment::kind`. Add `EDGE_CHANNEL_MASK_SHIFT: u32 =
  2` and `EDGE_CHANNEL_MASK_BITS: u32 = 0b111` next to the existing
  `EDGE_KIND_*` constants; reuse them in both `sdf_gen.wgsl` and
  `msdf_gen.wgsl`.
- `gpu_rasterizer/pipeline.rs` — second compute pipeline for MSDF.
  The pipeline resource holds both compiled pipelines; the
  dispatcher picks via the request variant `match`.
- `gpu_rasterizer/dispatch.rs` — `dispatch_glyph_compute` matches
  on the request variant. Per-page grouping still happens after
  the variant match.
- `text/atlas_config.rs` — delete the `(Gpu, Msdf)` rejection
  branch in `AtlasConfig::validate`.

**Shader work:**
- Per-channel distance accumulation (one signed distance per RGB
  channel, using only edges whose color mask includes that
  channel).
- **Pseudo-distance** (not true distance): for points past an
  edge's endpoint, extend the edge tangent instead of rounding to
  the endpoint. This is what makes the channels combine cleanly
  at corners. ~50 LoC inside the kernel; the rest is structural
  mirroring of `sdf_gen.wgsl`.

**Acceptance:**
- `warmup_burst/ebg_ascii_256_msdf` GPU ≥ 10× faster than CPU.
  **Status:** the bench case is present in
  `benches/glyph_rasterization.rs::WARMUP_CASES` but
  `bench_gpu_main_thread` currently `continue`s for any
  non-`Sdf` case and only measures main-thread cost, not
  `queue.submit()` round-trip. Remaining work: build a
  wgpu-device-backed harness (the `LazyLock<App>` +
  `RenderPlugin` described under "Bench device initialization")
  and delete the SDF gate. Not a flag flip.
- 3-glyph snapshot test (A, W, V across JetBrains Mono and EB
  Garamond) on the software adapter. The GPU-rendered MSDF page
  is its own golden image; visual review covers apex/corner
  quality.
- `edge_coloring_matches_cpu` unit test: deterministic
  graph-partition output must match exactly between CPU and GPU
  edge-buffer builders. Distance-field output need not.
- **CPU/GPU MSDF side-by-side comparison via the G key toggle in
  the typography example.** Same approach used to validate Phase 1
  SDF: take screenshots before/after toggling, pixel-diff in the
  Typography text region, confirm the GPU result is visually clean
  and the diff is concentrated on letter edges (rasterization
  variance, not positional drift). The CPU path and the G chip
  stay in place through Phases 2 / 2.5 / 3 specifically so this
  comparison stays available; only Phase 4 retires them.

### Retrospective

**What worked:**
- Enum split of `GpuGlyphRequest` into `Sdf(Common)` / `Msdf(Common)` — variant tag drives pipeline selection in `dispatch.rs`; no runtime tag misuse possible.
- Channel mask packed into bits 2–4 of `EdgeSegment::kind` (`EDGE_CHANNEL_MASK_SHIFT=2`, `EDGE_CHANNEL_MASK_BITS=0b111`); deterministic `edge_coloring_matches_cpu` test passes on JetBrains Mono and EB Garamond.
- Pseudo-distance mirroring of fdsm's `distance_to_pseudodistance` (~80 LoC including helpers — the doc's "~50 LoC" was close).

**What deviated from the plan:**
- `dispatch.rs` partitions per-image dispatches into separate SDF and MSDF vectors and emits a single compute pass per image that calls `set_pipeline` twice (once per group). MSDF jobs whose pipeline isn't yet compiled re-queue rather than drop, so warm-up is robust.
- `AtlasConfigError` collapsed to `pub type AtlasConfigError = Infallible;` (rather than `enum AtlasConfigError {}`) so callers' `if let Err(_)` keeps compiling without an `#[allow(clippy::uninhabited_references)]`.
- Pulled `build_per_glyph_dispatch` helper out of `encode_image` to satisfy `clippy::too_many_lines` after the variant match grew the function body.
- Per-segment color is read from `ColoredSegment::color.value()` (a `u8` with bits 0–2 = R/G/B), copied straight into `kind` — no separate `channel_masks: …` field on the `Msdf` variant.

**Surprises:**
- fdsm stores `parameter` in `DistanceResult` from the endpoint-tangent projection (potentially in `[0,1]` for the endpoint guess), then refines via cubic roots; the WGSL kernel mirrors that exactly so `parameter < 0` / `> 1` triggers pseudo correctly.
- Sign reconciliation: chose a simple "median sign vs winding rule" flip in the kernel rather than three independent per-edge signs from cross-product alone. Per-channel signs already come from `cross(tangent, foot - pt)`; the global flip just resolves disagreements with the winding count.
- `signed_pseudo_distance` returns the raw pseudo when its squared magnitude is `<= distance_squared` of the true distance (matching fdsm); otherwise falls back to the cross-product-signed true distance.

**Implications for remaining phases:**
- Phase 2.1's "in-shader error correction" tweak already has the data it needs — three signed channels are produced before the texture store and could be median-checked in place.
- Phase 2.1's "MTSDF A-channel emitted during MSDF pass" lands as one extra `min()` per edge inside the existing per-edge loop in `msdf_gen.wgsl`; no separate Phase 3 shader needed for the A channel itself.
- Phase 4's "delete `RasterBackend` enum" is more constrained: removing the runtime CPU path also requires deleting `enqueue_on_atlas`'s sibling on the CPU side and the `(Cpu, Gpu)` swap branch in `AtlasSlot`. `AtlasConfigError = Infallible` is a no-op compatible alias and can stay or be removed.
- The variant-aware `encode_image` + `build_per_glyph_dispatch` split is the surface Phase 2.5 will extend when `GlyphAtlases: HashMap<RasterQuality, GlyphAtlas>` lands — each bucket gets its own image-handle stream that flows through the same dispatcher.

### Phase 2 Review

Architect review of remaining phases against the Phase 2
retrospective produced 9 findings; 5 applied as mechanical
clarifications, 4 brought to the user as real decisions.

Mechanical edits:
- Phase 3 reframed: generator side is mechanical, the
  consumer-side effects pipeline is the actual remaining work.
- Phase 4 file-touch list corrected: `text/mod.rs` holds the
  swap-trigger conjunct, not `atlas_slot.rs`. The substantive
  deletion is the CPU worker-pool branch in `atlas.rs` that
  today catches every glyph oversized for `allocate_gpu_region`
  or running without a GPU dispatcher.
- Phase 2 bench acceptance clarified: `ebg_ascii_256_msdf` GPU
  bench requires a new wgpu-device-backed harness, not just
  flipping the SDF gate in the existing main-thread bench.
- Phase 2.5 swap-state-machine vs bucket-selection-state-machine
  documented as orthogonal axes; behavior of in-flight glyphs
  during entity re-binding spelled out.
- `AtlasConfigError = Infallible` flagged as a temporary alias
  that un-aliases when a real `(backend, distance_field)`
  rejection comes back (e.g. `(Gpu, Mtsdf)` on devices without
  4-channel storage support).

User decisions:
- **D1 (in-shader error correction):** chose true-parity
  two-pass via `ReadWrite` storage texture, fully GPU-resident.
  Phase 2.1 row rewritten; validator gains a
  `STORAGE_READ_WRITE` check. Workgroup-memory and ping-pong
  are documented as fallbacks if the device lacks the feature.
- **D2 (oversized-glyph policy):** chose dynamic per-page
  sizing — when a glyph doesn't fit any existing page,
  allocate a new page sized to fit. The only hard-fail case
  becomes "glyph exceeds `max_texture_dimension_2d`," which
  is a real hardware limit. Atlas page metadata grows
  `width / height` per page; `AtlasConfig::page_size` becomes
  the default, not a guarantee.
- **D3 (MTSDF generator):** chose shader-defs single source.
  `msdf_gen.wgsl` gains `#ifdef MTSDF` at the alpha-write site;
  MTSDF pipeline queues with `MTSDF=1`. Avoids copy-paste drift
  between MSDF and MTSDF kernels.
- **D4 (per-bucket budget):** removed the per-frame cap as a
  default — `GpuGlyphBudget.per_frame` becomes `u32::MAX`. The
  async pipeline handles backpressure end to end. Round-robin
  across buckets stays as the policy when an app opts in to a
  finite cap; the default path drains everything.

### Phase 2.1 — GPU-only quality exploration

Once Phase 2 lands and the CPU/GPU comparison gives us a baseline,
we have headroom on the GPU that fdsm doesn't have on the CPU. The
GPU runs the per-pixel distance search in parallel and has all the
edge data co-located in the workgroup — operations that would be
prohibitive on CPU are routine on GPU. Candidate tweaks to
evaluate:

| Tweak | What it addresses | GPU cost |
|---|---|---|
| Per-pixel supersampling (sample distance at 2×2 or 4×4 sub-pixel positions and combine) | Boundary anti-aliasing baked into the SDF rather than reconstructed at sample time | 4× / 16× per-pixel work; bounded |
| Two-pass in-shader error correction (second compute pass reads neighbor texels; patches channel-median disagreements in place) | True parity with fdsm's `correct_error_msdf`, fully GPU-resident — no CPU round-trip | Second compute dispatch + migrate `msdf_gen.wgsl` output binding from `WriteOnly` to `ReadWrite` (`Rgba8Unorm` + `STORAGE_READ_WRITE` feature; validator gains a check). Workgroup-memory + barrier or ping-pong are fallbacks if `ReadWrite` is unavailable on a target device. |
| Tangent-direction pseudo-distance instead of continuation | Reduces corner artifacts; fdsm's `TangentPseudoDistanceField` is documented as more accurate at corners but more expensive | A handful of extra dot products per edge per pixel |
| Closed-form cubic distance | Eliminate the remaining Newton stickiness on cubics; quintic root finder (numerically more delicate than the depressed cubic we use for quadratics) | Larger kernel; bounded |
| Hybrid f32/f64 distance computation | Match fdsm's f64 precision on the candidate distance set without f64 throughout | Modest; only the best-candidate refinement uses f64 |
| Per-glyph adaptive sample count | Use coarse sampling for low-detail glyphs and finer sampling near sharp features | Glyph-dependent; needs heuristic |

These are exploratory — each gets a separate measurement against
the Phase 2 baseline (visual diff + parity test + bench) before
landing. The bar is "demonstrably better-looking output for
reasonable GPU cost"; tweaks that don't move the needle get
dropped. The order is not fixed — pick whichever addresses the
most visible artifact in the post-Phase-2 comparison first.

### Phase 2.5 — Per-element atlas binding

**Goal:** allow a single scene to render small overlay text and a
large headline simultaneously, each from an atlas of the right
canonical size. SDFs sample cleanly when the atlas resolution is
near the display resolution and degrade when they're far apart —
"Huge" (256 px) atlas sampled at 12 px display is the worst case.

**Data model:**
- `GlyphAtlases` resource (replaces single `GlyphAtlas`):
  `HashMap<RasterQuality, GlyphAtlas>` or array indexed by the 5
  variants of `RasterQuality`. Each bucket is a full, independent
  atlas with its own pages, pipe, and swap state.
- New `AtlasSizeBinding` component on text-bearing entities:
  - `Force(RasterQuality)` — explicit choice
  - `Auto` — derive from screen-space size each frame
  - `Inherit` — use parent's value (cascade)
- Glyph cache key already includes `canonical_size`, so each
  bucket's cache populates only with glyphs requested at that
  size.

**Phase A — manual binding:**
- Generalize `GlyphAtlas` resource into `GlyphAtlases`.
- Add `AtlasSizeBinding`, default = `Force(Small)`.
- Shaping/reconcile picks the atlas via the component.
- Test: tag the center "Typography" entity as `Force(Huge)`,
  leave everything else as `Force(Small)`. Both should render
  well.

**Dispatcher / queue plumbing:**
`partition_by_image` in `dispatch.rs` already keys solely on
`Handle<Image>`, so per-bucket atlases each owning their own
`AtlasGpuPipe` produce N disjoint image-handle streams that
flow through the existing dispatcher unchanged. State that
becomes ambiguous across buckets and needs a small change:
- `GpuRenderJob` gets a `bucket: RasterQuality` tag so the
  queue-overflow warning at `dispatch.rs::QUEUE_HIGH_WATER`
  names the bucket responsible.
- `GpuRenderJobQueue.pending` stays a single render-world
  resource (no per-bucket fan-out needed).
- `collect_gpu_render_jobs` iterates `HashMap<RasterQuality,
  GlyphAtlas>` instead of `&GlyphAtlas`.

**Budget policy across buckets:**
The default `GpuGlyphBudget.per_frame` becomes `u32::MAX` — the
async pipeline (worker pool → main → render → GPU) handles
backpressure, and realistic font workloads (ASCII-94 × N
buckets) encode in sub-millisecond GPU time. No per-frame cap
is needed by default. The `per_frame` knob stays as an opt-in
limit for low-end GPUs or thousands-of-CJK-glyphs scenarios;
when set to a finite value, the dispatcher drains
**round-robin across buckets** in `RasterQuality` order so a
single bucket can't starve others. Round-robin only fires when
the cap is set.

**Swap state machine vs bucket-selection state machine:**
These are orthogonal axes. `AtlasSlot::Swapping` still handles
font / quality / backend changes *within* one bucket; entity-level
re-binding moves an entity from bucket X's stream to bucket Y's
stream without swapping either bucket. Behavior when an
`Auto`-bound entity re-binds mid-flight: pending glyphs in
bucket X still complete and land in X's cache (harmless — the
entity simply samples Y now; X's entry stays cached for the next
entity that re-binds back). When buckets exist, `AtlasSlot`
generalizes to `AtlasSlots(HashMap<RasterQuality, AtlasSlot>)`.

**Phase B — auto / screen-space:**
- New system: per `Auto`-bound text element, compute effective
  screen-space pixel size (for 3D world text: project the text's
  display-height-in-world through the camera matrix; for 2D
  panel text: layout size directly).
- Pick bucket = smallest canonical ≥ screen-space size, capped at
  Huge. SDFs upscale better than they downscale.
- Hysteresis at bucket boundaries (e.g. >15% size change required
  to switch).

**Cascade semantics:**
- `AtlasSizeBinding` on a parent panel applies to all text
  descendants unless a child overrides. Bevy doesn't ship a
  propagation system for arbitrary components, so this needs a
  small custom propagation system (~20 lines).

**Memory:** 5 buckets per font, ASCII-94, at canonical sizes
16/32/64/128/256: `(16² + 32² + 64² + 128² + 256²) × 94 ≈ 8.5 MB
per font`. Acceptable; bounded.

**Warmup:** lazy — only populate buckets that get used. Matches
the "operational atlas, dynamically populated" framing.

The bearing/pad split that landed in Phase 1 is the prerequisite
that makes dynamic bucket switching jitter-free.

### Phase 3 — MTSDF and text effects

**MTSDF generation** is nearly free once Phase 2 lands. After
Phase 2 the generator side is mechanical; the work that remains
in Phase 3 is the *consumer-side effects pipeline* (parameter
components, shader uniforms, fragment-shader extension).

Generator side:
- R/G/B carry the MSDF channels as before.
- A carries the true unsigned distance (min across **all** edges,
  ignoring channel masks). The per-edge loop in `msdf_gen.wgsl`
  already computes `ed.dist_sq` for every edge; adding an
  unconditional `min()` into `best_sq_a` is one extra line per
  edge plus one extra `clamp` and a `vec4` `textureStore`.
- Storage is already RGBA8.
- **One shader source, two pipelines via `shader_defs`.** Add
  `#ifdef MTSDF` at the alpha-write site in `msdf_gen.wgsl`. The
  existing `pipeline.rs` `shader_defs` plumbing (currently empty)
  passes `MTSDF=1` when queuing the MTSDF pipeline. MSDF and
  MTSDF stay in lockstep on every future shader fix; no copy-paste
  drift.
- `DistanceField::Mtsdf` enum variant; `GpuGlyphRequest::Mtsdf`
  variant; third cached pipeline in `GpuRasterizerPipeline`;
  dispatcher variant arm — all per the Phase 2 precedent. CPU
  rasterizer (test-only) calls fdsm's `distance4` instead of
  `distance3`.

**Why MTSDF matters:** the median of R/G/B is "distance-like" but
has discontinuities at the corner texels where two channels swap
which one is the median. The A channel is a continuous true
distance — stable for text effects.

**Text effects** consume the A channel:

| Effect | How A is used |
|---|---|
| Outline / stroke | Sample A at a threshold different from 0.5 |
| Drop shadow | Offset sample position, threshold A at softer value |
| Glow / halo | A's distance controls glow brightness via smoothstep |
| Inner / outer bevel | SDF gradient as faux normal for fake lighting |

The effects pipeline (parameter components on text entities,
shader uniforms, fragment-shader extension) is a separate feature
on top of MTSDF generation.

### Phase 4 — Retire the runtime CPU rasterizer

Once GPU MSDF is verified across the supported font set:

- Delete the `RasterBackend` enum and every branch on it. Touch
  sites: `atlas_config.rs` (the enum + `backend` field), the
  `target_backend == active_backend` conjunct of the swap
  trigger in `text/mod.rs`, the `backend` field on
  `AtlasPreference`, and the `backend` field on `AtlasSlot`.
  `atlas_slot.rs` itself is otherwise backend-agnostic.
- Delete the CPU worker-pool branch in `atlas.rs` that
  `spawn`s `rasterizer.rasterize(...)` when the GPU dispatcher
  is absent or `allocate_gpu_region` returns `None`. Test sites
  in `text/atlas.rs` and elsewhere that construct
  `RasterBackend::Cpu` also disappear.
- Delete the `G` chip from the typography example; one less
  runtime knob.
- Move `text/msdf_rasterizer/` and the `fdsm` /
  `fdsm-ttf-parser` dependencies behind `#[cfg(test)]`. The
  parity tests are the only consumers; they stay in-tree as the
  algorithmic reference.
- Move the fdsm dependency from `dependencies` to
  `dev-dependencies` in `crates/bevy_diegetic/Cargo.toml`.

When a real `(backend, distance_field)` rejection comes back —
e.g. `(Gpu, Mtsdf)` on a device that lacks 4-channel storage
support — `AtlasConfigError = Infallible` un-aliases back to a
real enum with that variant. Until then the alias keeps callers'
`if let Err(_)` form compiling.

**Oversized-glyph policy (no CPU fallback):**
1. If the glyph doesn't fit on any existing page, allocate a new
   page sized to `max(default_page_size, bitmap_size + 2 ×
   padding)`. Existing pages keep their dimensions; pages are no
   longer required to be uniform within an atlas.
2. If `bitmap_size` exceeds
   `RenderDevice::limits().max_texture_dimension_2d`, log a
   structured `warn!` (font + codepoint + bitmap size + device
   limit) and mark the glyph invisible. This is the only
   unservable case and it's a hardware limit, not a config knob.

Per-page dimensions become part of the atlas page metadata
(`PageDescriptor.width / height` instead of a single
`AtlasConfig.page_size()`). Touch sites: `atlas.rs` page
allocation; `atlas_config.rs::page_size` becomes the *default*
page size, not a guarantee. The fragment shader already binds
per-page textures and UV math is per-glyph, so non-uniform page
dimensions need no sampler-side change.

Estimated impact: ~30–40% reduction in text-pipeline branching.
The architecture flattens significantly.

## Testing strategy

### Unit tests

- `gpu_rasterizer/edges.rs::tests` — `build_edge_buffer` produces
  the expected `EdgeSegment` array (count, control points,
  channel masks for MSDF).
- `gpu_rasterizer/pipeline.rs::tests` — pipeline initializes
  against the test wgpu device.

### Parity tests

`gpu_rasterizer/parity.rs` ports the WGSL kernel to Rust and
compares against fdsm's CPU SDF reference for 5 representative
glyphs (JetBrains Mono A/W/O, EB Garamond V/A). The Rust port
mirrors the kernel arithmetic verbatim so any WGSL algorithm bug
shows up in the port too. Catches algorithm bugs without spinning
up a wgpu device.

Tolerance accounts for the documented per-pixel-ray-cast vs
scanline-fill sign-correction divergence at glyph boundaries
(thin halo, ~1 px of signed distance).

### Bench coverage

`benches/glyph_rasterization.rs` extends to cover the GPU path. A
`LazyLock<App>` with `RenderPlugin` and
`synchronous_pipeline_compilation: true` shares one wgpu device
across iterations.

### Visual regression

Snapshot tests on the wgpu software adapter render a fixed glyph
set through the GPU path and compare against committed golden
images. First run produces the snapshot; subsequent runs catch
unintended kernel changes. The GPU output is its own golden — no
byte-for-byte comparison to the CPU rasterizer.

## Synchronization

A glyph is marked complete when `queue.submit()` returns for its
compute pass. The compute-pass write into the atlas page storage
texture becomes visible to subsequent render passes via wgpu's
documented command-buffer ordering — no explicit fence, no
`map_async` wait, no one-frame defer. If a vendor-specific
barrier bug surfaces (Apple Silicon has the strictest
synchronization model), insert an explicit `Barrier` between the
compute pass and the first sampling render pass.

`in_flight_count` includes glyphs whose request is queued,
dispatched, or awaiting the completion sink drain.

## Limitations

Accepted limits of the design, not bugs.

### GPU device loss

If the OS terminates the GPU context (driver crash, power
management, resource exhaustion), in-flight glyph dispatches are
lost. The atlas has no detection or recovery — affected glyphs
stay in `in_flight` until the app restarts. A side effect: an
atlas swap that started before the loss never completes because
the swap completion check `pending.in_flight_count() == 0` never
fires.

### Per-glyph dispatch timeout

A compute dispatch on a heavily loaded GPU runs to completion or
hangs. The atlas has no per-glyph timeout.
`GpuGlyphBudget.per_frame` bounds per-frame work so a
pathological single-glyph stall does not freeze the app, but the
affected glyph itself remains pending.

### Unbounded request queue

`GpuRenderJobQueue.pending` has no hard cap. Sustained enqueue
rates above the per-frame budget grow the queue without bound
(4096 pending triggers a warning log). Apps that enqueue
thousands of glyphs at runtime must pre-warm during loading or
raise `GpuGlyphBudget`.

### GPU vendor floating-point drift

Different GPU vendors implement floating-point math slightly
differently. The same glyph rasterized on two vendors may produce
distance values that differ by 1–3 quantization units — minor
edge softness differences at extreme zoom, rarely perceptible at
normal text sizes. The parity test uses the wgpu software adapter
(deterministic); real-hardware variance is not CI-tested.
Vendor-specific reports reproduce via `examples/msdf_font_audit.rs`.

### Synchronization barrier on Apple Silicon

The design trusts wgpu's documented command-buffer ordering to
make compute-pass writes visible to subsequent sampling passes
without an explicit barrier. Apple Silicon (Metal) has the
strictest synchronization model and is the most likely candidate
for a barrier bug. If real-hardware testing surfaces stale or
inverted samples on Apple Silicon, an explicit `Barrier` between
compute and the first sampling render pass resolves it. The
parity test (software adapter) does not catch this class of bug.

## Diagnostics

### `examples/msdf_font_audit.rs`

Standalone binary that loads a curated font set, rasterizes every
BMP codepoint through CPU MSDF and GPU MSDF, and reports per-glyph
disagreement counts (pixels where the GPU median would
mis-classify relative to CPU). Lands alongside Phase 2. Used to
triage reports of MSDF artifacts on specific fonts.

## wgpu limits validation

At `GpuRasterizerPlugin::build`, after `RenderApp` is reachable,
the validator queries `RenderDevice::limits()` and checks:

- `max_storage_buffer_binding_size` ≥ the worst-case edge buffer
  (estimate: 2000 edges × 36 bytes ≈ 72 KB — trivially under any
  desktop limit).
- `max_storage_buffers_per_shader_stage` ≥ 3 (edges, glyph
  headers, params; the output texture binds separately).
- `max_compute_workgroup_size_x` ≥ 8 and `_y` ≥ 8.
- `Rgba8Unorm` supports `STORAGE_BINDING` write access (queried
  via `adapter.get_texture_format_features`).

On any failed check: log a warning and do not insert the dispatch
system. Text rendering will not work on that device. The crate
does not support fallback rasterizers.

## Pipeline parameters

The wgpu compute pipeline is built once per process at plugin
init. Per-atlas runtime parameters (`sdf_range`, etc.) pass
through the uniform buffer (`RasterParams`), not as shader
constants, so the pipeline stays single-instance and never
re-compiles.

## Bench device initialization

The GPU bench (`benches/glyph_rasterization.rs`) shares one wgpu
device across iterations via a `LazyLock<App>` that adds
`bevy::render::RenderPlugin { render_creation:
RenderCreation::Automatic(WgpuSettings::default()),
synchronous_pipeline_compilation: true }` on top of
`MinimalPlugins`. Combined setup is ~10 ms one-time and amortized
across all iterations.

## Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| MSDF pseudo-distance shader bug | High during Phase 2 development | Parity port in `gpu_rasterizer/parity.rs` extends to MSDF; software-adapter snapshot tests |
| Frame budget default (16) is wrong for some workload | Medium | Tune after bench data. User-overridable via `GpuGlyphBudget.per_frame` |
| MSDF artifacts on specific fonts | Low (unmeasured) | Phase 3 error-correction pass; `msdf_font_audit.rs` triages |
| GPU device loss strands in-flight glyphs | Low (vendor-dependent) | Accepted; see "Limitations → GPU device loss" |
| Vendor floating-point drift breaks parity test | Medium | Software-adapter parity in CI; real-hardware drift documented and accepted |
| Subtle wgpu barrier bug on Apple Silicon | Low-medium | Add explicit `Barrier` if observed |
| Per-element atlas binding doubles memory | Low | 5 buckets × ASCII-94 × float-sized atlases ≈ 8.5 MB per font; bounded |
| Auto bucket selection thrashes at boundaries | Medium | Hysteresis (>15% size change required to switch) |

## Relationship to other docs

- `sdf_text.md` — describes the SDF / MSDF / MTSDF distance-field
  encodings. This doc covers the rasterizer that produces them.
- `roadmap/` — none of the in-flight roadmap items conflict; the
  GPU rasterizer is purely additive.

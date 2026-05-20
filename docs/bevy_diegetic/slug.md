# Slug text renderer backend

## Goal

Add an experimental vector text renderer backend to `bevy_diegetic`
based on Eric Lengyel's Slug reference shaders. This is not a
replacement for the current MTSDF renderer at first. It is an
alternative backend that reuses the existing text stack up to shaping
and layout, then renders glyphs from curve data instead of
pre-rasterized distance-field atlas texels.

The backend split should happen after text shaping. A shaped run gives
the renderer glyph identities and positions; the selected renderer
decides whether those glyphs become atlas-textured distance-field
quads or Slug curve-backed quads.

## Source and license

Reference source:

- https://github.com/EricLengyel/Slug
- https://jcgt.org/published/0006/02/02/
- https://terathon.com/blog/decade-slug.html

The GitHub repository contains HLSL reference shaders and is
dual-licensed under MIT OR Apache-2.0. The README states that the
patent has been dedicated to the public domain, and that distributed
software using the code must give credit.

Implementation rule:

- Use the MIT side of the dual license for the port unless there is a
  reason to choose Apache-2.0 for a specific file.
- Preserve SPDX/copyright attribution in ported shader/source files.
- Add Slug attribution to `crates/bevy_diegetic/NOTICE` before any
  copied or ported shader logic is distributed.
- Do not depend on the commercial Slug SDK or font converter.

## Terminology

- `TextRendererBackend`: chooses the text rendering model, initially
  `DistanceField` or `Slug`.
- `DistanceField`: describes the atlas texel encoding inside the
  current renderer: `Sdf`, `Msdf`, or `Mtsdf`.
- `RasterBackend`: describes who computes distance-field atlas texels:
  CPU or GPU.
- `SlugStorageBackend`: future name for Slug curve/band storage
  strategy if both buffer and texture layouts are supported.

Slug should not be added to `DistanceField`. It is not another atlas
encoding; it is a different renderer.

## Feasibility isolation

Keep the first Slug work structurally separate from the production text
modules until the renderer proves that it can draw correct text. The
first branch should be a feasibility study, not a shared-renderer
refactor.

Initial code should live behind an experimental feature in a private
Slug spike module and a standalone example. It should not add
`TextRendererBackend`, rewrite `GlyphQuadData`, or change panel/world
text systems until plain Slug text is rendering.

Allowed dependencies for the isolated spike:

- existing font assets and font-loading helpers where convenient
- `ttf-parser`/`fdsm_ttf_parser` outline loading
- a private Slug-specific mesh/material/pipeline path
- `examples/slug_text.rs` as the first visible target

Deferred until after feasibility:

- public backend configuration
- shared renderer-neutral glyph instance types
- panel and `WorldText` integration
- production readiness events
- MTSDF/Slug runtime switching

## Existing pieces to keep

The Slug backend should use the current `bevy_diegetic` front half:

- `FontRegistry` and `Font` asset loading remain the source of font
  bytes and family resolution.
- `parley` remains responsible for shaping, fallback, font features,
  cluster advances, line breaking, and glyph IDs.
- `shape_text_cached` remains the panel/world text shaping entrypoint,
  after it is brought into parity with the measurement path.
- `ShapedTextRun`, `LayoutTextStyle`, panel layout, `WorldText`, clip
  rectangles, cascaded styling, and readiness semantics stay shared
  across renderers.
- Existing MTSDF/SDF/MSDF atlas rendering remains available and remains
  the production default until the Slug path is proven.

Mechanical prerequisite: align render shaping with measurement before
using it as the shared Slug front half. The measurer applies weight,
slant, letter spacing, and word spacing; `shape_text_cached` should
apply the same relevant style inputs so measured layout and rendered
glyph placement cannot diverge.

## Backend ownership

After the isolated Slug renderer proves it can draw text, add a
renderer-level preference layer instead of reusing atlas configuration:

```rust
pub enum TextRendererBackend {
    DistanceField,
    Slug,
}

pub struct TextRendererPreference {
    pub backend: TextRendererBackend,
}
```

The first API can be a global resource. Per-text overrides can come
later if needed. `AtlasConfig`, `AtlasPreference`, `AtlasSlot`,
`DistanceField`, and `RasterBackend` remain owned by the
distance-field backend.

Slug needs its own backend resource, not just a glyph-data map:

- cache entries
- async outline preprocessing work
- GPU buffer or texture allocation
- dirty upload tracking
- storage generation/version tracking
- lookup state equivalent to queued, pending, ready, and failed
- readiness polling for panel and world text

Render systems should ask the selected backend for ready glyph
instances. They should not reach into Slug cache internals.

## Shared renderer contract

After feasibility and before production integration, define a
renderer-neutral contract for the point after shaping:

- resolved font/face identity
- glyph ID
- glyph origin, baseline, advance, and bounds
- color and per-run style data
- clip rectangle in panel/world-local coordinates
- effect margin requirements
- backend lookup result: ready, queued, pending, invisible, or failed

Current `GlyphQuadData` is atlas-specific because it stores atlas UVs
and clipping mutates those UVs. Slug needs glyph-local curve
coordinates plus backend metadata offsets. Split the pipeline into:

1. backend-neutral positioned glyph instances
2. distance-field mesh data with atlas UVs
3. Slug mesh data with glyph-local coordinates and curve/band handles

Clipping is part of this contract. Slug clipping must preserve
glyph-local coordinates after CPU-side clipping, including partial
clips, empty clips, effect margins, and shadow offsets.

## Font identity and fallback

Slug outline extraction needs the exact font face that produced each
glyph. A glyph ID alone is not portable across fonts. The current
`ShapedGlyph` carries the glyph ID and placement, while later render
paths request outlines from the styled `font_id`; that is not enough
for parley fallback or mixed-font shaping.

The long-term shape boundary should carry resolved font identity per
glyph or per glyph run:

```rust
pub struct ResolvedGlyphId {
    pub font_id: FontId,
    pub face_index: u32,
    pub glyph_id: u16,
    pub font_generation: u64,
}
```

Exact field names can change, but the Slug cache key must be based on
the resolved face, not only the requested style font.

## Slug data model

Each Slug glyph cache entry needs:

- resolved font identity and glyph ID
- glyph bounds in em space
- bearing and advance-compatible metrics derived from the resolved font
  face
- curve data packed from quadratic Bezier segments
- horizontal band records
- vertical band records
- stable handles or offsets into shared GPU buffers or texture-backed
  storage

The reference README gives two important packing constraints:

- Curve data stores quadratic Bezier control points.
- Bands index curves relevant to a ray direction and should be sorted
  by descending maximum coordinate.

Use `ttf-parser`/`fdsm_ttf_parser` for outline extraction at first.
The existing GPU rasterizer already converts font outlines into flat
edge records; the Slug preprocessor should share outline loading where
possible, but it should produce Slug-specific band data rather than
reusing MSDF edge-coloring data.

Define a versioned `SlugGlyphKey` before integration:

```rust
pub struct SlugGlyphKey {
    pub font: ResolvedGlyphId,
    pub preprocess_version: u32,
    pub banding_profile: SlugBandingProfile,
    pub storage_profile: SlugStorageProfile,
    pub effect: SlugEffectProfile,
}
```

The key must account for font reloads, face index, preprocessing
algorithm changes, banding options, storage layout, and effect margins.

If Slug storage offsets are embedded in meshes, storage migration must
either preserve stable handles or emit a backend event that forces mesh
rebuilds. Buffer growth, compaction, and storage-layout changes cannot
silently leave stale offsets in existing mesh attributes.

## Curve preprocessing

Slug's reference packing is quadratic. The preprocessor must make this
explicit:

- lines can be encoded as degenerate quadratics
- quadratic segments can pass through directly
- cubic segments need a chosen strategy before broad font support:
  either cubic-to-quadratic approximation with an error bound, or an
  explicitly limited TrueType-quadratic-only spike

Unsupported outline formats should produce a visible backend failure
state, not missing glyphs.

## Rendering model

The Slug renderer should use one quad per glyph, sized to glyph bounds
plus any effect margin. The quad should carry enough data for the Slug
shader to evaluate coverage in glyph-local coordinates.

The renderer must plan for both vertex and fragment work. The reference
shader path is not just a fragment shader swap; it needs Slug-specific
vertex data such as glyph-local coordinates, band/glyph metadata, and
the transforms needed for antialiasing and dynamic dilation.

The Phase 1 spike must prove the binding model before production
integration:

- Bevy `MaterialExtension` vs custom render pipeline
- storage buffers vs texture-packed reference-style data
- fragment-stage feature/limit requirements
- alpha, clipping, depth, and shadow/prepass requirements for the
  chosen first scope
- clean fallback to the distance-field renderer when required device
  capabilities are unavailable

The first shader target should be a direct, readable WGSL port. Avoid
early compression or clever packing beyond what is needed to match the
reference algorithm. Once parity is established, optimize storage and
buffer access.

## Readiness semantics

`WorldTextReady` should remain a backend-neutral public event: selected
backend data is available, render entities are spawned, and Bevy has
run through bounds/transform propagation.

Distance-field readiness currently means atlas glyphs are rasterized.
Slug readiness must include:

- outline extraction
- curve/band preprocessing
- GPU storage allocation and upload
- material or pipeline availability
- mesh spawn
- `AwaitingReady` after spawn, then `WorldTextReady` after propagation

Do not make Slug fake atlas readiness. Share the public event semantics
and general state machine, but let each backend own its internal
preparation stages.

## Effects

Treat effects as separate milestones:

1. Fill rendering only.
2. Hard drop shadow by drawing the same shaped glyph run with an offset
   and shadow color behind the fill.
3. True outline by preprocessing offset contours or a Slug-compatible
   outline dataset and drawing it behind the fill.

Do not implement outlines by scaling the glyph. Scaling changes
bearings, counters, joins, and stroke thickness. A true outline needs
expanded contours or equivalent effect geometry.

Soft shadows, glow, and blur are not core Slug functionality. Keep
them out of the first backend unless they fall naturally out of a later
postprocess or multi-pass effect.

## Phases

Future phases must use sequential integer labels. If a new phase is
inserted, renumber later phases in this section rather than adding
lettered or decimal sub-phases.

### Phase 0: isolated feasibility module

Status: completed.

Completed:

- Created a private experimental Slug module behind the experimental
  feature path.
- Added standalone `examples/slug_text.rs`.
- Kept the code separate from production panel/world text modules.
- Avoided `TextRendererBackend`, shared glyph instance types, and
  production readiness integration.

Exit criteria: met. The repository has an isolated Slug feasibility
target that can be built and run without changing the current
distance-field text renderer.

### Phase 1: Slug algorithm and pipeline spike

Status: completed for manual feasibility; formal fixtures and
tolerances remain in the test matrix.

Completed:

- Loaded JetBrains Mono glyph outlines and shaped `Typography` through
  parley so Slug receives glyph IDs and advances from the shaper.
- Built quadratic curve records and horizontal band records for
  supported TrueType outlines.
- Added a CJK probe using bundled open-source Noto Sans CJK assets. The
  current quadratic-only spike rejects the CFF/cubic outline with clear
  diagnostics, preserving cubic support as future work.
- Ported the useful first fill path to WGSL using a private
  `MaterialExtension` path.
- Rendered Slug fill quads in the isolated example.
- Added a `WorldText` contrast overlay to compare the current MTSDF path
  with Slug output.
- Fixed Slug glyph placement to use the shaped glyph origin/advance
  rather than treating glyph bounds as the advance cursor.
- Switched Slug fill to masked/discarded alpha so non-discarded Slug
  fragments participate in depth writes.
- Removed the standalone `WorldText` OIT depth offset so world-space
  text no longer fakes its depth against unrelated scene geometry.

Exit criteria: met for feasibility. One font path, one storage path,
and one pipeline path render correct-looking fill coverage in the
manual example. Named fixture tests and numeric tolerances are still
future verification work.

### Phase 2: SlugTextRun and glyph cache in the example

Status: completed.

Completed:

- Add CPU-only `SlugTextRun` data for one shaped text entity. The run
  stores an ordered list of glyph instances with glyph ID, origin,
  advance, bounds contribution, and a key/reference to reusable packed
  glyph data.
- Add `SlugFontKey`, `SlugGlyphKey`, and `SlugGlyphCache` names for the
  example path. The cache key is `(font identity, glyph id)`.
- Keep run data per entity for now. Do not add shared run caching or
  word-level caching until profiling shows a real need.
- Cache packed glyph curve/band data at glyph granularity so repeated
  glyphs reuse outline preprocessing.
- Update `examples/slug_text.rs` to build one `SlugTextRun` for
  `Typography` and then drive the current per-glyph rendering path from
  that run.
- Keep the `WorldText` contrast overlay in the example.
- Defer the final GPU representation decision until this data shape
  exists and exposes real bottlenecks.

Exit criteria: met. The example renders from `SlugTextRun` data and
reuses packed glyph data through `SlugGlyphCache`, while keeping the
same visible Slug output target as Phase 1.

### Phase 3: shared text prerequisites

- Align `shape_text_cached` with the parley measurement path for
  relevant text style inputs.
- Define the resolved font/face identity carried by shaped glyphs.
- Define renderer-neutral positioned glyph instances.
- Define backend-neutral lookup/readiness states.

Exit criteria: existing distance-field panel and world text still
render through the refactored shared front half, with measured and
rendered advances matching for style fixtures.

### Phase 4: Slug backend resource

- Add `TextRendererBackend::Slug` behind an experimental feature.
- Add a Slug backend resource that owns cache state, async work, GPU
  storage, uploads, lookup state, and readiness polling.
- Define `SlugGlyphKey` and invalidation rules.
- Batch shaped glyphs into Slug quads through the renderer-neutral
  contract.
- Add a small `examples/slug_text.rs` comparing Slug and MTSDF output.

Exit criteria: shaped strings render through parley and the Slug
backend, with glyph positions matching the existing renderer under
explicit layout fixtures.

### Phase 5: panel and world text parity

- Support panel text clipping while preserving glyph-local coordinates.
- Support `WorldText` anchoring and backend-neutral readiness behavior.
- Preserve material color, alpha mode, depth behavior, and shadow
  compatibility for the selected first rendering scope.
- Add regression tests around glyph readiness, backend swaps, font
  changes, cache misses, cache invalidation, and `WorldTextReady`
  timing.

Exit criteria: existing panel/world text examples can opt into Slug and
keep layout behavior stable under the accepted first-scope constraints.

### Phase 6: quality and robustness

- Test EB Garamond, JetBrains Mono, Noto Sans, Liberation Sans, and
  Crimson Text.
- Include small text, large text, oblique world text, high zoom, dense
  CJK glyphs, and fallback CJK strings.
- Compare screenshots against MTSDF at 32, 64, 128, and 256 px
  equivalent sizes.
- Measure CPU preprocessing cost, GPU storage size, draw count,
  fragment cost, upload cost, and first-render latency.

Exit criteria: Slug has documented quality/performance envelopes and
known cases where it is better or worse than MTSDF.

### Phase 7: effects

- Add hard drop shadow as a second glyph pass.
- Decide whether true outlines are worth implementing in
  `bevy_diegetic` or should remain out of scope.
- If true outlines proceed, add contour-offset preprocessing and
  separate outline glyph cache entries.

Exit criteria: shadow is production-quality; outline has either a clear
implementation path or a documented decision to defer.

## Test matrix

- `slug_geometry` unit tests: outline loading, line-to-quadratic
  encoding, cubic conversion or rejection, winding, band sorting, and
  horizontal/vertical edge cases.
- Slug cache unit tests: `SlugGlyphKey`, font-generation invalidation,
  effect-profile invalidation, storage-profile invalidation, and failed
  glyph states.
- Shared shaping tests: render shaping vs measurement for weight,
  slant, letter spacing, word spacing, font features, multi-line text,
  spaces, and invisible glyphs.
- Placement tests: per-glyph origin, bounds, line metrics, anchor
  offsets, panel clip results, and world-scale positions compared to the
  current renderer within explicit tolerances.
- Readiness tests: queued, pending, ready, failed, backend swap, font
  change, cache miss recovery, and `WorldTextReady` timing.
- Integration example: `examples/slug_text.rs` for manual Slug vs MTSDF
  comparison.
- Optional screenshot tests: final integration evidence only, with
  explicit update rules.

## Reviewed decisions

Team review and adhoc review outcomes are recorded in the decision log
below.

## Non-goals for the first pass

- Replacing MTSDF as the default renderer.
- Importing the commercial Slug font converter format.
- Implementing full rich-text effects.
- Supporting arbitrary SVG/path rendering.
- Optimizing band packing before correctness is established.

## Suggested first branch

Create a dedicated branch/worktree for a spike:

```text
feature/slug-text-backend
```

Start with a debug-only path and one example. Keep the public API
experimental until shader parity, cache behavior, and panel/world text
integration are all understood.

## Decision log

Team review ran against this plan and the current `bevy_diegetic`
renderer on 2026-05-20. Mechanical feedback has been folded into the
sections above. Directional decisions are recorded here as they are
reviewed.

1. **Fallback scope:** Slug should always render from the exact font
   face that parley used for shaping. The implementation should extend
   the shaped glyph/run boundary as needed so the renderer receives the
   resolved font face and glyph ID together, rather than assuming the
   requested style font is the rendered face.
2. **Storage and pipeline strategy:** Treat the first Slug branch as a
   feasibility study. Match the upstream Slug shader/data layout as
   closely as practical before optimizing or redesigning storage for
   Bevy. Try the existing Bevy material path first, but the priority is
   proving the Slug algorithm works correctly.
3. **Curve support:** Phase 1 can be TrueType/quadratic-only and should
   fail clearly on unsupported cubic outlines. This is only a
   feasibility-study boundary; cubic outline support remains a future
   extension once the reference-like quadratic path works.
4. **First rendering scope:** Keep the feasibility renderer separate
   from the production text material path. Start with a bare-bones
   standalone example that proves Slug fill coverage and glyph
   placement. PBR lighting, prepass/shadow proxy behavior, OIT, stable
   transparency, and panel render-to-texture parity are later
   integration milestones.
5. **Effects scope:** Defer all effects until plain Slug text renders
   correctly. Hard drop shadows, true outlines, glow, and blur should
   not be part of the first feasibility target.
6. **Module isolation:** Keep the Slug feasibility implementation in a
   separate private module and standalone example until text rendering
   works. Do not reshape the production renderer or shared module
   structure before the feasibility study proves the algorithm.
7. **Run data and cache granularity:** Start with per-entity
   `SlugTextRun` data rather than a shared run cache. The run is a small
   ordered list of shaped glyph instances: glyph ID, origin/advance,
   bounds contribution, and a key/reference to reusable packed glyph
   data. The important shared cache is the glyph-level cache,
   `(font identity, glyph id) -> packed curve/band data`. Do not add
   whole-run or word-level caching until profiling shows shaping or run
   construction is a real cost. Avoid word-level caching by default
   because shaping can depend on neighboring text, font features, script,
   and wrapping.
8. **Glyph cache key and unsupported glyphs:** Cache packed Slug glyph
   data by `(font identity, glyph id)`, where font identity is one
   stable identifier for the resolved font face (for example a registry
   ID, asset handle, or font-bytes hash/generation). Do not cache by
   character. Parley shaping returns glyph IDs, and glyphs are not always
   one-to-one with Rust `char` values: ligatures, combining marks,
   contextual script forms, and emoji sequences can all differ. The first
   Slug backend only targets monochrome outline glyphs. Emoji/color glyph
   formats should be treated as unsupported for Slug and routed to an
   existing/fallback renderer later.
9. **Example run construction:** Update the `slug_text` example so it
   builds one `SlugTextRun` for `Typography` and then uses that run to
   drive the current simple per-glyph rendering path. This is a small
   bridge step, not a phase by itself, and should be bundled with the
   initial glyph-cache/data-shape work.
10. **GPU shape timing:** Defer the final GPU representation decision.
    Do not choose between a run-level material/buffer, instanced glyph
    quads, or another batching shape while the work is still example-only.
    First build the CPU run data and glyph cache, then use the resulting
    data and visible bottlenecks to choose the smallest GPU change that
    removes the worst current inefficiency.

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

## WorldText behavior compatibility

Slug must replace the glyph silhouette source, not the public text
behavior contract. The current `WorldText` and panel text feature set is
valuable, but Slug does not need to copy the exact MTSDF implementation
mechanics when a simpler design gives the same behavior.

The Slug model should be reusable run data plus one or more render
passes:

- a shaped `SlugTextRun` stores glyph positions
- packed Slug glyph data stores curves, bands, and glyph records
- each pass chooses how to use that run: visible, shadow-casting, or
  both

That pass model must preserve the existing choices:

- visible render mode: `Invisible`, `Text`, `PunchOut`, and
  `SolidQuad`
- shadow mode: `None`, `Text`, `PunchOut`, and `SolidQuad`

The same Slug run data can serve those policies:

- `Text`: evaluate Slug coverage and draw the glyph fill.
- `PunchOut`: evaluate Slug coverage and draw the inverse inside the
  glyph quad.
- `SolidQuad`: draw the glyph quad without curve evaluation.
- `Invisible`: skip the visible pass while still allowing a shadow pass
  when requested.

Shadow support should be expressed as another pass over the same run
data. Text and punch-out shadows evaluate Slug coverage in the
shadow/prepass path. Solid-quad shadows can use the quad geometry. `None`
suppresses shadow casting. This keeps run-level GPU storage compatible
with the current feature matrix without forcing Slug to preserve every
internal MTSDF mesh/proxy detail.

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

### Phase 3: run-level Slug GPU data

Status: completed.

Completed:

- Replaced the example's one-material, one-curve-buffer,
  one-band-buffer-per-glyph path with one run-level render object.
- Built one mesh for the shaped run, with one quad per glyph.
- Packed all unique glyph curve records for the run into one curve
  buffer.
- Packed all unique glyph band records for the run into one band buffer.
- Added a glyph table that maps each glyph instance to its packed
  curve/band ranges, bounds, and glyph-local transform data.
- Updated the WGSL shader so each quad selects the right glyph record
  from the run-level data.
- Kept render mode and future shadow-pass mode as explicit inputs to
  the run-level path so `Text`, `PunchOut`, `SolidQuad`, `Invisible`,
  and all current shadow modes remain representable.
- Kept the `WorldText` contrast overlay in the example.

Exit criteria: met. The isolated `slug_text` example renders `Typography`
from one run-level Slug mesh/material/storage set, not one material and
storage pair per glyph. Visual output matches Phase 2, and the data
layout still supports the current visible/shadow mode matrix.

### Phase 4: shared text prerequisites

Status: completed.

Completed:

- `shape_text_cached` is aligned with the parley measurement path for
  the relevant text style inputs currently used by `LayoutTextStyle`.
- `ShapedGlyph` carries a resolved font/face identity from parley:
  requested font id, font blob identity, and collection face index.
- Add a font-face resolver that maps the shaped glyph face identity back
  to exact font bytes plus face index for atlas rasterization and Slug
  outline extraction.
- Define renderer-neutral positioned glyph instances.
- Split shared placement data from atlas `GlyphQuadData` so panel and
  world text can feed either distance-field quads or Slug glyph
  instances.
- Define backend-neutral lookup/readiness states that cover queued,
  pending, ready, invisible, and failed work across atlas rasterization,
  Slug preprocessing, Slug upload, and backend fallback.
- Keep visible render mode and shadow mode renderer-neutral so Slug and
  distance-field backends share the same behavior contract.
- Keep anchoring and debug bounds tied to the resolved glyph face rather
  than assuming the requested font id and face index 0.
- Preserve existing distance-field panel/world text rendering through
  the refactored front half.

Exit criteria: met. Existing distance-field panel and world text still
render through the refactored shared front half, with measured and
rendered advances sharing the same parley shaping style inputs.

### Retrospective

**What worked:**

- `ShapedGlyph` now carries `ResolvedFontFace`, including the requested
  font id, parley blob id, and collection face index.
- `shape_text_cached` now applies weight, slant, letter spacing, word
  spacing, line height, and font features before collecting glyphs.
- `FontRegistry` can resolve the parley face identity back to the exact
  registered font bytes and collection face index.
- Panel and world text now build distance-field quads from a shared
  positioned glyph boundary instead of looking up atlas glyphs directly
  from the requested style font.
- Readiness now distinguishes invisible and failed glyph outcomes from
  ordinary pending atlas work.

**What deviated from the plan:**

- The renderer-neutral instance type is still intentionally small:
  `PositionedGlyph` plus `GlyphQuadPlacement`, not a production backend
  request object.
- Distance-field atlas quads still exist where meshes are built. Phase 4
  only moved the boundary before atlas UVs are attached.

**Surprises:**

- The current shared shaped run already gives enough data to begin
  separating the text front half from the renderer back half.
- Failed resolved-face lookup needs to clear stale text meshes and
  pending markers, not just skip new quad output.

**Implications for remaining phases:**

- Phase 5 can use `PositionedGlyph`, `ResolvedFontData`, and
  backend-neutral readiness as the starting point for a real Slug
  backend resource.
- Phase 5 still needs Slug-specific cache keys, upload tracking, and
  renderer selection; Phase 4 only prepared the shared front half.

### Phase 4 Review

- Phase 4 now records the completed shaping-parity and resolved-face
  checkpoint separately from the remaining renderer-neutral contract.
- Phase 4 now requires a face-to-font-byte resolver before Slug outline
  extraction moves into the backend.
- Phase 4 now makes lookup/readiness backend-neutral instead of
  atlas-shaped.
- Phase 4 now names the shared placement split before Phase 5 backend
  integration.
- Phase 4 now keeps anchoring and debug bounds tied to the resolved
  glyph face.
- Phase 5 now updates the existing `slug_text` example instead of
  adding another comparison example.
- Phase 6 now requires Slug clipping to preserve glyph-local
  coordinates for both overlap/padding trims and panel scissor clips,
  instead of reusing atlas UV mutation semantics.

### Phase 5: Slug backend resource

Status: completed for the isolated Slug backend boundary. Production
panel/world routing remains Phase 6.

Completed:

- Added an internal backend decision point in
  `crates/bevy_diegetic/src/render/text_backend.rs`.
- Added `TextRendererBackend { DistanceField, Slug }` and
  `TextRendererPreference { backend }` behind the experimental feature.
  The first selector is a global resource used by the existing render
  modules and `examples/slug_text.rs`; per-text backend switching stays
  out of this phase.
- Added a `SlugBackend` resource for the isolated path. It owns the
  reusable glyph cache, backend generation, completion count, failure
  count, and preprocessing version.
- Added a Slug-owned completion signal. The current isolated example
  triggers it after CPU preprocessing. Phase 6 will connect the signal
  to production pending-text retries when panel/world text can opt into
  Slug.
- Prepared the first Slug backend path with Parley fallback disabled. If a
  Slug text request would need fallback, detect that as an explicit
  unsupported/missing-glyph state instead of silently receiving an
  unregistered fallback face.
- Treated the first Slug backend as TrueType/quadratic-outline only. If a
  selected registered font or glyph cannot be represented by that path,
  report a clear unsupported-text state. Do not drop glyphs and do not
  mix Slug with MTSDF inside one text run.
- Extended `SlugGlyphKey` with a preprocessing version so changes to
  curve/band preprocessing invalidate cached glyph data.
- Updated the existing `examples/slug_text.rs` so it exercises real
  backend selection while still comparing Slug and MTSDF output.

Exit criteria: met for the isolated backend boundary. The existing
`slug_text` example selects `TextRendererBackend::Slug`, prepares the
`Typography` run through `SlugBackend`, reuses the backend-owned glyph
cache, and emits a Slug completion signal. The production panel/world
render systems still default to distance-field rendering until Phase 6
adds opt-in Slug routing.

### Retrospective

**What worked:**

- `TextRendererPreference` gives the renderer a single backend decision
  point without disturbing the current distance-field path.
- `SlugBackend` moved reusable glyph data out of `SlugBuiltTextRun`, so
  the run now references backend-owned packed glyph data.

**What deviated from the plan:**

- Phase 5 stayed isolated to `slug_text` and shared backend selection.
  Panel and world text still do not route through Slug.
- Slug completion is a Bevy event the example triggers manually after
  CPU preprocessing. It is not yet connected to production pending-text
  retries.

**Surprises:**

- The existing example could use the backend resource without changing
  the run-level shader or material layout.
- The fallback-disabled path needed an explicit font cmap precheck so
  parley cannot silently choose an unregistered fallback face.

**Implications for remaining phases:**

- Phase 6 needs the production route from shared positioned glyphs to
  Slug run data; the backend resource exists, but panel/world systems
  still build atlas quads.
- Phase 6 needs to connect `SlugBackendCompleted` to the same pending
  text retry behavior currently driven by atlas events.
- Phase 6 should decide how Slug render entities are spawned for
  visible and shadow passes before adding example opt-ins.

### Phase 5 Review

- Phase 6 now starts with production routing from `PositionedGlyph` and
  `ResolvedFontData` into Slug run data before parity work.
- Phase 6 now names `SlugBackendCompleted` as the wakeup path for
  pending Slug text.
- Phase 6 now moves GPU storage allocation/upload ownership into the
  production Slug backend path instead of leaving it in examples.
- Phase 6 now separates shadow-pass representation from Phase 8 shadow
  effect quality.
- Phase 6 cache invalidation tests now target the key dimensions that
  exist today; storage and effect profiles remain deferred.
- Phase 7 CJK testing now stays inside the current TrueType/quadratic
  scope until the post-transition unsupported-text review decides CFF
  and cubic support.

### Phase 6: panel and world text parity

- Route shared `PositionedGlyph` data into Slug run data when
  `TextRendererPreference` selects `TextRendererBackend::Slug`.
  Distance-field rendering keeps building atlas `GlyphQuadData`.
- Bridge production `PositionedGlyph.font` / `ResolvedFontData` to
  `SlugFontKey` and exact font bytes. Do not reuse the example-only
  font-family shaping path for production Slug text.
- Produce clear unsupported-run diagnostics when the selected font face
  or glyph cannot use the current Slug path.
- Move Slug GPU storage allocation, upload handles, dirty tracking, and
  lookup state into the production Slug backend path. The example may
  keep local setup only as a manual comparison target.
- Connect `SlugBackendCompleted` to pending text retries, matching the
  role atlas swap events play for distance-field text.
- Define Slug render entity spawning for visible and shadow passes.
  Phase 6 should preserve the existing visible/shadow mode matrix as
  pass representation; Phase 8 owns production-quality shadow effects
  and tuning.
- Support Slug clipping through representations that preserve
  glyph-local coordinates for both overlap/padding trims and panel
  scissor clips.
- Support `WorldText` anchoring from Slug/native glyph bounds, without
  depending on distance-field atlas metrics, and preserve
  backend-neutral readiness behavior.
- Preserve material color, alpha mode, depth behavior, and the full
  visible/shadow mode matrix:
  `Invisible`, `Text`, `PunchOut`, `SolidQuad`, and shadow
  `None`, `Text`, `PunchOut`, `SolidQuad`.
- Add regression tests around glyph readiness, backend swaps, font
  changes, cache misses, current cache-key invalidation dimensions,
  and `WorldTextReady` timing. Defer storage-profile and effect-profile
  invalidation tests until those profiles exist.

Exit criteria: existing panel/world text examples can opt into Slug and
keep layout behavior stable under the accepted first-scope constraints.

### Phase 7: quality and robustness

- Test EB Garamond, JetBrains Mono, Noto Sans, Liberation Sans, and
  Crimson Text.
- Include small text, large text, oblique world text, high zoom, dense
  CJK glyphs from explicitly selected registered TrueType/quadratic CJK
  fonts, and later fallback strings once fallback support is
  deliberately enabled. Broad CJK testing for CFF/cubic fonts waits for
  the post-transition unsupported-text review.
- Compare screenshots against MTSDF at 32, 64, 128, and 256 px
  equivalent sizes.
- Measure CPU preprocessing cost, GPU storage size, draw count,
  fragment cost, upload cost, and first-render latency.

Exit criteria: Slug has documented quality/performance envelopes and
known cases where it is better or worse than MTSDF.

### Phase 8: effects

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
11. **WorldText behavior compatibility:** Run-level Slug rendering
    should replace the glyph silhouette source, not the visible/shadow
    behavior contract. The design must preserve the existing separation
    between visible render mode (`Invisible`, `Text`, `PunchOut`,
    `SolidQuad`) and shadow mode (`None`, `Text`, `PunchOut`,
    `SolidQuad`). Phase 3 is now the run-level GPU data step, because
    it proves that Slug can render a coherent text run while still
    carrying those behavior choices forward. Shared production
    prerequisites and backend integration follow after that proof.
12. **Run-level GPU shape:** Phase 3 uses one mesh and one material for
    a shaped Slug run, plus combined curve, band, and glyph-record
    storage buffers. Each quad carries a glyph-record index, and the
    shader uses that record to select the correct bounds and band range.
    This removes the per-glyph material/storage path without committing
    the production backend to a final batching strategy.
13. **Phase 5 backend boundary:** Phase 4 did not create the full
    renderer abstraction; it prepared the shared data needed for one.
    Phase 5 should create the internal backend decision point in
    `render/text_backend.rs`, starting with a global experimental
    `TextRendererPreference` resource. Panel and world text should route
    after parley shaping: distance-field continues to build atlas quads,
    while Slug builds Slug run/glyph GPU data. Per-text style switching
    is deferred until the backend path works.
14. **Slug readiness wakeup:** Slug needs its own completion signal for
    outline preprocessing, curve/band packing, GPU storage allocation,
    and uploads. Public `WorldTextReady` remains backend-neutral, but
    Slug pending text must be retried from Slug backend completion
    events rather than atlas completion or atlas swap events.
15. **Slug fallback scope:** The first Slug backend should shape with
    Parley fallback disabled. Missing glyphs or text that would require
    fallback become explicit unsupported states with clear diagnostics.
    CJK testing should use explicitly registered CJK fonts. Fallback can
    be enabled later only after the backend can detect fallback use and
    resolve every fallback face through `FontRegistry`.
16. **Slug glyph scope and anchoring:** The first Slug backend targets
    TrueType/quadratic outline glyphs only. Other font/glyph
    representations are outside the current scope and should produce a
    clear unsupported-text state. The backend should not drop glyphs or
    mix Slug with MTSDF inside one run. Slug world text anchoring should
    use Slug/native glyph bounds rather than distance-field atlas
    metrics.

## Post-transition review

After Slug can replace the current distance-field path for the migrated
examples, run a review dedicated to unsupported text cases and decide the
next implementation steps.

Review questions:

- Should cubic/CFF outlines be converted to quadratics, handled directly
  in the Slug shader path, or deferred?
- Should fallback be enabled again, and if so how does every fallback
  face become registered and resolvable through `FontRegistry`?
- How should color emoji and other non-monochrome glyphs be represented:
  Slug extension, separate renderer, or explicit unsupported state?
- Do unsupported glyphs still fail the whole run, or is there a proven
  need for a mixed renderer after the main Slug path is working?
- Which examples and benchmarks prove the next scope: typography,
  world text, units, CJK, emoji, fallback strings, or dense paragraph
  text?

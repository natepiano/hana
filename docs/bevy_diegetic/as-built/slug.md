# Slug text renderer backend

> **Archived 2026-06-07 — implemented, then promoted.** The experimental
> backend this doc proposes shipped and subsequently became hana's **sole**
> text renderer: the SDF/MSDF/MTSDF distance-field path it was designed to sit
> beside was removed entirely (see [`slug-migration.md`](slug-migration.md)).
> The "backend-neutral" selection machinery described below is therefore gone —
> there is nothing left to select between. Slug code lives in
> `src/text/slug/{glyph,render,runtime,shaders,support}/`; the runtime backend
> module was later renamed `glyph_cache` (a3ed7d1). Effect-support analysis
> that motivated the promotion is in [`../slug_fx.md`](../slug_fx.md).

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

- `TextRenderer`: chooses the text rendering model, initially
  `DistanceField` or `Slug`.
- `DistanceField`: describes the atlas texel encoding inside the
  current renderer: `Sdf`, `Msdf`, or `Mtsdf`.

Experiment history lives in `docs/bevy_diegetic/performance/slug-experiments.md`.
Check that document before retrying shader or packing optimizations.
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

Initial code should stay in a private Slug spike module and a standalone
example. This branch now compiles Slug unconditionally to keep testing
simple; if the experiment fails, remove the branch code instead of
carrying feature-gate ceremony.

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
pub enum TextRenderer {
    DistanceField,
    Slug,
}

pub struct TextRendererPreference {
    pub renderer: TextRenderer,
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

The long-term shaping boundary should carry resolved font identity per
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
- cubic segments currently use adaptive cubic-to-quadratic conversion;
  keep the error bound explicit and covered by fixtures before claiming
  broad font support

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

The renderer swap contract is behavioral, not architectural. Users
should be able to choose the distance-field renderer or the Slug
renderer at runtime and keep the same public text capabilities. The two
renderers do not need matching internals, mesh layouts, material
structures, alpha-mode choices, shadow passes, or cache layouts.
`WorldText` supports a per-entity renderer override, so apps can spawn
one text entity with Slug and another with the distance-field renderer in
the same scene while still falling back to the global preference when no
override is set.
Panel text supports the same override on `LayoutTextStyle`, so a panel
can mix `b.text(..., LayoutTextStyle::new(...).with_renderer(...))`
entries without forcing the whole app to one renderer.

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

- visible render mode: `Text` and `PunchOut`
- shadow mode: `None` and `Cast`

The same Slug run data can serve those policies:

- `Text`: evaluate Slug coverage and draw the glyph fill.
- `PunchOut`: evaluate Slug coverage and draw the inverse inside the
  glyph quad.
- ghost text: use `GlyphShadowMode::Cast` with a fill color alpha of `0` so
  the color pass paints nothing while the shadow pass casts the glyph
  silhouette.

Shadow support should be expressed as another pass over the same run
data. Cast shadows evaluate Slug coverage in the shadow/prepass path.
`None` suppresses shadow casting. This keeps run-level GPU storage compatible
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

- Created a private experimental Slug module for the feasibility path.
- Added standalone `examples/slug_text.rs`.
- Kept the code separate from production panel/world text modules.
- Avoided `TextRenderer`, shared glyph instance types, and
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
- Added a CJK probe using bundled open-source Noto Sans CJK assets.
  Later phases added adaptive cubic-to-quadratic conversion so CFF/CFF2
  outlines can enter the Slug path instead of failing at the first
  quadratic-only boundary.
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
- Defer the final GPU representation decision until this data layout
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
  the run-level path so `Text`, `PunchOut`, and the current `None`/`Cast`
  shadow modes remain representable.
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
  atlas-specific.
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
- Added `TextRenderer { DistanceField, Slug }` and
  `TextRendererPreference { renderer }`. The first selector is a global
  resource used by the existing render modules and `examples/slug_text.rs`;
  per-text backend switching stays out of this phase.
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
- Treated the first Slug backend as TrueType/quadratic-outline only.
  Later phases added adaptive cubic-to-quadratic conversion for CFF/CFF2
  outlines. Any remaining unsupported font or glyph format should still
  report a clear unsupported-text state. Do not drop glyphs and do not
  mix Slug with MTSDF inside one text run.
- Extended `SlugGlyphKey` with a preprocessing version so changes to
  curve/band preprocessing invalidate cached glyph data.
- Updated the existing `examples/slug_text.rs` so it exercises real
  backend selection while still comparing Slug and MTSDF output.

Exit criteria: met for the isolated backend boundary. The existing
`slug_text` example selects `TextRenderer::Slug`, prepares the
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

- Phase 6 added the production route from shared positioned glyphs to
  Slug run data.
- Phase 6 connected `SlugBackendCompleted` to pending text retry
  behavior.
- Phase 6 defined how Slug render entities are spawned for visible and
  shadow passes before broader example opt-ins.

### Phase 5 Review

- Phase 6 now starts with production routing from `PositionedGlyph` and
  `ResolvedFontData` into Slug run data before parity work.
- Phase 6 now names `SlugBackendCompleted` as the wakeup path for
  pending Slug text.
- Phase 6 now moves GPU storage allocation/upload ownership into the
  production Slug backend path instead of leaving it in examples.
- Phase 6 now separates shadow-pass representation from Phase 12 shadow
  effect quality.
- Phase 6 cache invalidation tests now target the key dimensions that
  exist today; storage and effect profiles remain deferred.
- Phase 8 CJK testing now includes the adaptive cubic-to-quadratic path.
  The post-transition unsupported-text review should decide whether that
  conversion is sufficient or whether direct cubic handling is needed.

### Phase 6: world text opt-in routing

Status: completed for `WorldText` opt-in routing. Panel text routing
was completed in Phase 7 because panel text is batched into panel render
targets and needed a separate Slug batching path.

Completed:

- Route shared `PositionedGlyph` data into Slug run data when
  `TextRendererPreference` selects `TextRenderer::Slug`.
  Distance-field rendering keeps building atlas `GlyphQuadData`.
- Bridge production `PositionedGlyph.font` / `ResolvedFontData` to
  `SlugFontKey` and exact font bytes. Do not reuse the example-only
  font-family shaping path for production Slug text.
- Produce clear unsupported-run diagnostics when the selected font face
  or glyph cannot use the current Slug path.
- Added `WorldText` Slug mesh spawning using the same run-level mesh and
  storage buffers proven by the isolated example.
- Added a `SlugBackendCompleted` observer that marks panel and world
  text pending, matching the role atlas events play for distance-field
  text.
- Added backend-preference change handling that marks existing text
  pending when the global backend changes.
- Defined `WorldText` Slug visible and shadow-pass spawning so the
  existing visible/shadow mode matrix remains representable. Phase 12
  owns production-quality shadow effects and tuning.
- Updated `examples/world_text.rs` to opt into Slug through
  `TextRendererPreference::slug()`.
- Kept `examples/slug_text.rs` as the manual Slug-vs-MTSDF comparison
  target so its contrast overlay remains useful.

Remaining:

- Move Slug GPU storage allocation, upload handles, dirty tracking, and
  lookup state fully into the production Slug backend instead of
  allocating per spawned `WorldText` mesh.
- Add Slug routing and batching for panel text render targets.
- Add Slug clipping for panel overlap/padding trims and panel scissor
  clips.
- Support Slug clipping through representations that preserve
  glyph-local coordinates for both overlap/padding trims and panel
  scissor clips.
- Support `WorldText` anchoring from Slug/native glyph bounds, without
  depending on distance-field atlas metrics, and preserve
  backend-neutral readiness behavior.
- Preserve material color, alpha mode, depth behavior, and the current
  visible/shadow mode matrix: visible `Text`/`PunchOut` and shadow
  `None`/`Cast`.
- Add regression tests around glyph readiness, backend swaps, font
  changes, cache misses, current cache-key invalidation dimensions,
  and `WorldTextReady` timing. Defer storage-profile and effect-profile
  invalidation tests until those profiles exist.

Exit criteria: partially met. `WorldText` examples can opt into Slug and
keep distance-field rendering available as the default. Panel examples
still use distance-field text until the panel Slug batching path exists.

### Retrospective

**What worked:**

- `TextRendererPreference::slug()` now routes production `WorldText`
  through `SlugBackend::prepare_positioned_run`.
- Existing `WorldText` visible/shadow modes stayed representable because
  Slug spawning mirrors the existing visible and shadow proxy split.

**What deviated from the plan:**

- Panel text did not move to Slug in this phase. Panel text is batched into
  panel render targets, so it needs its own Slug batching path instead of
  reusing the `WorldText` mesh path.
- Slug GPU buffers still allocate per spawned `WorldText` mesh. Backend-owned
  upload state remains planned work.

**Surprises:**

- Production routing needed `PositionedGlyph.font` and `ResolvedFontData`
  access, not the example-only font-family path.
- The existing render system was already large enough that adding Slug
  routing required splitting `WorldText` rendering into backend-specific
  helpers.

**Implications for remaining phases:**

- Phase 7 started with backend-owned Slug GPU storage before panel
  batching, so panels did not inherit per-mesh buffer allocation.
- Phase 11 keeps panel routing documented as a separate render-target
  batching task, not as a small extension of `WorldText` spawning.
- Quality/performance work must include production `WorldText` opt-in examples
  and keep the isolated Slug-vs-MTSDF comparison example.

### Phase 6 Review

- Phase 7 now covers backend-owned GPU storage, panel Slug batching, and
  clipping before broad quality work.
- Phase 8 covered quality and robustness evidence after panel routing
  existed.
- Phase 9 isolated Slug visual quality, Phase 10 targets pixel-parity
  shader performance, Phase 11 returns to panels, and Phase 12 treats
  shadows as production tuning over the existing proxy paths.
- Immediate cache tests now target the current `SlugGlyphKey` fields.
  Font-generation, storage-profile, and effect-profile invalidation wait
  until those fields or profiles exist.

### Phase 7: backend-owned storage and panel routing

Status: completed.

- Move Slug GPU storage allocation, upload handles, dirty tracking, and
  lookup state into `SlugBackend`.
- Replace per-spawned-`WorldText` `ShaderStorageBuffer` allocation with
  backend-owned Slug run storage references.
- Add Slug data generation for panel text children instead of routing panel
  text through atlas `GlyphQuadData`.
- Add panel Slug batching that can render panel text from Slug run storage.
- Add Slug clipping for panel overlap/padding trims and panel scissor clips
  while preserving glyph-local coordinates.
- Keep backend-neutral readiness behavior for backend swaps, font changes,
  cache misses, and `WorldTextReady` timing.

Exit criteria: `WorldText` and panel text can both opt into Slug through
`TextRendererPreference`, using backend-owned Slug storage rather than
per-mesh buffer allocation.

### Retrospective

**What worked:**

- `SlugBackend` now owns run storage keys and GPU handles for meshes,
  curve buffers, band buffers, and glyph buffers.
- Panel text now has a Slug route through `PanelSlugTextRun` and
  `build_panel_slug_meshes`, so it no longer needs atlas `GlyphQuadData`
  when `TextRendererPreference::slug()` is active.
- Slug clipping is handled by clipping the run mesh and UVs before upload,
  preserving glyph-local shader coordinates.

**What deviated from the plan:**

- Panel Slug batching currently spawns one Slug mesh per panel text child
  instead of merging multiple Slug runs into one panel-wide mesh.
- Panel hue offset is still MSDF-only. Slug panel text preserves the text
  fill color but does not yet apply `HueOffset`.

**Surprises:**

- Panel routing needed non-uniform Slug bounds scaling because
  `PanelTextChild` can scale X and Y independently.
- `WorldText` production scaling needed to move into the prepared run so
  backend-owned storage can be reused by the spawn path without a separate
  per-spawn scale.

**Implications for remaining phases:**

- Phase 11 screenshots should include at least one panel example using
  `TextRendererPreference::slug()`.
- Phase 11 performance notes should distinguish backend-owned storage from
  still-unmerged panel Slug meshes.
- Phase 12 should include panel hue-offset behavior in the effects/parity
  review if Slug is still intended to replace MSDF.

### Phase 7 Review

- Phase 8 treated panel evidence as required because panel Slug routing
  exists.
- Phase 8 measured the current per-child panel Slug mesh route and kept
  the merged panel-level batching decision for Phase 11.
- Phase 8 included backend-owned storage lifetime, invalidation, and
  cleanup behavior under repeated text changes and backend swaps.
- Phase 8 decided whether `SlugBackendCompleted` remains a production
  wakeup contract or waits for async Slug work.
- Phase 8 added explicit clipping validation for overlap, padding,
  scissor, partial-glyph, and non-uniform panel scale cases.
- Phase 12 narrows shadow work to validation and tuning of the existing
  `WorldText` and panel Slug proxy paths.
- Phase 12 includes a panel `HueOffset` parity decision for Slug text.

### Phase 8: quality and robustness

Status: completed.

- Test EB Garamond, JetBrains Mono, Noto Sans, Liberation Sans, and
  Crimson Text.
- Include small text, large text, oblique world text, high zoom, dense
  CJK glyphs from explicitly selected registered CJK fonts, and later
  fallback strings once fallback support is deliberately enabled. CFF/CFF2
  outlines now enter through adaptive cubic-to-quadratic conversion, so
  broad CJK testing should measure conversion quality and cost.
- Compare screenshots against MTSDF at 32, 64, 128, and 256 px
  equivalent sizes.
- Start with `WorldText` opt-in screenshots and measurements using
  `examples/world_text.rs` and `examples/slug_text.rs`.
- Add panel screenshots and measurements using at least one panel example
  with `TextRendererPreference::slug()`.
- Validate panel clipping for overlap, padding trims, scissor clips,
  partial glyph clips, and non-uniform X/Y panel scale.
- Measure CPU preprocessing cost, backend-owned GPU storage size, draw
  count, fragment cost, upload cost, and first-render latency.
- Measure the current per-child panel Slug mesh route and decide whether
  merged panel-level Slug batching is required before final draw-count
  claims.
- Measure backend-owned storage lifetime, invalidation, and cleanup under
  repeated text changes and backend swaps.
- Decide whether `SlugBackendCompleted` remains a production wakeup
  contract now or waits for async Slug work.

Exit criteria: Slug has documented quality/performance envelopes and
known cases where it is better or worse than MTSDF.

### Retrospective

**What worked:**

- Added a Slug run storage profile API. A live `slug_text` run for
  `Typography` measured 10 glyph instances, 8 unique glyphs, 40 mesh
  vertices, 60 indices, 970 curve records, 256 band records, and 35,392
  curve/band/glyph record bytes before GPU alignment.
- Added tests for storage-key reuse/removal, run storage profiling,
  partial clip mesh trimming, fully clipped glyph removal, and the Phase
  8 Latin font matrix.
- Captured BRP screenshot evidence from `crates/bevy_diegetic/examples/slug_text.rs`
  at `/tmp/slug_text_phase8_wide.png`; the example home frame now shows
  the whole word instead of cropping the run.

**What deviated from the plan:**

- The first CJK fixture exposed the quadratic-only boundary. The current
  branch now includes adaptive cubic-to-quadratic conversion and expects
  `NotoSansCJKsc-Regular.otf` to render; dense CJK quality and
  performance evidence still need to be gathered.
- Phase 8 measured the isolated `slug_text` path and added code-level
  panel clipping/storage coverage. Full panel screenshots and merged
  panel draw-count measurements remain for the panel-specific follow-up.
- Temporarily running `panel_rendering` with `TextRendererPreference::slug()`
  exposed a production routing gap: many panel glyphs failed with
  `font does not contain glyph id 821`, so the existing panel example
  remains on the distance-field renderer until Slug resolves the exact
  face/glyph identity used by panel text.

**Surprises:**

- The `Typography` run reuses packed glyph data as expected: 10 glyph
  instances collapse to 8 unique glyph records.
- Backend-owned storage needed explicit cleanup. Slug mesh children now
  carry the run storage key, panel rebuilds remove old run storage by
  key, and world-text Slug rebuilds clear backend run storage before
  uploading the replacement run.

**Implications for remaining phases:**

- Slug is currently a strong fit for Latin TrueType/quadratic text:
  JetBrains Mono, Noto Sans, EB Garamond, Crimson Text, and Liberation
  Sans all prepare `Typography` successfully.
- Slug is not yet ready as a full MSDF replacement for emoji, fallback
  fonts, or panel text. CJK/CFF/CFF2 now has an implementation path, but
  still needs quality and performance proof before replacement claims.
- Phase 9 returned to the isolated `slug_text` example and moved the
  quality question back to the Slug shader instead of panel
  infrastructure.
- Phase 10 should keep that exact output and reduce shader cost before
  more panel work. The panel example mixes layout, clipping, picking,
  layers, and camera controls, so it is the wrong place to judge Slug
  shader changes.
- Separate follow-up work should decide the final production cache policy
  for backend-owned storage and whether the current cubic conversion is
  sufficient before any MSDF removal decision.

### Phase 8 Review

- Phase 9 became the isolated Slug quality gate instead of a panel
  readiness phase.
- Phase 9 recorded the panel findings as deferred follow-up: no-outline
  glyphs such as space must be skipped, panel text must stay off RTT as a
  Slug quality path, and the panel example exposed clipping/picking/layer
  issues that should not drive shader-quality decisions.
- Phase 10 now covers pixel-parity Slug shader performance. It must keep
  exact current output while reducing fragment cost.
- Phase 11 now covers panel Slug readiness, including the panel
  screenshot evidence, current per-child Slug mesh measurement, and the
  merged-batching decision before final panel performance claims.
- Phase 11 now records the future interactive-panel constraint: batching
  must not prevent per-element behavior for buttons, sliders, dropdowns,
  or similar panel controls.
- Phase 11 now includes production storage lifetime policy work for
  backend-owned Slug run storage.
- Phase 12 is now the effects phase. It stays focused on validating and
  tuning existing shadow proxy paths, and names the concrete Slug
  `HueOffset` implementation hooks.
- The test matrix now records that dense CJK quality tests should prove
  the adaptive cubic-to-quadratic path with the bundled CJK font.

### Phase 9: isolated Slug quality gate

Status: completed for isolated quality baseline and benchmark
instrumentation. Pixel-perfect shader performance remains Phase 10.

Completed:

- Kept the quality work in `examples/slug_text.rs`: direct Slug mesh
  rendering, camera controls, large text, small text, and no panels as
  the primary quality surface.
- Updated the example so `Typography` can be compared through both the
  distance-field and Slug renderers at matching size and placement.
- Added Slug as a selectable `WorldText` renderer in
  `examples/typography.rs`, while carrying Slug glyph metrics through
  the same overlay/debug path.
- Improved Slug glyph packing with horizontal and vertical bands, band
  overlap, padded glyph quads, and no-outline glyph handling.
- Replaced the five-sample fill approximation with non-zero winding plus
  analytic quadratic distance coverage in the Slug shader.
- Kept the Slug material in the lighting/depth path instead of forcing
  an unlit setup.
- Added adaptive cubic-to-quadratic outline conversion for CFF/CFF2
  fonts and updated the font matrix so Noto Sans CJK SC is expected to
  render.
- Added CPU renderer-prep benchmarks comparing SDF, MSDF, MTSDF, and
  Slug.
- Added `examples/text_renderer_gpu_bench.rs` so empty, Slug, SDF, MSDF,
  and MTSDF render-loop costs can be measured in the same scene.

Exit criteria: met for the isolated quality surface and benchmark
instrumentation. The remaining quality/performance work is tied to the
shader algorithm rather than panel infrastructure, and Phase 10 will
capture deterministic baseline screenshots before changing it.

### Phase 10: pixel-parity Slug shader performance

Status: in progress; first pixel-parity optimization pass completed.

- Freeze the current Slug output as the visual baseline before changing
  the shader. Capture deterministic screenshots for large text, small
  text, zoomed edge inspection, and the GPU benchmark scene.
- Keep the exact same visible output. Candidate shader changes must be
  pixel-perfect against the baseline screenshots, or the difference must
  be explained and reviewed before keeping the change.
- Optimize the fragment path without changing the public renderer
  contract: reduce band/curve iteration, storage fetches, branch cost,
  redundant distance work, and avoidable coverage math while preserving
  non-zero winding plus analytic quadratic distance coverage.
- Keep the curve/band data format stable unless a measured improvement
  justifies changing it and the screenshots still match.
- Run the CPU renderer-prep benchmark after shader work to confirm the
  prep path did not regress.
- Run `examples/text_renderer_gpu_bench.rs` for empty, Slug, SDF, MSDF,
  and MTSDF. When available, use command-line Xcode Metal System Trace
  data to compare last-window GPU active and fragment intervals.
- Treat the current Slug fragment cost as the next known bottleneck.
  Recent local Metal traces showed Slug far above the distance-field
  modes in the 720-instance benchmark, so performance work should start
  in the Slug shader before larger renderer architecture changes.

Exit criteria: Slug screenshots are pixel-identical to the pre-phase
baseline, and the render-loop benchmark shows a measurable Slug GPU
fragment-time improvement without worsening CPU prep.

Progress:

- Added a local `scripts/xctrace_text_renderer.sh` wrapper so record and
  export commands share one approved command surface.
- Captured a Slug baseline screenshot at
  `/tmp/slug_phase10_baseline_slug_current.png`.
- Rejected two pixel-perfect shader candidates because same-wrapper Metal
  traces showed slower Slug fragment time.
- Kept the squared-distance candidate: curve loops now compare squared
  distances and take one square root after the nearest curve is known.
  The Slug screenshot remained pixel-identical (`AE 0`).
- Kept CPU-precomputed curve data for the shader: control deltas,
  quadratic second differences, conservative control-point bounds, and
  distance-solver coefficients. These fields increase packed curve bytes
  but avoid repeated fragment work.
- Kept a bounds-distance early return. Pixels farther than the
  antialiasing width from every candidate curve return solid inside or
  transparent outside after the winding test, without running the exact
  quadratic distance solver.
- Rejected a 64-band global packing change because it produced small
  pixel differences against the frozen baseline.
- Rejected a two-dimensional distance-cell grid for this phase. It may
  become useful if reviewed as a quality change, but it did not preserve
  exact current output.
- Rejected a point-space solver rewrite because the Metal trace was
  slower despite pixel-identical output.
- Final Slug screenshot for this pass stayed pixel-identical to the
  baseline: `/tmp/slug_phase10_final_shader.png` compared with
  `/tmp/slug_phase10_baseline_slug_current.png` at `AE 0`.
- Same-wrapper 720-instance Metal traces showed Slug fragment mean moving
  from the original 5.9117 ms to 5.4053 ms. Current same-wrapper
  fragment means: empty 0.2125 ms, Slug 5.4053 ms, SDF 1.9421 ms,
  MSDF 2.0059 ms, MTSDF 1.8055 ms.
- Filtered `renderer_prep` Criterion results still show Slug prep far
  below distance-field prep after the extra packed fields: Slug about
  1.19 ms for both 128 and 256 cases, while SDF/MSDF/MTSDF are tens to
  hundreds of milliseconds in the same filtered run.

### Phase 11: panel Slug readiness

- Keep Slug panel text on the direct mesh path. Do not use panel
  render-to-texture as quality evidence for Slug replacement work; RTT
  stays a legacy/compatibility path, not the target for analytic text.
- Fix or explain the panel Slug face/glyph identity failure found by
  temporarily running `panel_rendering` with
  `TextRendererPreference::slug()`: repeated `font does not contain glyph
  id 821` warnings left the main panel text blank.
- Add a panel example or opt-in mode that renders panel text through Slug
  reliably enough for screenshots and manual inspection.
- Capture panel Slug screenshots that include clipped text, overflow text,
  padding trims, scissor clips, partial-glyph clips, and non-uniform X/Y
  panel scale.
- Measure the current per-child panel Slug mesh route and decide whether
  merged panel-level Slug batching is needed before final draw-count
  claims.
- Preserve future per-element panel behavior while evaluating mesh
  batching. Buttons, sliders, dropdowns, and similar controls may need
  per-element hit testing, visibility, material state, or update routing
  even if Slug rendering eventually merges draw work.
- Replace the temporary world-text clear-all run-storage cleanup with a
  production storage lifetime policy that covers reuse, invalidation,
  backend swaps, text changes, despawn cleanup, and multi-entity scenes.
- Decide whether `SlugBackendCompleted` remains a production wakeup
  contract now or waits for async Slug preprocessing.

Exit criteria: panel Slug text renders reliably enough to inspect and
measure; the remaining panel performance and storage policy decisions are
explicit enough to support effects and replacement work.

### Phase 12: effects

- Validate and tune hard drop shadows using the existing `WorldText`
  Slug shadow proxy path. The basic proxy path already exists; this
  phase is about quality, layers, depth bias, alpha mode, and proxy
  behavior.
- Validate and tune panel Slug shadow proxy quality, layers, depth bias,
  alpha mode, and proxy behavior.
- Decide whether panel `HueOffset` should be implemented for Slug text,
  documented as MSDF-only, or removed from Slug parity expectations. If
  implemented, update `SlugTextUniform`, `slug_text.wgsl`, Slug material
  construction, and the `sync_panel_hue_offset` path.
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
- Dense CJK quality tests should use the bundled Noto Sans CJK SC font
  and prove the adaptive cubic-to-quadratic path for representative Han,
  Japanese, and Korean glyphs.
- Phase 10 pixel-parity tests: compare Slug screenshots before and after
  shader-performance edits at large text, small text, zoomed edge
  inspection, and the GPU benchmark scene.
- Phase 10 performance tests: run the CPU renderer-prep benchmark and
  the render-loop GPU benchmark for empty, Slug, SDF, MSDF, and MTSDF;
  use Metal System Trace GPU intervals when available.
- Slug cache unit tests: `SlugGlyphKey`, current preprocess-version
  invalidation, and failed glyph states. Storage-profile and
  effect-profile invalidation tests wait until those profiles exist.
- Shared shaping tests: render shaping vs measurement for weight,
  slant, letter spacing, word spacing, font features, multi-line text,
  spaces, and invisible glyphs.
- Placement tests: per-glyph origin, bounds, line metrics, anchor
  offsets, panel clip results, and world-scale positions compared to the
  current renderer within explicit tolerances.
- Readiness tests: queued, pending, ready, failed, backend swap, font
  change, cache miss recovery, backend-owned upload/lookup state, and
  `WorldTextReady` timing.
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
3. **Curve support:** Phase 1 started as TrueType/quadratic-only and
   failed clearly on unsupported cubic outlines. The current branch has
   moved past that boundary by converting CFF/CFF2 cubic outlines to
   quadratics adaptively. Keep unsupported states for glyph formats
   outside the current monochrome outline path.
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
   works. Do not modify the production renderer or shared module
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
   initial glyph-cache/data-layout work.
10. **GPU representation timing:** Defer the final GPU representation decision.
    Do not choose between a run-level material/buffer, instanced glyph
    quads, or another batching layout while the work is still example-only.
    First build the CPU run data and glyph cache, then use the resulting
    data and visible bottlenecks to choose the smallest GPU change that
    removes the worst current inefficiency.
11. **WorldText behavior compatibility:** Run-level Slug rendering
    should replace the glyph silhouette source, not the visible/shadow
    behavior contract. The design must preserve the existing separation
    between visible render mode (`Text`, `PunchOut`) and shadow mode
    (`None`, `Cast`). Phase 3 is now the run-level GPU data step, because
    it proves that Slug can render a coherent text run while still
    carrying those behavior choices forward. Shared production
    prerequisites and backend integration follow after that proof.
12. **Run-level GPU representation:** Phase 3 uses one mesh and one material for
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
16. **Slug glyph scope and anchoring:** The first Slug backend targeted
    TrueType/quadratic outline glyphs only. The current branch now adds
    adaptive cubic-to-quadratic conversion for CFF/CFF2 outlines. Other
    font/glyph representations are outside the current scope and should
    produce a clear unsupported-text state. The backend should not drop
    glyphs or mix Slug with MTSDF inside one run. Slug world text
    anchoring should use Slug/native glyph bounds rather than
    distance-field atlas metrics.

## Post-transition review

After Slug can replace the current distance-field path for the migrated
examples, run a review dedicated to unsupported text cases and decide the
next implementation steps.

Review questions:

- Is the current adaptive cubic-to-quadratic conversion sufficient for
  CFF/CFF2 quality and performance, or does Slug need direct cubic
  handling or tighter error controls?
- Should fallback be enabled again, and if so how does every fallback
  face become registered and resolvable through `FontRegistry`?
- How should color emoji and other non-monochrome glyphs be represented:
  Slug extension, separate renderer, or explicit unsupported state?
- Do unsupported glyphs still fail the whole run, or is there a proven
  need for a mixed renderer after the main Slug path is working?
- Which examples and benchmarks prove the next scope: typography,
  world text, units, CJK, emoji, fallback strings, or dense paragraph
  text?

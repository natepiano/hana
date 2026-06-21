# Panel Shape API As-Built Review - 2026-06-10

This document archives what was built for panel-owned line drawing in
`bevy_diegetic`, the decisions that survived implementation, and the direction
chosen after visual review.

It is not the old historical phase plan. Historical "what we thought we might
do" requirements were removed so future implementers can start from the current
system state. The remaining implementation phases are retained as forward work
from that state.

## Current State

Panel-owned line drawing exists from authored API through layout resolution,
render command emission, analytic-path batching, performance counters, and the
`units.rs` ruler example.

The old dedicated panel-line SDF renderer proved the data model, lifecycle, and
batching, but visual review found that its independent SDF/OIT/AA path diverged
from slug text quality at grazing angles. Panel lines now route through the
shared analytic path renderer used by text glyphs and panel-authored vector
marks. The old line material and WGSL file remain on disk as unregistered
fallback/quarantine code.

## Authored API

`PanelDraw` is an element-owned, paint-only visual layer:

- `El::draw(PanelDraw::lines(...))` attaches line draw data to an element.
- `PanelDraw` does not affect measurement.
- Draw-only changes are classified as visual-only.
- Draw data is scaled with the layout tree and excluded from structural layout
  hashing.
- `DrawOverflow::Clipped` is the default.
- `DrawOverflow::Visible` lets a line escape the owning element while still
  respecting inherited panel or ancestor clipping.

`PanelLine` is the authored primitive:

- A line is authored as a centerline from `PanelPoint` start to end.
- Stroke width expands around the centerline during rendering.
- `LineStyle` owns stroke width, color, cap size, start/end `CalloutCap`, and
  an optional `HairlineFade` override (`PanelLine::hairline_fade`; `None`
  inherits the element → panel → global cascade — see the Phase C addendum).
- `CalloutCap` is reused for cap semantics instead of introducing a separate
  panel-only cap model.
- `start_inset` and `end_inset` allow callout-like endpoint adjustment.

`PanelPoint` and `PanelCoord` use panel layout coordinates:

- origin is top-left
- X grows right
- Y grows down
- `Start(value)` measures from the leading edge
- `End(value)` measures inward from the trailing edge
- `Percent(value)` resolves against the owning element size
- negative `End` values intentionally support overflow-capable authoring

Public API entry points are re-exported from the crate root so examples can use
`bevy_diegetic::*`.

## Layout Resolution

The layout engine resolves authored lines after element bounds are known and
emits `RenderCommandKind::Lines`.

Resolved line data includes:

- stable source identity
- resolved panel-space endpoints
- tip and shaft positions after insets and caps
- resolved style values
- owner bounds
- visual bounds
- effective clip
- paint lane
- depth and OIT layering hints
- resolved shaft/cap primitives

Identity is split by layer:

- `PanelShapeSourceKey` comes from layout and identifies the source element, draw
  ordinal, line ordinal, and primitive ordinal.
- `PanelShapeRenderKey` prefixes the source key with the panel entity for
  retained renderer storage.

Element-owned `PanelDraw::Lines(Vec<PanelLine>)` currently uses ordinal
identity. Inserting or reordering lines before an existing line can churn later
retained keys, but stale cleanup keeps output correct. Producers with stronger
semantic identity, such as text metrics or callouts, should provide their own
stable source keys when they become path producers.

Clipping is resolved at layout time:

- clipped draws use owner-bounded clipping
- visible-overflow draws use inherited parent/panel clipping and intentionally
  ignore the owner's own clip
- renderer code consumes the resolved clip instead of reconstructing scissor
  state from the flat command stream

## Current Renderer

The current panel-shape adapter lives under `render/panel_shapes/` and is
registered by `RenderPlugin`.

Implemented pieces:

- `render/batch_key.rs` contains shared visual compatibility keys and material
  interning used by text and line batching.
- `render/panel_shapes/mod.rs` registers the plugin and systems.
- `render/panel_shapes/path.rs` converts groups of resolved shaft/cap primitives
  into one closed multi-contour analytic `PathOutline` plus clipped instance
  rect/UV data.
- `render/panel_shapes/batching.rs` groups same-styled primitives per element
  and routes them into retained cross-panel analytic path batches.
- `render/panel_shapes/primitive.rs` owns stable panel-shape render identity.
- `render/analytic_paths/atlas.rs` owns the generic path atlas used by
  non-glyph path producers.
- `render/analytic_paths/material.rs` / `analytic_path*.wgsl` provide the
  shared analytic coverage, AA, vertex-pulling, and material route.
- The old dedicated line material (`material.rs`, `panel_line_batch.wgsl`) was
  removed in commit `e925cbe`; the shared analytic path is the only registered
  route.

Within one element, same-styled line primitives merge into a single
multi-contour analytic path before packing:

- `LineMergeKey` groups resolved primitives by element, color, clip, owner
  bounds, paint lane, and layering.
- Each group becomes one `PathOutline` whose contours union under a single
  winding rule, so abutting or overlapping members (tick-to-spine junctions)
  render solid instead of compositing two AA ramps as
  `1 − (1 − a)(1 − b) < 1` and letting background show through as a line.
- Merging never crosses element boundaries; same-colored lines in different
  elements still composite their AA edges independently.
- Each `PathContour` carries the stroke `min_feature` of its source primitive;
  the packer writes it per curve (`CurveRecord.solver.w`) so the shader
  dilates sub-pixel strokes to the `HairlineWidth` floor per member, not per
  merged path.
- Band counts scale per axis with path extent against
  `PANEL_LINE_BAND_TARGET_DESIGN_UNITS`, keeping merged ruler-scale paths on
  bounded per-band curve lists while small paths keep one exact band.

The renderer batches line path instances across panels when compatibility
matches.
Each batch uses:

- an inert capacity-sized mesh
- shared path-atlas buffers for curve, band, and path records
- per-batch instance and run storage buffers
- per-run panel-to-world transforms
- clipped instance rects and UVs
- dirty record uploads
- capacity growth
- explicit bounds before visibility
- hidden/removed-panel cleanup

`DiegeticPerfStats::line_batch` exposes the visible verification counters:

- batch count
- record count
- upload count

The `units` example shows these values in a lower-left Fairy-Dust-style screen
panel.

## Visual Review Result

The old Bevy 0.18 ruler baseline in `../bevy_hana` drew ruler ticks and spines
as tiny `El.background(...)` rectangles. Those rectangles used the mature panel
geometry/SDF rectangle path.

The first ruler migration correctly emitted and batched line records, but the
dedicated panel-line shader showed poor visibility and unstable-looking color at
grazing angles with stable transparency enabled. A diagnostic opaque/masked
material made the geometry clearly visible, which proved the data and batching
were present, but it looked too aliased to accept as the final renderer.

Conclusion: the authored API and layout/batching model were useful, and the
visual renderer needed to be the shared analytic path renderer. Phase B made
that route the registered panel-line renderer.

## Units Example

`crates/bevy_diegetic/examples/units.rs` now uses `PanelDraw::lines` for ruler
ticks and ruler spine geometry.

Helper functions:

- `metric_vertical_tick_lines`
- `metric_horizontal_ruler_lines`
- `imperial_vertical_ruler_lines`
- `imperial_horizontal_ruler_lines`

The helpers preserve the physical-unit behavior of the previous rectangle
rulers:

- inclusive endpoint marks
- metric and imperial major/minor tick lengths
- stroke-center insets at measurement edges
- imperial measured-track height independent of `EDGE_LABEL_EXTRA`
- bounded batch count instead of per-tick visual entities

The left A4 ruler visual issue was not a layout or record-collection issue. It
was the evidence that the old dedicated line renderer was the wrong long-term
visual path.

## Shared Analytic Path Target

The production target is a shared analytic path renderer.

Rationale:

- Slug text already renders analytic glyph outlines with better AA and
  grazing-angle behavior.
- Text glyphs are closed quadratic contours.
- Panel lines and callouts can become stroked closed contours.
- Future renderer fixes should improve glyphs, ruler ticks, guide lines,
  arrows, callouts, dividers, and vector overlays together.

Target ownership:

```text
crates/bevy_diegetic/src/render/
  batch_key.rs
  analytic_paths/
    mod.rs
    atlas.rs
    batching.rs
    geometry.rs
    material.rs
    packing.rs
    analytic_path.wgsl
    analytic_path_vertex_pull.wgsl
  panel_text/
    ...
  panel_shapes/
    batching.rs
    path.rs
    primitive.rs
```

Text remains responsible for:

- shaping
- font lookup
- glyph selection
- text-run layout
- producing glyph outlines

Panel lines remain responsible for:

- authored `PanelDraw::lines`
- layout resolution
- source identity
- clipping and paint lanes
- converting resolved line/cap primitives into stroked path contours by emitting
  `PathOutline` / `PathContour` values made of `QuadraticSegment`s

The shared analytic path renderer should own:

- packed path records
- curve and band data where appropriate
- material/shader setup
- batching and compatibility
- AA and grazing-angle coverage behavior

The current `render/panel_shapes` module is now an adapter: it owns panel-shape
source identity, clipping, retained cleanup, and batch counters while feeding
the shared analytic renderer.

## Remaining Implementation Phases

These phases are the remaining work from the as-built state above. They are not
historical requirements.

### Phase A - Shared Analytic Path Core (complete)

Create `render/analytic_paths/` as the shared renderer target.

Deliverables:

- Define path contour records, curve records, instance/run records, and batch
  keys that can represent glyph outlines and panel-authored vector marks.
- Move reusable slug/text coverage and AA behavior into this renderer-owned
  layer without moving text shaping or glyph lookup out of text modules.
- Keep panel backgrounds on the existing SDF rectangle path.
- Preserve or improve the current glyph visual quality.
- Keep the renderer compatible with stable transparency and the current
  batching/visibility lifecycle.

Acceptance:

- Existing text still renders through the shared path infrastructure or through
  an explicitly documented compatibility bridge.
- The shared path module owns the coverage/AA policy that future line and
  callout marks will use.
- The module structure is discoverable and does not hide the renderer under
  `text/slug/runtime`.

#### Retrospective

**What worked:**

- `render/analytic_paths/` now owns packing, material, shader handles, batch
  storage, path geometry records, and the shared analytic path plugin.
- Text shaping, glyph lookup, font cache, and glyph outline extraction stayed in
  `text/slug/`; `text/slug/glyph::build_packed_glyph` is the explicit bridge to
  renderer-owned `PathOutline` packing.

**What deviated from the plan:**

- The renderer still exposes glyph-compatible aliases such as `PathRecord` and
  `PathInstanceRecord` while Phase B begins consuming the path names.
- The existing text batch key remains text-oriented, while the already-shared
  `VisualBatchKey` remains the generic render compatibility key for future path
  producers.

**Surprises:**

- The shader coverage mirror test moved cleanly after updating the tripwire to
  `render/analytic_paths/analytic_path.wgsl`.
- Moving `MaterialPlugin<TextMaterial>` into `AnalyticPathPlugin` did not require
  panel text lifecycle changes.

**Implications for remaining phases:**

- Phase B should convert panel line primitives into `PathOutline` / path record
  inputs instead of adding another line-specific SDF path.
- Phase B should decide whether it can reuse the text-compatible analytic batch
  store directly or needs a small source-kind wrapper around the shared visual
  batch key.
- Phase D and Phase E should treat text/callouts as clients of
  `render/analytic_paths`, not owners of separate renderer logic.

#### Phase A Review

- Phase B was narrowed to line/cap contour construction, generic path atlas
  ownership, instance routing, clipping policy, focused tests, and fallback
  retirement; it must not rebuild coverage or AA infrastructure.
- Phase B now names the batch-store boundary decision explicitly: panel marks
  must either become analytic path runs with stable source identity or use a
  small source-kind wrapper around the shared visual batch key.
- Phase B now carries the clipping, hidden-panel cleanup, visual-bounds, and
  line-batch-stat requirements before typography overlay or callouts consume
  the shared path renderer.
- Phase D was narrowed to remaining typography overlay line/callout/gizmo paths
  because overlay metric panels already use transparent `DiegeticPanel`
  children.
- Phase E was split into planar mapping/classification and renderer routing so
  standalone `CalloutLine` unification has a concrete boundary.

### Phase B - Panel Lines To Analytic Paths (complete)

Convert resolved `PanelLine` primitives into analytic path contours and route
`PanelDraw::lines` through the shared path renderer.

Deliverables:

- Decide and document the analytic batch-store boundary before changing line
  rendering: either panel marks become stable analytic path runs, or the shared
  renderer gets a small source-kind wrapper around `VisualBatchKey`.
- Introduce or select a generic path atlas / mark cache keyed by non-glyph
  sources such as `PanelShapePrimitiveKey`.
- Add the panel-line path emitter, expected under `render/panel_lines/`, that
  maps each `ResolvedPanelShapePrimitive` into renderer-owned `PathOutline`
  data: straight line edges emit `QuadraticSegment`s with midpoint controls,
  and curved caps/marks emit true quadratic segments or explicit quadratic
  subdivisions.
- Convert stroked line shafts into closed filled contours.
- Convert `CalloutCap` primitives into compatible path contours.
- Route line/cap contours into the existing analytic coverage, AA, material,
  and shader path; do not add another line-specific coverage implementation.
- Preserve `PanelShapeSourceKey`, including external/stable source ids for
  non-ordinal producers, plus clips, paint lanes, layering, and panel-local
  coordinate semantics.
- Choose and implement the clipping policy explicitly: pre-clipped contours,
  clipped instance quads/UVs, or a per-instance clip field that preserves
  `DrawOverflow`.
- Keep `DiegeticPerfStats::line_batch` meaningful as vector-mark stats until a
  deliberate replacement is designed.
- Keep the current `render/panel_lines` batch renderer only as a temporary
  fallback while parity is proven.

Acceptance:

- The `units` rulers still use authored `PanelDraw::lines`.
- The A4 left ruler renders with the analytic precision of the Bevy 0.18
  rectangle-backed baseline.
- Batch count remains bounded by compatibility, not tick count.
- Conversion, clipping, hidden-panel cleanup, visual-bounds, and stale-record
  behavior have focused tests in Phase B before Phase D or Phase E depend on
  the path renderer.
- The `units.rs` line batch HUD still reports useful vector-mark batch counts
  after the renderer route changes.
- The dedicated line SDF shader is retired for ruler-quality paths or clearly
  documented as a temporary fallback.

#### Retrospective

**What worked:**

- `render/panel_lines/path.rs` now converts segment shafts and resolved cap
  forms into closed `PathOutline` contours, including midpoint-control
  quadratics for straight edges and quadratic arc segments for circles.
- `PanelLineBatchStore` kept panel-owned source cleanup while routing payloads
  through `PathAtlas<PanelLineRenderKey>`, `PathInstanceRecord`, `RunRecord`,
  and `TextMaterial`.

**What deviated from the plan:**

- Phase B added `RunRecord::oit_depth_offset` so panel-line analytic paths keep
  per-primitive OIT ordering through the shared run table.
- The old `render/panel_lines/material.rs` and `panel_line_batch.wgsl` were
  left unregistered instead of deleted so Phase F can remove or archive them
  after later consumers prove the shared route.

**Surprises:**

- The shared analytic shader needed no coverage changes for panel-line paths;
  only the run-table OIT offset and shader hash changed.
- `SdfPrimitiveKind` no longer needs the panel-line-only oriented variants once
  panel lines leave the dedicated SDF material path.

**Implications for remaining phases:**

- Phase D and Phase E can produce ordinary `PanelDraw::lines` records and rely
  on the analytic path adapter instead of carrying a separate overlay/callout
  renderer route for planar marks.
- Phase F should decide whether to delete the quarantined line SDF files or keep
  them as archived reference code once typography and planar callouts use the
  shared path renderer.

#### Phase B Review

- Phase D now names border-backed typography metric panels as analytic
  `PanelDraw::lines` migration work instead of allowing SDF borders to satisfy
  the vector-mark acceptance by accident.
- Phase D now requires an external-source-id producer path or an explicit
  stale-cleanup check when overlay panel entities are recreated.
- Phase D now treats stale metric gizmo construction as cleanup/documentation
  work, not a live renderer migration.
- Phase D exempts typography-example dots/circles from the line migration and
  records the separate non-line mark design question at the end of this doc.
- Phase E now focuses on planar classification, transparent-panel selection,
  coordinate mapping, stable source identity, and parity tests because Phase B
  already owns shaft/cap analytic rendering.
- Phase E now names `CalloutLine::surface_shadow` preservation as a transparent
  panel grouping or fallback requirement.
- Phase F now includes path-neutral naming cleanup for glyph/text-compatible
  analytic renderer internals, or a deliberate deferral.

#### Phase B Post-Review Fixes

Visual review of the analytic ruler output surfaced and fixed:

- Tick-to-spine junction line: two abutting analytic draws composite their AA
  ramps below full coverage even when the geometric coverages sum to one.
  Fixed by the per-element merge described under Current Renderer; `units.rs`
  now authors each metric vertical ruler's spine and ticks in one element so
  they share a merge group.
- Hairline dilation moved from per-path to per-curve: `CurveRecord.solver.w`
  (previously a dead orientation flag) now carries the owning contour's
  narrowest stroke, so a merged path containing both thin ticks and a thicker
  spine dilates each member correctly.
- AA Off/Supersample sign inversion: `distance_coverage` uses an
  inside-positive smoothstep ramp while the `aa_band` path is inside-negative;
  the dilation rework initially applied one sign convention to both, rendering
  thin glyph strokes as hollow rims with AA off. Fixed with per-path sign
  handling. The CPU coverage probe mirrors only the `aa_band` path, which is
  why 318 tests passed through the regression — a `distance_coverage` mirror
  is Phase C work.
- The pre-fix Supersample/Off path eroded sub-floor strokes proportionally to
  their width deficit — an accidental distance fade that presented as tick
  LOD. All four AA modes now dilate to the floor at full alpha; reproducing
  the fade deliberately is Phase C's `HairlineFade` policy.

### Phase C - AA And Hairline Fade Cascades (complete)

Make the anti-aliasing mode and the hairline fade policy cascade
global → panel → element, the way `AlphaMode` cascades, so text and lines —
or individual elements — can be tuned independently. Motivating cases: thin
distant ruler ticks should be able to fade out instead of dilating at full
alpha, and small text on colored backgrounds needs different AA than the
global default.

Deliverables:

- Add a `HairlineFade` policy field to `HairlineWidth`: `Full` (default,
  current behavior) dilates sub-floor strokes to the floor at full alpha;
  `Fade { exponent }` dilates but scales alpha by
  `(natural_width / floor)^exponent`. Validate the exponent (positive,
  finite) at one site; the resource is Reflect/BRP-mutable, so arbitrary
  values can arrive at runtime.
- Declare `AntiAlias` and `HairlineFade` as cascade attributes through
  the existing `cascade_attr!` infrastructure (`TextAlpha` is the template):
  `Override<A>` / `Resolved<A>` components, cascade plugin registration, and
  attribute verbs. The global resources stay as the cascade root defaults.
- Carry the resolved AA mode and fade policy in `RunRecord` instead of
  material uniforms so overrides do not split batches or materials. Update
  the `RunRecord` size assertion and the WGSL mirror struct. Add a single
  enum→bits conversion site (an `AaBits`-style helper plus a fade-bits
  helper) so the GPU encoding cannot drift from the authored enums.
- Shader constraint: read the per-run flags and compute `fwidth`/`dpdx`/
  `dpdy` unconditionally in uniform control flow, then branch — derivative
  ops inside a branch on per-run data are non-uniform flow and undefined in
  WGSL.
- Derive the fade factor in the shader from the already-tracked winning curve
  dilation (`natural = target − 2 × dilation`); no new per-curve data.
  Compute it per coverage evaluation (per supersample point) from that
  evaluation's winning dilation; acceptance checks for shimmer where adjacent
  samples select different winning curves.
- Keep fade independent of AA mode — fade susceptibility must not be a side
  effect of which coverage estimator runs (that coupling was the Phase B sign
  bug).
- Text stays structurally exempt from fade with no run flag needed: glyph
  curves carry `solver.w = 0`, so dilation is 0, `natural = target`, and the
  fade factor is 1.
- Merge interaction: `LineMergeKey` groups per element and cascade
  resolution is per element, so every member of a merge group shares one
  resolved AA mode and fade policy; the merged group's run carries the
  resolved values.
- Global-change invalidation: a `AntiAlias` or `HairlineWidth` change
  must re-resolve cascades and mark run tables dirty — the sync systems
  evolve from material-uniform mirrors into cascade-root updates; per-record
  bits cannot be refreshed by rewriting a uniform.
- Discard fragments below the alpha epsilon before OIT submission so
  near-zero faded fragments do not occupy OIT fragment-pool slots (the pool
  has a measured exhaustion history).
- Update the shader-hash tripwire and extend the CPU coverage probe to mirror
  `distance_coverage` including dilation tracking and the fade factor,
  closing the test gap that let the Phase B sign inversion through. These
  tests are Phase C acceptance prerequisites, not later cleanup.
- Complete Phase C before Phase D and Phase E implementation so overlay and
  callout line producers inherit per-element AA/fade support without
  retrofit.

Acceptance:

- An element-level AA override renders with its own mode while siblings keep
  the inherited mode, without increasing batch count.
- `Fade` on the units rulers reproduces the distance fade-out of thin ticks
  while spines and at-floor strokes stay at full alpha, with no shimmer at
  fade boundaries under `Supersample`/`Both`.
- Text rendering is unaffected by any fade configuration.
- A global resource change applied after per-element overrides exist re-packs
  run records correctly (integration test).
- With no overrides authored and `Full` fade, output is visually identical to
  the current as-built state.

#### Retrospective

**What worked:**

- `cascade_attr!(existing Ty, default = ...)` — a new macro arm in
  `cascade/resolved.rs` — joins an already-named render value type to the
  cascade without minting a wrapper struct, so the attributes are literally
  `AntiAlias` and `HairlineFade` (`Override<AntiAlias>`,
  `resolved_anti_alias`).
- `RunRecord` grew `aa_flags: u32` + `fade_exponent: f32` (96 → 112 B stride);
  the encase assertion, both WGSL mirrors, and the padded-payload tests all
  moved together. `AntiAlias::aa_flags()` and
  `HairlineFade::fade_exponent()` are the single conversion/validation sites.
- The fade factor derives in-shader from the evaluation's winning
  `CoverageTerms.dilation` exactly as planned — no new per-curve data; text
  exemption fell out of `solver.w = 0` with no run flag.
- The `distance_coverage` CPU mirror (with dilation + fade) locked the Phase B
  sign convention as a profile-equivalence test: a dilated sub-floor stroke
  must render identically to a naturally-at-floor stroke.

**What deviated from the plan:**

- Panel line elements are arena indices, not entities, so the element level of
  the cascade is tree config (`El::anti_alias` / `El::hairline_fade`,
  `Element` fields, `VisualOnly` in `classify_element_change`), resolved in
  `build_panel_line_group` as element override else the panel entity's
  `Resolved<A>`. The entity cascade covers global → panel → label.
- The aa_band single-sample path's `fwidth(signed_distance(...))` could not
  stay: it sat inside what is now a non-uniform branch (per-run `aa_flags`).
  It was replaced by forward differences along `dpdx`/`dpdy` of the design
  point — the same model the CPU probe already used, so probe and shader now
  agree operation-for-operation. Cost: two extra `signed_distance`
  evaluations in that mode only.
- `CascadePlugin::<AntiAlias>` / `::<HairlineFade>` are registered in
  `HeadlessLayoutPlugin` (not `RenderPlugin`) because `seed_panel_overrides`
  reads their `CascadeDefault<A>` resources in headless layout apps.
- Per-label text AA authoring is the `override_anti_alias` verb on the
  label entity; no `TextStyle::with_anti_alias` capture was added (not a doc
  deliverable — record here so it is a deliberate gap, not an omission).

**Surprises:**

- The alpha-epsilon-before-OIT deliverable was already satisfied: the
  `DISCARD_ALPHA` discard has always preceded `oit_draw` in the fragment
  entry; Phase C only documented the ordering.
- Two global resources now mirror into `CascadeDefault<A>` roots
  (`sync_anti_alias`, `sync_hairline_fade`), ordered
  `.before(CascadeSet::Propagate)` so a global change re-resolves and re-packs
  the same frame; the headless integration test proves the whole chain
  (element override survives a global flip, batch count stays 1).

**Implications for remaining phases:**

- Phase D / Phase E line producers get per-element AA/fade for free by
  authoring `El::anti_alias` / `El::hairline_fade` on the elements that own
  `PanelDraw::lines`; external-source producers (Phase D) inherit the owning
  panel's resolved values through the same `build_panel_line_group` path.
- Phase F's record rename must now also cover `aa_flags` / `fade_exponent`
  WGSL mirror comments and the `AA_FLAG_*` constant pair mirrored between
  `render/mod.rs` and `analytic_path.wgsl`.

#### Phase C Review

- Phase D drops the external `PanelShapeSourceKey` deliverables (user-approved
  2026-06-11): overlay marks are element-owned `PanelDraw::lines`; Phase E
  settles whether `External` gains semantics or is deleted, default element-
  owned trees.
- Phase D now owns migrating the overlay's standalone `CalloutLine` producers
  (arrows + dashed glyph pointers) to `PanelDraw::lines` (user-approved
  2026-06-11); Phase E's parity acceptance names a synthetic consumer instead.
- Phase D's pre-phase producer-site identification is recorded inline as a
  concrete inventory (metric panel builders, the discarded gizmo builder and
  its `draw_dimension_arrow` orphan, arrow/dash producers, exempted dots,
  lifecycle components, stale module doc).
- Phase D adds a fade-policy pin: overlay guides author
  `El::hairline_fade(HairlineFade::Full)` (or record the opposite decision),
  with an acceptance line that guides stay visible under a global `Fade`.
- Phase E's parity acceptance now states the expected AA/dilation divergence
  between the panel-backed and direct routes and restricts parity geometry or
  tolerance accordingly.
- Phase F's SDF deliverable is split into removable (quarantined panel-line
  files) vs retained (`LegacySdfExtendedMaterial` for backgrounds and the non-coplanar
  callout fallback), plus the stale `sdf_material.rs` discriminant doc.
- Phase F's rename audit now covers the Phase C mirror surfaces (`RunRecord`
  WGSL mirrors, `AA_FLAG_*` pair, coverage-probe tripwire) and re-counts the
  reference-site figure at phase start.
- Phase F's cross-panel OIT regression check gains a heterogeneous per-run
  `aa_flags` / `fade_exponent` case after a global resource flip.

#### Phase C Addendum — Per-Line Fade And Two-Lane Coverage (as built 2026-06-11)

Follow-up implemented after Phase C closed, fixing the tick/spine abutment
artifact the element-level fade exemption produced in `units.rs`. Supersedes
the Phase C retrospective statements that `RunRecord` carries `fade_exponent`
and that the fade factor derives from a single winning curve. User-verified
in `units` 2026-06-11: junction line gone at zoom-in, fade-out joins
correctly, spine/majors stay solid.

Problem: exempting majors/spine via `El::hairline_fade(HairlineFade::Full)`
required a second element, splitting the ruler into two merge groups → two
analytic paths whose independent AA ramps composite to ~0.75 alpha where
minor ticks abut the spine (a faint junction line). Blend modes cannot fix
this — at the junction each path contributes ~0.5 coverage and `over` gives
0.75, `max` 0.5, while the union is 1.0 — so the merge has to happen at the
winding level, inside one record.

As built:

- **Per-line authoring**: `PanelLine::hairline_fade(HairlineFade)` /
  `LineStyle::hairline_fade` (`Option<HairlineFade>`, `None` inherits the
  element → panel → global resolution). Carried through
  `ResolvedPanelShape::hairline_fade`; resolved per member in
  `build_panel_line_group` and passed to `build_panel_shape_path` as
  `PanelShapeMember { primitive, fade_exponent }`. The merge key is unchanged
  — mixed fade policies share one group, one record, one batch.
- **Fade is per-curve data**: `PathContour` and `CurveRecord` gained
  `fade_exponent` (curve stride 64 → 80 B, new encase assertion);
  `RunRecord.fade_exponent` and `PathUniform.hairline_fade_exponent` were
  deleted (run stride 112 → 96 B; both WGSL mirrors, padded-payload tests,
  and every constructor updated). `aa_flags` stays per-record.
- **Two-lane coverage evaluation** in `analytic_path.wgsl`: curves split into
  an exempt lane (`fade_exponent == 0`; never-fading contours and all text
  glyphs) and a faded lane (`> 0`). Contours are wholly one lane, so each
  lane's winding is a valid winding number of its sub-geometry and the lane
  windings sum to the whole path's. Each lane carries its own `LaneTerms`
  (winding, adjusted distance, dilation) through the existing scan loops;
  `union_lane` rebuilds the whole-path terms from the two accumulators
  (windings add, the nearest-silhouette race is the cross-lane min), and
  coverage combines as `mix(exempt_coverage, union_coverage, fade_factor)`.
  At fade factor 1 that is exactly the pre-fade single-winding evaluation —
  an exempt/faded abutment is union-interior, so no junction line; at factor
  0 only the exempt sub-geometry remains. The aa_band feeders (`SdSample`,
  `signed_distance`) carry exempt + union signed distances (`vec2`,
  per-evaluation band widths); the interior-edge suppression
  (`lanes_any_outside_neighbor`) tests the exempt lane and the union and
  walks neighbors once for both.
- Why not single-winning-curve fade selection (the first cut): with mixed
  stroke widths the thinner stroke dilates more, so its curves win the
  `distance − dilation` race inside the wider exempt stroke — a 0.1 mm
  fading minor tick would dim the 0.2 mm exempt spine's pixels on every tick
  row at zoom-out (dotted spine). The mix combine keeps an exempt contour
  at full alpha wherever it covers (`mix(1, 1, f) = 1`).
- Why not `max(exempt_coverage, faded_coverage × fade_factor)` (the second
  cut, reverted same day): at the boundary where an exempt contour abuts a
  faded one, each lane's AA ramp reaches ~0.5 and `max(0.5, 0.5) = 0.5` — a
  half-alpha junction line in the zoomed-in regime (dilation 0, factor 1),
  worse than the 0.75 the original two-record compositing gave. Union
  coverage at the junction needs both windings in one accumulator. Pinned by
  `merged_mixed_fade_path_has_no_junction_dip` in `coverage_probe.rs`:
  exempt spine in the merged path == spine alone, fading tick == tick alone,
  pointwise along the shared row the mixed path is never darker than the
  all-fading path or the spine alone, and in the undilated regime the mixed
  path equals the all-fading path with the junction at full alpha (the
  max-combine defect's regime, which the dilated-only first version of the
  test never sampled).
- **CPU mirror** (`coverage_probe.rs`): `LaneTerms`/`CoverageTerms`,
  lane-split winding/accumulation, `union_lane`, exempt/union
  `lane_coverage` / `lane_signed_distance`, exempt/union band math in the
  aa_band/aniso mirrors; shader-hash tripwire re-pinned
  (`0x74a9_fc4a_efdd_a544`).
- **units.rs**: the nested fixed/fading element workaround is deleted; each
  ruler is one element again, majors and the spine pin
  `HairlineFade::Full` per line (`exempt_major_ticks`), minors inherit the
  global fade. The Phase C acceptance test
  (`element_overrides_share_one_batch_and_global_changes_repack`) now reads
  fade from the record outline's contours instead of the run record.
- Cost note: the fade exponent lives in packed curve data, so a global
  exponent change re-packs and re-uploads the line atlas (the panel
  reconcile already rebuilt outlines on `Changed<Resolved<HairlineFade>>`;
  the added work is the band pack plus a tens-of-KB upload per step).
- Phase D's fade-policy pin can now be authored per line as well as per
  element; the Phase F rename audit covers `CurveRecord::fade_exponent` (not
  `RunRecord`) and the heterogeneous-fade OIT regression case exercises
  per-curve fade after a global flip.

### Phase D - Typography Overlay Migration (complete)

Move the remaining typography overlay guide paths onto panel-backed analytic
path marks. Existing transparent overlay panels stay; this phase targets the
remaining line, arrow, callout, gizmo, and documentation paths that still bypass
ordinary panel drawing.

Deliverables:

- Keep source text panels and their layout results read-only.
- Use transparent overlay panels mapped to measured text/run bounds.
- Represent metric guides, arrows, and similar annotations with ordinary panel
  draw/path data.
- Convert border-backed metric guide panels such as typography metric-line
  panels to `PanelDraw::lines` / analytic vector marks rather than leaving them
  on SDF rectangle borders.
- Author overlay marks as ordinary element-owned `PanelDraw::lines` with
  element/draw/line ordinals (decision 2026-06-11: the external
  `PanelShapeSourceKey` deliverables are dropped — `External` has no producer,
  no element-index/material/AA-fade resolution semantics, and the overlay's
  per-refresh tree rebuilds make element ordinals naturally stable; Phase E
  settles whether `External` gains semantics or is deleted).
- Keep overlay panel entities stable when retained renderer identity matters.
  If a metric refresh recreates overlay panels, treat the panel-entity portion
  of `PanelShapeRenderKey` as churn and verify stale cleanup explicitly.
- Rebuild or update overlay panels on metric changes so renderer records refresh
  through normal panel lifecycle.
- Remove or explicitly exempt retained gizmo/callout overlay line paths.
  Phase D owns the overlay's standalone `CalloutLine` producers (decision
  2026-06-11): migrate `spawn_metric_arrow_callouts` (metric_lines.rs) and
  `spawn_dashed_callout_line` (glyph.rs) directly to element-owned
  `PanelDraw::lines` on the overlay panels — `LineStyle` already carries
  start/end `CalloutCap` and Phase B renders caps analytically, and one
  element per dash group replaces today's per-dash `CalloutLine` entity
  fan-out. Phase E does not depend on these producers.
- Clean up stale typography metric `GizmoAsset` line/arrow construction when
  the caller already discards those gizmos.
- Exempt typography-example dots/circles from this phase's line migration; they
  are non-line marks and should not force a public path/vector-mark API into
  Phase D.
- Metric guides become hairline-dilating analytic strokes, so a global
  `HairlineFade::Fade` policy would fade them at distance — wrong for a debug
  overlay. Author `El::hairline_fade(HairlineFade::Full)` on guide elements
  (or record an explicit decision that overlay guides may fade).

Producer-site inventory (recorded 2026-06-10, satisfies the pre-phase
identification step):

- Border-backed metric panel: `debug/typography_overlay/metric_lines.rs`
  `spawn_metric_line_panel` + `build_metric_line_tree`.
- Discarded gizmo builder: `metric_lines.rs` destructures
  `build_metric_gizmos` as `(_, _, metric_lines)` — the gizmo halves are
  built and thrown away. Deleting it also orphans
  `callouts::draw_dimension_arrow` (`callouts/render.rs`, its only caller).
- Arrow callouts: `metric_lines.rs::spawn_metric_arrow_callouts`.
- Bbox border panels, callouts, and dashed lines:
  `debug/typography_overlay/glyph.rs` (`spawn_dashed_callout_line` fans one
  dashed line into many `CalloutLine` entities).
- Mesh-circle dots: `glyph.rs::spawn_overlay_dot` (exempted, non-line mark).
- Lifecycle/cleanup components: `debug/typography_overlay/pipeline.rs` and
  `lifecycle.rs`.
- Stale documentation: `debug/typography_overlay/mod.rs` still says "Metric
  lines are drawn using Bevy's retained GizmoAsset" — already false; fix in
  this phase's cleanup.

Acceptance:

- Typography guide lines align with the measured text/run bounds they annotate.
- Old callout/gizmo line paths are removed or explicitly exempted.
- Overlay guide lines remain visible under a global
  `HairlineWidth { fade: Fade { .. } }` configuration.
- An explicit churn test: recreate overlay panels through repeated metric
  changes, scan all batch run records for source keys with no live panel
  entity, and assert zero stale records — "does not leave stale records" is
  not satisfiable by inspection alone.

#### Retrospective

**What worked:**

- Border-backed metric panels, arrow callouts, and dashed glyph pointers now
  author `PanelDraw::lines` with `DrawOverflow::Visible`
  (`metric_lines.rs::spawn_metric_guide_panel`,
  `glyph.rs::spawn_glyph_metric_guides`); the per-dash `CalloutLine` entity
  fan-out is gone.
- `build_metric_gizmos` and its `callouts::draw_dimension_arrow` orphan were
  deleted; the stale "drawn using Bevy's retained GizmoAsset" module doc is
  gone.
- Every guide element pins `El::hairline_fade(HairlineFade::Full)`
  (`metric_lines.rs:317`, `glyph.rs:156,200,476`), so a global
  `HairlineFade::Fade` cannot fade the debug overlay.
- The churn acceptance is met by `recreated_guide_panels_leave_no_stale_records`
  (`render/panel_shapes/batching.rs:1179`): recreate guide panels across
  refreshes, assert zero batch records and zero panel-index entries keyed to a
  dead panel entity.

**What deviated from the plan:**

- Dots were planned as *exempt* non-line marks kept on `Mesh3d(Circle)`, and the
  "Open Design Discussion — Non-Line Panel Marks" recorded "do not add this API
  during Phase D." Instead `PanelShape { Line, Circle }` / `PanelCircle` was
  added and the two advancement dots migrated to `glyph.rs::spawn_dot_panel`
  (filled circles through the panel machinery). This supersedes the exempt
  decision and partially answers the Non-Line Panel Marks question.
- The whole renderer module and identity layer were renamed
  `panel_lines` → `panel_shapes`, Line → Shape (`PanelShapeSourceKey`,
  `PanelShapeBatchStore`, `ShapeBatchKey`, `ResolvedPanelShape`,
  `build_panel_shape_path`, `PanelShapeMember`). At Phase D close-out this
  document and its title were renamed to "Panel Shape API" /
  `panel-shape-api.md` and the live module-tree references updated to
  `panel_shapes`; residual "panel-line" doc-comments in code remain for Phase F.
- Acceptance gap: "overlay guide lines remain visible under a global `Fade`" is
  satisfied structurally by the `HairlineFade::Full` pins, but has no dedicated
  overlay regression test; only the Phase C element-override test
  (`panel_shapes/batching.rs`) exercises a global `Fade` flip.

**Surprises:**

- Bisecting the dots on the baseline required anchoring them to the *rendered*
  baseline line's center (`layout_to_world_y(baseline) + metric_line_width/2`),
  because `metric_guide_lines` shifts each line half a stroke toward the top.
  The now-dead `ComputedGlyphMetrics::origin_y` field fell out and was removed.
- Coplanar transparent `PanelCircle` dots composite *behind* the red baseline
  (both transparent at the same draw slot). This z-order issue is deferred to
  the separate z-index / draw-slot branch — not a D/E/F concern.

**Implications for remaining phases:**

- Phase E's mark-identity choice: `PanelShapeSourceKey::External` still has zero
  producers (overlay marks ended up element-owned), so Phase E settles on route
  (a) — element-owned `PanelDraw::lines`. `External` is kept-but-deferred per
  `callouts.md` ("Avoid External as the first route"), not deleted.
- Document reconciliation (filename/title → "Panel Shape", live `panel_lines`
  module-tree references → `panel_shapes`, Non-Line Panel Marks section
  superseded) was done at Phase D close-out; Phase F retains only the residual
  `panel-line` doc-comment cleanup in code.

#### Phase D Review

- Phase E superseded by `docs/bevy_diegetic/callouts.md` (user-approved
  2026-06-15): callouts.md is now the authoritative planar-callout plan; Phase E
  collapses to a pointer carrying its sub-decisions (route (a), `External`
  deferred, shadow grouping, net-new `Vec3`→`PanelPoint` projection, non-exact
  parity) as inputs.
- Phase E mark-identity settled as route (a) (element-owned panel marks);
  `External` is retained-but-unused, NOT deleted — callouts.md "Avoid External
  as the first route" overrides the architect's delete-as-dead recommendation.
- Phase F SDF deliverable corrected: the named quarantined files
  (`panel_lines/material.rs`, `panel_line_batch.wgsl`) were already deleted in
  `e925cbe`; retargeted at the real dead code — the stale discriminant comment
  in both `sdf_material.rs:45` and `sdf_panel.wgsl:70`, plus the unreachable
  `sdf_kind == 4u..7u` branches in `sdf_panel.wgsl`.
- Phase E gained notes (architect, minor): the `Vec3`→`PanelPoint` planar
  projection is the only net-new work; shadow mode is a hard panel-grouping key;
  no in-tree planar `CalloutLine` consumer remains after Phase D.
- Phase F gained notes (architect, minor): rename-audit paths corrected to
  `panel_shapes`; the cross-panel OIT regression must be sequenced against the
  deferred z-index branch.
- Open Design Discussion (Non-Line Panel Marks) marked superseded: `PanelShape`
  / `PanelCircle` shipped in Phase D; remaining questions tracked in callouts.md.
- This document was renamed to `panel-shape-api.md` and reconciled at close-out
  (user-approved 2026-06-15).

### Phase E - Planar Callout Unification (superseded by callouts.md)

Planar callout unification is now planned in `docs/bevy_diegetic/callouts.md`,
which subsumes this phase's intent at larger scope: a semantic callout facade
(`DiegeticCallout::screen` / `world_on_plane`, `PanelCallout`), a neutral
`CalloutSpec` shared spec, typestate space adapters, and units / targets /
draw-order policy — all lowering to the same `PanelShape` analytic backend.
This phase is retained only as the sub-decisions that feed that plan:

- Mark-identity route is (a): element-owned `PanelDraw::lines` / `::shapes` on
  transparent panels. `PanelShapeSourceKey::External` stays unused/deferred
  (callouts.md "Avoid `External` as the first route") — it has zero producers
  crate-wide today, but is kept until post-layout producers have explicit
  lifecycle/ownership/cascade/cleanup semantics.
- The genuinely net-new work is the `Vec3`→`PanelPoint` planar projection
  (panel basis/normal from the two endpoints plus a reference axis; cf.
  `callouts/render.rs::cap_perp`); the transparent-panel + element-owned
  authoring it routes into is already proven by Phase D
  (`glyph.rs::spawn_guide_panel`, `metric_lines.rs` arrow lines).
- Shadow mode is a hard panel-grouping key: `CalloutLine::surface_shadow` is
  per-callout while `DiegeticPanel::surface_shadow` is per-panel, so two
  standalone callouts with different `SurfaceShadow` cannot share one backing
  panel (matches callouts.md's render-context grouping list).
- Keep the direct `CalloutLine` SDF renderer for non-coplanar cases.
- Parity is not pixel-exact: the panel-backed route applies cascade-resolved
  `AntiAlias` + hairline dilation, the direct `LegacySdfExtendedMaterial` route does
  not, so a sub-floor stroke renders wider panel-backed. Restrict any parity
  test to at-floor-or-wider strokes. There is no in-tree planar `CalloutLine`
  consumer after Phase D, so the first real consumers arrive through
  callouts.md.

See `docs/bevy_diegetic/callouts.md` for the implementation outline and the
units / targets / draw-order policy this phase did not cover.

### Phase F - Hardening And Archive Closeout

Close hardening and documentation gaps after the shared path renderer has real
consumers.

Deliverables:

- The bulk of the document reconciliation was done at Phase D close-out
  (filename/title → "Panel Shape API" / `panel-shape-api.md`, live module-tree
  references → `panel_shapes`, Non-Line Panel Marks section superseded). What
  remains: clean up the residual `panel-line` doc-comments in code (e.g.
  `panel_shapes/primitive.rs` calls `PanelShapeRenderKey` a "panel-line"
  identity), and keep the as-built module summary current.
- Rename text/glyph-compatible internal analytic-path names such as
  `TextMaterial`, `PathRecord`, and `PathInstanceRecord` to path-neutral
  names, or explicitly defer with a per-type rationale recorded in code. The
  rename touches reference sites across render and text modules (re-count at
  phase start; the `panel_lines`→`panel_shapes` module rename is already done
  uncommitted, so Phase C's added uses are now in `panel_shapes/batching.rs`
  and `panel_text/batching.rs`); this deliverable also folds in the residual
  Line→Shape doc-comment cleanup the Phase D rename left partial — e.g.
  `panel_shapes/primitive.rs` still calls `PanelShapeRenderKey` a "panel-line"
  identity. Stage it through the existing `PathRecord` /
  `PathInstanceRecord` aliases — first switch crate-internal uses to the
  aliases, then rename the definitions — so there is a compiling
  intermediate state. The rename audit also covers the Phase C surfaces: the
  `RunRecord` mirror structs and `aa_flags` comments in
  `analytic_path.wgsl` and `analytic_path_vertex_pull.wgsl`, the
  `CurveRecord::fade_exponent` mirrors (Phase C addendum moved fade from
  `RunRecord` to per-curve data), the `AA_FLAG_SUPERSAMPLE` /
  `AA_FLAG_BAND` constant pair mirrored between `render/mod.rs` and
  `analytic_path.wgsl`, and the shader-hash tripwire + CPU mirror in
  `text/slug/glyph/coverage_probe.rs`.
- Add a cross-panel OIT ordering regression check: lines from multiple
  panels batched together must composite in the order their
  `oit_depth_offset` values declare. Include a heterogeneous case: a
  cross-panel batch whose runs carry different `aa_flags` and whose packed
  curves carry different `fade_exponent` values must composite and re-pack
  correctly after a global `AntiAlias` / `HairlineWidth` flip (Phase
  C's headless test covers only the intra-panel case). This test's
  `oit_depth_offset` semantics may shift if the deferred z-index / draw-slot
  branch (Phase D retrospective: coplanar `PanelCircle` dots compose behind the
  baseline) lands first — sequence this check after that branch, or pin it to
  draw-slot ordering explicitly.
- Keep only cross-feature regression tests here; Phase B owns the focused path
  conversion, cleanup, clipping, and visual-bounds tests required before later
  consumers use the path renderer.
- Capture and keep the ruler visual baseline evidence used to accept the pivot.

Acceptance:

- `units.rs`, typography overlay guides, and representative planar callouts all
  use the shared analytic path renderer for vector marks.
- Panel backgrounds remain on the SDF rectangle fast path.
- The document no longer names the old line renderer as a production path.

## Panel Backgrounds

Ordinary panel backgrounds, rectangle fills, and simple borders stay on the
existing SDF panel-geometry path.

Reasoning:

- Backgrounds are surface primitives, not arbitrary vector marks.
- They need efficient rectangular layout ownership, fill, border,
  rounded-corner behavior, material inheritance, lighting, shadows, and
  predictable depth behavior.
- Generic path packing would add cost and complexity without improving the
  common rectangle case.

Use the shared analytic path renderer for marks:

- glyphs
- ruler ticks
- guide lines
- arrows
- callouts
- dividers
- ornaments
- future vector overlays

Non-rectangular panel fills, cutouts, or decorative vector backgrounds can use
analytic paths later if a real feature needs them.

## Callouts

`CalloutCap` semantics are shared with `LineStyle`.

Standalone `CalloutLine` remains useful as a public world/local API and for
non-coplanar cases. Planar callouts should evolve toward transparent
panel-backed authoring:

- create or select a transparent `DiegeticPanel`
- represent callout geometry with `PanelDraw::lines` or future path draw data
- convert strokes and caps into analytic path contours
- batch with other compatible panel marks

Do not spend more work making the old line/cap SDF shader the long-term shared
callout implementation.

## Typography Overlay

Typography overlay line migration should use the analytic path adapter or
explicitly document any renderer-specific exception.

The preferred model remains unified panel-backed drawing:

- the source text panel and its layout result stay read-only
- a transparent overlay panel maps to measured text/run bounds
- overlay elements own lines/path marks through normal panel draw data
- metric changes rebuild or update the overlay panel so renderer records
  refresh normally

This preserves the same coding model that unified `WorldText` and panels:
standalone-looking world content can be represented as a transparent-backed
panel when it is planar.

## As-Built Module Summary

Current implemented modules:

```text
crates/bevy_diegetic/src/
  layout/
    draw.rs                 # PanelDraw and DrawOverflow
    line.rs                 # authored/resolved line API and resolution
    render.rs               # RenderCommandKind::Lines
    engine/positioning.rs   # line command emission
  panel/
    perf.rs                 # analytic line batch performance counters
  render/
    batch_key.rs            # shared visual compatibility pieces
    analytic_paths/
      mod.rs
      atlas.rs              # generic non-glyph path atlas
      batching.rs           # analytic path batch store (PathBatchStore) and shared GPU handle types
      geometry.rs           # Bounds / PathOutline / PathContour / QuadraticSegment
      material.rs           # shared analytic-path PathExtendedMaterial route
      packing.rs            # curve, band, path, instance, and run records
      analytic_path.wgsl
      analytic_path_vertex_pull.wgsl
    panel_shapes/
      mod.rs                # system registration
      batching.rs           # retained analytic path batches for panel shapes
      path.rs               # resolved line/cap primitive to PathOutline adapter
      primitive.rs          # stable panel-shape render identity
  callouts/
    caps.rs                 # shared CalloutCap resolution helpers
  examples/
    units.rs                # panel-shape-backed rulers and batch-count HUD
```

The shared target stays under `render/`, not under `text/slug/runtime`, so text and
panel marks are peers that feed the renderer.

## Verification Notes

Useful verification points:

- layout command tests for `RenderCommandKind::Lines`
- helper tests in `units.rs` for tick generation
- `DiegeticPerfStats::line_batch` for batch/record/upload counts
- visual comparison against the Bevy 0.18 rectangle-backed ruler baseline

The visual comparison is the reason this archive points to the shared analytic
path renderer instead of treating the current dedicated panel-line renderer as
complete.

## Open Design Discussion - Non-Line Panel Marks

Phase D intentionally exempts typography-example dots/circles from the
panel-line migration. They are filled non-line marks, and adding a public
path/vector-mark draw API would expand the typography migration beyond its goal.

Design questions to revisit:

- Do we need a public `PanelDraw::paths` or `PanelDraw::marks` API for filled
  circles, arbitrary closed contours, symbols, and non-line ornaments?
- Should small filled dots be represented as a degenerate line with a circular
  cap, or is that too implicit for authors and future maintainers?
- Should filled marks share the panel-line retained store and
  `PathAtlas<PanelLineRenderKey>`, or should a broader vector-mark store own
  both line and non-line path producers?
- What clipping, paint-order, stable-source-key, shadow, and material semantics
  should apply when the mark is not naturally a stroke/cap pair?

Superseded (Phase D, 2026-06-15): this API was added during Phase D —
`PanelShape { Line, Circle }` / `PanelCircle` is public, packs as a filled
contour (`panel_shapes/path.rs`), and the typography dots migrated to
`glyph.rs::spawn_dot_panel`. The filled-circle and shared-store questions are
answered; the remaining open questions (units, clipping/material semantics for
non-stroke marks, semantic-vs-primitive layering) are now tracked in
`docs/bevy_diegetic/callouts.md`. Phase F reconciles this section.

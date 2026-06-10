# Panel Line API As-Built Review - 2026-06-09

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
- `LineStyle` owns stroke width, color, cap size, and start/end `CalloutCap`.
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

- `PanelLineSourceKey` comes from layout and identifies the source element, draw
  ordinal, line ordinal, and primitive ordinal.
- `PanelLineRenderKey` prefixes the source key with the panel entity for
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

The current panel-line adapter lives under `render/panel_lines/` and is
registered by `RenderPlugin`.

Implemented pieces:

- `render/batch_key.rs` contains shared visual compatibility keys and material
  interning used by text and line batching.
- `render/panel_lines/mod.rs` registers the plugin and systems.
- `render/panel_lines/path.rs` converts resolved shaft/cap primitives into
  closed analytic `PathOutline` contours plus clipped instance rect/UV data.
- `render/panel_lines/batching.rs` routes panel line primitives into retained
  cross-panel analytic path batches.
- `render/panel_lines/primitive.rs` owns stable panel-line render identity.
- `render/analytic_paths/atlas.rs` owns the generic path atlas used by
  non-glyph path producers.
- `render/analytic_paths/material.rs` / `analytic_path*.wgsl` provide the
  shared analytic coverage, AA, vertex-pulling, and material route.
- `render/panel_lines/material.rs` and `panel_line_batch.wgsl` are no longer
  registered; they remain only as temporary fallback/quarantine files.

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
  panel_lines/
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

The current `render/panel_lines` module is now an adapter: it owns panel-line
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

- The renderer still exposes glyph-compatible aliases such as `GlyphRecord` and
  `GlyphInstanceRecord` while Phase B begins consuming the path names.
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
- Phase C and Phase D should treat text/callouts as clients of
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
- Phase C was narrowed to remaining typography overlay line/callout/gizmo paths
  because overlay metric panels already use transparent `DiegeticPanel`
  children.
- Phase D was split into planar mapping/classification and renderer routing so
  standalone `CalloutLine` unification has a concrete boundary.

### Phase B - Panel Lines To Analytic Paths (complete)

Convert resolved `PanelLine` primitives into analytic path contours and route
`PanelDraw::lines` through the shared path renderer.

Deliverables:

- Decide and document the analytic batch-store boundary before changing line
  rendering: either panel marks become stable analytic path runs, or the shared
  renderer gets a small source-kind wrapper around `VisualBatchKey`.
- Introduce or select a generic path atlas / mark cache keyed by non-glyph
  sources such as `PanelLinePrimitiveKey`.
- Add the panel-line path emitter, expected under `render/panel_lines/`, that
  maps each `ResolvedPanelLinePrimitive` into renderer-owned `PathOutline`
  data: straight line edges emit `QuadraticSegment`s with midpoint controls,
  and curved caps/marks emit true quadratic segments or explicit quadratic
  subdivisions.
- Convert stroked line shafts into closed filled contours.
- Convert `CalloutCap` primitives into compatible path contours.
- Route line/cap contours into the existing analytic coverage, AA, material,
  and shader path; do not add another line-specific coverage implementation.
- Preserve `PanelLineSourceKey`, including external/stable source ids for
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
  behavior have focused tests in Phase B before Phase C or Phase D depend on
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
  through `PathAtlas<PanelLineRenderKey>`, `GlyphInstanceRecord`, `RunRecord`,
  and `TextMaterial`.

**What deviated from the plan:**

- Phase B added `RunRecord::oit_depth_offset` so panel-line analytic paths keep
  per-primitive OIT ordering through the shared run table.
- The old `render/panel_lines/material.rs` and `panel_line_batch.wgsl` were
  left unregistered instead of deleted so Phase E can remove or archive them
  after later consumers prove the shared route.

**Surprises:**

- The shared analytic shader needed no coverage changes for panel-line paths;
  only the run-table OIT offset and shader hash changed.
- `SdfPrimitiveKind` no longer needs the panel-line-only oriented variants once
  panel lines leave the dedicated SDF material path.

**Implications for remaining phases:**

- Phase C and Phase D can produce ordinary `PanelDraw::lines` records and rely
  on the analytic path adapter instead of carrying a separate overlay/callout
  renderer route for planar marks.
- Phase E should decide whether to delete the quarantined line SDF files or keep
  them as archived reference code once typography and planar callouts use the
  shared path renderer.

#### Phase B Review

- Phase C now names border-backed typography metric panels as analytic
  `PanelDraw::lines` migration work instead of allowing SDF borders to satisfy
  the vector-mark acceptance by accident.
- Phase C now requires an external-source-id producer path or an explicit
  stale-cleanup check when overlay panel entities are recreated.
- Phase C now treats stale metric gizmo construction as cleanup/documentation
  work, not a live renderer migration.
- Phase C exempts typography-example dots/circles from the line migration and
  records the separate non-line mark design question at the end of this doc.
- Phase D now focuses on planar classification, transparent-panel selection,
  coordinate mapping, stable source identity, and parity tests because Phase B
  already owns shaft/cap analytic rendering.
- Phase D now names `CalloutLine::surface_shadow` preservation as a transparent
  panel grouping or fallback requirement.
- Phase E now includes path-neutral naming cleanup for glyph/text-compatible
  analytic renderer internals, or a deliberate deferral.

### Phase C - Typography Overlay Migration

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
- Assign stable external `PanelLineSourceKey` values for overlay-produced marks
  so metric refreshes do not depend on ordinal churn.
- Add or select a producer path for those external source ids; ordinary
  element-owned `PanelDraw::lines` continues to use element/draw/line ordinals.
- Keep overlay panel entities stable when retained renderer identity matters.
  If a metric refresh recreates overlay panels, treat the panel-entity portion
  of `PanelLineRenderKey` as churn and verify stale cleanup explicitly.
- Rebuild or update overlay panels on metric changes so renderer records refresh
  through normal panel lifecycle.
- Remove or explicitly exempt retained gizmo/callout overlay line paths.
- Clean up stale typography metric `GizmoAsset` line/arrow construction when
  the caller already discards those gizmos.
- Exempt typography-example dots/circles from this phase's line migration; they
  are non-line marks and should not force a public path/vector-mark API into
  Phase C.

Acceptance:

- Typography guide lines align with the measured text/run bounds they annotate.
- Old callout/gizmo line paths are removed or explicitly exempted.
- Overlay removal and metric changes do not leave stale path records.

### Phase D - Planar Callout Unification

Add a transparent-panel-backed path for planar callouts.

Deliverables:

- Preserve `CalloutLine` as the standalone public API.
- Add a mapping/classification slice that detects planar callouts, chooses or
  creates the transparent panel, maps local/world `Vec3` endpoints into panel
  coordinates, and preserves render layers.
- Add a renderer-routing slice that emits stable external panel mark source ids
  and routes planar callout shafts/caps through the Phase B panel-line analytic
  path adapter; do not build another cap/shaft renderer route.
- Route planar callout geometry through a transparent panel and shared analytic
  path marks where possible.
- Keep the direct callout renderer for non-coplanar cases or accepted
  temporary exceptions.
- Preserve `CalloutLine::surface_shadow` by grouping panel-backed callouts into
  transparent panels with matching `SurfaceShadow`, or document a temporary
  direct-renderer fallback for shadow modes that cannot map through a panel.
- Define and document shadow policy for panel-backed callouts.

Acceptance:

- A representative planar standalone callout and panel-backed callout match in
  endpoints, insets, caps, thickness, color, and clipping.
- Panel-backed callouts batch with compatible panel marks.
- Non-coplanar callouts remain supported or are rejected by a clear boundary.

### Phase E - Hardening And Archive Closeout

Close the old-renderer gap after the shared path renderer has real
consumers.

Deliverables:

- Remove or quarantine the dedicated panel-line SDF renderer.
- Keep the as-built module summary current.
- Rename text/glyph-compatible internal analytic-path names such as
  `TextMaterial`, `GlyphRecord`, and `GlyphInstanceRecord` to path-neutral
  names, or explicitly defer those compatibility aliases with rationale.
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
      batching.rs           # text batch store and shared GPU handle types
      geometry.rs           # Bounds / PathOutline / PathContour / QuadraticSegment
      material.rs           # shared analytic-path TextMaterial route
      packing.rs            # curve, band, path, instance, and run records
      analytic_path.wgsl
      analytic_path_vertex_pull.wgsl
    panel_lines/
      mod.rs                # system registration
      batching.rs           # retained analytic path batches for panel lines
      path.rs               # resolved line/cap primitive to PathOutline adapter
      primitive.rs          # stable panel-line render identity
      material.rs           # unregistered temporary fallback/quarantine
      panel_line_batch.wgsl # unregistered temporary fallback/quarantine
  callouts/
    caps.rs                 # shared CalloutCap resolution helpers
  examples/
    units.rs                # panel-line-backed rulers and batch-count HUD
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

Phase C intentionally exempts typography-example dots/circles from the
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

Current decision: do not add this API during Phase C. Keep dots/circles in the
typography example on their existing path unless a later phase explicitly adds
non-line panel mark support.

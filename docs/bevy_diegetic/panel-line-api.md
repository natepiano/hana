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
render command emission, batched rendering, performance counters, and the
`units.rs` ruler example.

The current dedicated panel-line renderer is not the accepted long-term visual
renderer. It proved the data model, lifecycle, and batching, but visual review
found that its independent SDF/OIT/AA path diverges from slug text quality at
grazing angles. The production target is now a shared analytic path renderer
used by text glyphs and panel-authored vector marks.

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

The current renderer lives under `render/panel_lines/` and is registered by
`RenderPlugin`.

Implemented pieces:

- `render/batch_key.rs` contains shared visual compatibility keys and material
  interning used by text and line batching.
- `render/panel_lines/mod.rs` registers the plugin and systems.
- `render/panel_lines/batching.rs` routes panel line primitives into retained
  cross-panel batches.
- `render/panel_lines/primitive.rs` owns current CPU/GPU line primitive records.
- `render/panel_lines/material.rs` owns the current line batch material.
- `render/panel_lines/panel_line_batch.wgsl` is the current vertex-pulled line
  shader.

The renderer batches line primitives across panels when compatibility matches.
Each batch uses:

- an inert capacity-sized mesh
- a storage buffer of line records
- per-record panel-to-world transforms
- per-record clip rectangles
- dirty record uploads
- capacity growth
- explicit bounds before visibility
- hidden/removed-panel cleanup

`DiegeticPerfStats::line_batch` exposes the visible verification seam:

- batch count
- record count
- upload count

The `units` example shows these values in a lower-left Fairy-Dust-style screen
panel.

## Visual Review Result

The old Bevy 0.18 ruler baseline in `../bevy_hana` drew ruler ticks and spines
as tiny `El.background(...)` rectangles. Those rectangles used the mature panel
geometry/SDF rectangle path.

The new ruler migration correctly emits and batches line records, but the
dedicated panel-line shader showed poor visibility and unstable-looking color at
grazing angles with stable transparency enabled. A diagnostic opaque/masked
material made the geometry clearly visible, which proved the data and batching
were present, but it looked too aliased to accept as the final renderer.

Conclusion: the authored API and layout/batching model are useful, but the
dedicated line SDF renderer should be treated as interim.

## Units Example

`crates/bevy_diegetic/examples/units.rs` now uses `PanelDraw::lines` for ruler
ticks and ruler spine geometry.

Helper seams:

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

The left A4 ruler visual issue is not a layout or record-collection issue. It is
the evidence that the current line renderer is the wrong long-term visual path.

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
    batching.rs
    material.rs
    path.rs
    stroke.rs
    analytic_path.wgsl
  panel_text/
    ...
  panel_lines/
    ...
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
- converting resolved line/cap primitives into stroked path contours

The shared analytic path renderer should own:

- packed path records
- curve and band data where appropriate
- material/shader setup
- batching and compatibility
- AA and grazing-angle coverage behavior

The current `render/panel_lines` renderer should shrink toward an adapter or
fallback after the analytic path renderer covers ruler/callout quality and
lifecycle requirements.

## Remaining Implementation Phases

These phases are the remaining work from the as-built state above. They are not
historical requirements.

### Phase A - Shared Analytic Path Core

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

### Phase B - Panel Lines To Analytic Paths

Convert resolved `PanelLine` primitives into analytic path contours and route
`PanelDraw::lines` through the shared path renderer.

Deliverables:

- Convert stroked line shafts into closed filled contours.
- Convert `CalloutCap` primitives into compatible path contours.
- Preserve `PanelLineSourceKey`, clips, paint lanes, layering, and panel-local
  coordinate semantics.
- Keep the current `render/panel_lines` batch renderer only as a temporary
  fallback while parity is proven.

Acceptance:

- The `units` rulers still use authored `PanelDraw::lines`.
- The A4 left ruler renders with the analytic precision of the Bevy 0.18
  rectangle-backed baseline.
- Batch count remains bounded by compatibility, not tick count.
- The dedicated line SDF shader is retired for ruler-quality paths or clearly
  documented as a temporary fallback.

### Phase C - Typography Overlay Migration

Move typography overlay guides onto panel-backed analytic path marks.

Deliverables:

- Keep source text panels and their layout results read-only.
- Use transparent overlay panels mapped to measured text/run bounds.
- Represent metric guides, arrows, and similar annotations with ordinary panel
  draw/path data.
- Rebuild or update overlay panels on metric changes so renderer records refresh
  through normal panel lifecycle.

Acceptance:

- Typography guide lines align with the measured text/run bounds they annotate.
- Old callout/gizmo line paths are removed or explicitly exempted.
- Overlay removal and metric changes do not leave stale path records.

### Phase D - Planar Callout Unification

Add a transparent-panel-backed path for planar callouts.

Deliverables:

- Preserve `CalloutLine` as the standalone public API.
- Route planar callout geometry through a transparent panel and shared analytic
  path marks where possible.
- Keep the direct callout renderer for non-coplanar cases or accepted
  temporary exceptions.
- Define and document shadow policy for panel-backed callouts.

Acceptance:

- A representative planar standalone callout and panel-backed callout match in
  endpoints, insets, caps, thickness, color, and clipping.
- Panel-backed callouts batch with compatible panel marks.
- Non-coplanar callouts remain supported or are rejected by a clear boundary.

### Phase E - Hardening And Archive Closeout

Close the interim-renderer gap after the shared path renderer has real
consumers.

Deliverables:

- Remove or quarantine the dedicated panel-line SDF renderer.
- Keep the as-built module summary current.
- Add focused tests for path conversion, batch cleanup, clipping, and visual
  bounds.
- Capture and keep the ruler visual baseline evidence used to accept the pivot.

Acceptance:

- `units.rs`, typography overlay guides, and representative planar callouts all
  use the shared analytic path renderer for vector marks.
- Panel backgrounds remain on the SDF rectangle fast path.
- The document no longer names an interim renderer as a production path.

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

Do not spend more work making the interim line/cap SDF shader the long-term
shared callout implementation.

## Typography Overlay

Typography overlay line migration should wait for the analytic path pivot or
explicitly document that it is using the interim panel-line renderer.

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
    perf.rs                 # line batch performance counters
  render/
    batch_key.rs            # shared visual compatibility pieces
    panel_lines/
      mod.rs
      batching.rs
      primitive.rs
      material.rs
      panel_line_batch.wgsl
  callouts/
    caps.rs                 # shared CalloutCap resolution helpers
  examples/
    units.rs                # panel-line-backed rulers and batch-count HUD
```

Target module addition:

```text
crates/bevy_diegetic/src/render/analytic_paths/
```

Keep this target under `render/`, not under `text/slug/runtime`, so text and
panel marks are peers that feed the renderer.

## Verification Notes

Useful verification seams:

- layout command tests for `RenderCommandKind::Lines`
- helper tests in `units.rs` for tick generation
- `DiegeticPerfStats::line_batch` for batch/record/upload counts
- visual comparison against the Bevy 0.18 rectangle-backed ruler baseline

The visual comparison is the reason this archive points to the shared analytic
path renderer instead of treating the current dedicated panel-line renderer as
complete.

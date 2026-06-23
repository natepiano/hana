# Panel draw order

## What it is

A CSS-style single ordering axis for the elements inside a diegetic panel. It
replaced the flat `draw_slot` emission counter (a per-panel integer bumped on
every filled element), the text-only `DEFAULT_DRAW_LAYER = 64` global layer, and
the OIT depth clamp that limited reordering. Three inputs — a fixed per-kind draw
step (`Fill < Lines < Text`), declaration order in the layout tree (`tree_order`),
and one optional signed `DrawZIndex` per element — project to a single dense
ordinal per render command. That one ordinal drives both the sorted screen view
(via per-level `depth_bias` banding) and the OIT world view (via a
`text_anchor`-relative `oit_depth_offset`).

## How it works

**Authoring input.** `Element.z_index: DrawZIndex` (`DrawZIndex(pub i8)` in
`layout/text_props.rs`) is the sole override point; `DrawZIndex(0)` is the default
level. Authors set it through `El::z_index(DrawZIndex)` (`layout/builder.rs:258`),
including for text via the `text_element` / `text_id_element` builders. The layout
engine stamps each `RenderCommand` with the element's `z_index`; scissor commands
carry no draw step and do not participate.

**Sort key.** `HierarchicalDrawKey { z_index, step, tree_order }`
(`render/draw_order.rs`) orders lexicographically by `(z_level, step.ordinal(),
tree_order)`:
- `z_level` = the signed `i8` (default `0`)
- `step.ordinal()` = `Fill (0) < Lines (1) < Text (2)`, fixed per
  `RenderCommandKind` (`layout/render.rs`, `RenderCommandKind::draw_step()`)
- `tree_order` = the command's index in the layout-DFS stream

**Enumeration → projection.** `enumerate_ordinals(&[RenderCommand]) ->
Vec<Option<DrawOrdinal>>` sorts the draw-participating commands by the key and
assigns each a dense rank (`None` for scissors), index-aligned with the stream.
`DrawOrderProjection::from_commands` wraps this into per-command
`DrawCommandDepth { ordinal, z_level, screen_depth_bias, oit_depth_offset }`. It
computes the panel's `text_anchor` (the lowest ordinal among `Text`-step commands)
once, so default text lands at OIT offset `0.0`.

**Two depth axes from one ordinal.**
- **Screen (non-OIT) view:** `screen_depth_bias = level_sublane_depth_bias(z_level,
  level_ordinal)` places level `L` into the `depth_bias` window `[L ×
  DRAW_LEVEL_STRIDE, (L+1) × DRAW_LEVEL_STRIDE)`. A batch is one draw and blends in
  buffer order, so each z-level gets a fixed screen lane.
- **OIT (world) view:** `oit_depth_offset = (ordinal − text_anchor) ×
  OIT_DEPTH_STEP`, computed by `text_anchored_oit_depth_offset`. Per-fragment sort
  means submission order is irrelevant; the offset alone resolves coplanar order.

**Per-level band layout.** Within level `L`:
- Geometry (fills, borders, images) take the low sub-lanes `0..DRAW_LEVEL_GEOMETRY_LANES`
  (`= 64`) at per-command ordinal — each is its own SDF/mesh draw, so z-index already
  moves it on screen.
- The **line batch** takes the reserved sub-lane `DRAW_LEVEL_GEOMETRY_LANES − 1 =
  63` via `line_batch_depth_bias(z_level)`.
- The **text batch** takes sub-lane `DRAW_LEVEL_TEXT_SUBLANE = 64` via
  `text_batch_depth_bias(z_level)`.

**Batched vs. individual draws.** Lines (`render/panel_lines/batching.rs`) and text
(`render/panel_text/batching.rs`) are vertex-pulled batches. Each splits its
batches by z-level (`z_level` on the batch key), so default-level content stays one
shared batch across panels while a raised/lowered run spawns its own batch in the
matching band. Per-record OIT offsets (`PathRenderRecord` fields `depth_nudge` /
`oit_depth_offset`) disambiguate within a batch on the OIT axis. Fills/borders/images
are individual draws, each stamped with its command's full `DrawCommandDepth`.

**Reconcile identity.** A z-index or step change affects ordering only — it re-keys
a record, never respawns the entity. Text-run identity stays `(PanelFieldId,
line_index)`; image identity is `command_index`; SDF surfaces carry `PanelSdfSurface
{ command_index, draw_depth: DrawCommandDepth, … }`. The reuse signature stores the
whole `DrawCommandDepth` (not a bare ordinal) so a `text_anchor` shift — toggling
text on/off — invalidates reuse instead of leaving a stale `oit_depth_offset`.

## Invariants

- **Sorted/OIT parity.** Any two commands order the same on `screen_depth_bias`
  (sorted view) and `oit_depth_offset` (OIT view). The shared `HierarchicalDrawKey`
  order guarantees this by construction; the test
  `sorted_and_oit_orderings_agree_for_every_z_level_pair`
  (`render/draw_order.rs:495`, over `ORDERED_Z_LEVEL_PAIRS`) pins it.
- **Cross-panel anchoring.** `DrawZIndex` is panel-scoped and must never reorder one
  panel's children against another's. A panel's `depth_bias` span must stay below
  the minimum panel-distance `Transparent3d` separation (the 64-pixel threshold).
  Keep *used* levels compressed (≈±5); do not map the full `i8` ±127 range.
- **OIT focus-depth budget.** Near plane = `radius × 0.001` → focus fragment
  `position.z ≈ 1e-3`. Panel-global ordinal span × `OIT_DEPTH_STEP (1e-6)` must stay
  inside the budget, and the offset must never drive `position.z` non-positive (the
  resolve pass drops `alpha < 1` fragments there). The shader floor `n = 3e-6`
  (= `3 × OIT_DEPTH_STEP`) enforces this. Past budget, coplanar order degrades to
  OIT-list insertion order — never a step inversion.
- **Callout band separation.** Callouts keep their own positive-offset OIT axis
  above all panel content; the panel `HierarchicalDrawKey` does not cover them.
- **Single z-index source.** Every command — fill, line, text — takes its level
  from its own element's `z_index`. No inheritance: the old text-only `DrawLayer`
  cascade (a default-`64` layer propagated to label entities) is gone.

## Calibration / gotchas

- **Per-level screen cap.** `DRAW_LEVEL_GEOMETRY_LANES = 64` is a hard ceiling on
  geometry commands at one z-level — beyond it, fills reach the line/text sub-lanes
  or spill into the next band. `DRAW_LEVEL_STRIDE = DRAW_LEVEL_TEXT_SUBLANE + 1 =
  65`. This is a real screen-side limit, separate from the OIT budget.
- **OIT budget.** `oit_depth_budget() = floor(OIT_FOCUS_DEPTH / OIT_DEPTH_STEP)` ≈
  1000 commands panel-wide (`OIT_FOCUS_DEPTH = 0.001`, `OIT_DEPTH_STEP = 1e-6`).
- **Overflow guard.** `per_level_band_overflows` (busiest single z-level vs.
  `per_level_band_capacity()`) and `oit_total_overflows` (panel total vs.
  `oit_depth_budget()`) in `render/panel_geometry.rs` each `warn_once!`
  independently — the guard fires at whichever ceiling hits first. Per-level
  occupancy comes from `DrawOrderProjection::level_occupancy() -> Vec<(i8, usize)>`
  (`enumerate_ordinals` returns flat panel-global ranks with the level discarded, so
  a flat count would miss band occupancy).
- **Shader floor.** `n = 3e-6` is hard-coded in all three shaders
  (`sdf_panel.wgsl`, `analytic_path.wgsl`, `panel_line_batch.wgsl`). The
  `EXPECTED_SHADER_FNV1A` tripwire (`text/slug/glyph/coverage_probe.rs`) hashes
  **only** `analytic_path.wgsl`; editing that shader requires pasting the printed
  new hash in the same change.
- **Lane-boundary tie.** A `Fill` at the old `draw_slot == 63` tied the `Lines`
  lane; the new key deterministically orders `Fill` below `Lines` via
  `step.ordinal()` (the documented lane intent).
- **Two-axis split is the precedent.** Lines and text are the two worked examples of
  "one batch per `(z_level, …)`, CPU-fixed screen lane (`line_batch_depth_bias` /
  `text_batch_depth_bias`) + per-record OIT offset." Any future batched-geometry path
  replicates this `PathRenderRecord` structure rather than re-deriving it.

## Why

- **2-level key, not 1.** Collapsing z-level and step into one signed integer makes a
  set `z = 2` tie with unset `Text` instead of sitting above it. The separate `(z_level,
  step, tree_order)` key keeps "how high is the element" independent from "what kind
  of geometry within that level": unset `Text` beats `z=0 Fill`; `z=2 Fill` beats
  unset `Text`; `z=−1 Text` sinks below unset `Fill`.
- **`tree_order` is the layout-DFS stream index.** Batched glyph/line records
  concatenate in archetype order, not tree order, so "later wins" must land in
  `depth_bias` / `oit_depth_offset`, never in submission order. The stream index is
  the only later-wins definition stable through batching.
- **Text-anchor-relative OIT offset.** Anchoring the lowest text rank to `0.0`
  preserves the pre-existing calibration (default text sat at depth 0). Content above
  text gets a positive offset, below gets negative — a symmetric budget growing from
  0, with no clamp.
- **One signed z-index, uniform across types.** A single per-element field is simpler
  than the old text-only cascade (per-label override + panel default + global default,
  three lookups per run) and applies identically to fills, borders, images, lines, and
  text. Explicit, non-inheriting authoring avoids accidental level propagation.
- **Per-level banding on the screen view.** With one global text lane, a raised text
  run still landed in that lane, not above default fills. Per-level bands give each
  z-level its own fill/line/text sub-lanes, so z-index moves *batched* content on the
  non-OIT screen view, not only on OIT — at the cost of the 64-lane-per-level ceiling.

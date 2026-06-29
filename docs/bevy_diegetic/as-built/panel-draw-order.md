# Panel draw order

## What it is

A CSS-style single ordering axis for the elements inside a diegetic panel. It
replaced the flat `draw_slot` emission counter (a per-panel integer bumped on
every filled element), the text-only `DEFAULT_DRAW_LAYER = 64` global layer, and
the OIT depth clamp that limited reordering. Three inputs — a fixed per-kind draw
step (`Fill < Shapes < Text`), declaration order in the layout tree (`tree_order`),
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
- `step.ordinal()` = `Fill (0) < Shapes (1) < Text (2)`, fixed per
  `RenderCommandKind` (`layout/render.rs`, `RenderCommandKind::draw_step()`)
- `tree_order` = the command's index in the layout-DFS stream

**Enumeration → projection.** `enumerate_ordinals(&[RenderCommand]) ->
Vec<Option<DrawOrdinal>>` sorts the draw-participating commands by the key and
assigns each a panel-wide dense rank (`None` for scissors), index-aligned with the stream.
`DrawOrderProjection::from_commands` wraps this into per-command
`DrawCommandDepth { ordinal, z_level, screen_depth_bias, oit_depth_offset }`. It
computes the panel's `text_anchor` (the lowest ordinal among `Text`-step commands)
once, so the lowest text command lands at OIT offset `0.0`.

**Concrete example.** Layout first emits commands in layout-tree order. These are
the raw `LayoutResult::commands` indices:

| `command_index` | `RenderCommandKind` | `draw_step()` | `DrawZIndex` | Draw-participating? |
| ---: | --- | --- | ---: | --- |
| 0 | `Rectangle` | `Fill` | 0 | yes |
| 1 | `Text` | `Text` | 0 | yes |
| 2 | `Rectangle` | `Fill` | 0 | yes |
| 3 | `Text` | `Text` | -1 | yes |
| 4 | `ScissorStart` | `None` | 0 | no |
| 5 | `Shapes` | `Shapes` | 1 | yes |
| 6 | `Text` | `Text` | 1 | yes |

`DrawOrderProjection::from_commands` then sorts only the draw-participating
commands by `(z_level, step.ordinal(), tree_order)`. `DrawOrdinal` is the
panel-wide sorted rank. `level_ordinal` is the rank within the current `z_level`
only; it resets when `z_level` changes and is not stored in the final
`DrawCommandDepth`.

| Sorted rank (`DrawOrdinal`) | `command_index` | Sort key `(z_level, step, tree_order)` | `level_ordinal` | `ScreenDepthBias` math | `OitDepthOffset` math |
| ---: | ---: | --- | ---: | --- | --- |
| 0 | 3 | `(-1, Text, 3)` | 0 | `-1 * DRAW_LEVEL_STRIDE + DRAW_LEVEL_GEOMETRY_START_SUBLANE + 0 = -65` | `(0 - text_anchor) * OIT_DEPTH_STEP = 0` |
| 1 | 0 | `(0, Fill, 0)` | 0 | `0 * DRAW_LEVEL_STRIDE + DRAW_LEVEL_GEOMETRY_START_SUBLANE + 0 = 1` | `(1 - text_anchor) * OIT_DEPTH_STEP` |
| 2 | 2 | `(0, Fill, 2)` | 1 | `0 * DRAW_LEVEL_STRIDE + DRAW_LEVEL_GEOMETRY_START_SUBLANE + 1 = 2` | `(2 - text_anchor) * OIT_DEPTH_STEP` |
| 3 | 1 | `(0, Text, 1)` | 2 | `0 * DRAW_LEVEL_STRIDE + DRAW_LEVEL_GEOMETRY_START_SUBLANE + 2 = 3` | `(3 - text_anchor) * OIT_DEPTH_STEP` |
| 4 | 5 | `(1, Shapes, 5)` | 0 | `1 * DRAW_LEVEL_STRIDE + DRAW_LEVEL_GEOMETRY_START_SUBLANE + 0 = 67` | `(4 - text_anchor) * OIT_DEPTH_STEP` |
| 5 | 6 | `(1, Text, 6)` | 1 | `1 * DRAW_LEVEL_STRIDE + DRAW_LEVEL_GEOMETRY_START_SUBLANE + 1 = 68` | `(5 - text_anchor) * OIT_DEPTH_STEP` |

`tree_order` only breaks ties inside the same `z_level` and `DrawStep`. In the
example, command `1` is text at `DrawZIndex(0)`, so it sorts after command `2`
even though command `2` was emitted later. In this example, `text_anchor` is `0`
because the first sorted command is also the lowest text command.

**Two depth axes from one ordinal.**
- **Screen (non-OIT) view:** `screen_depth_bias = geometry_depth_bias(z_level,
  level_ordinal)` places level `L` into the `depth_bias` window `[L *
  DRAW_LEVEL_STRIDE, (L + 1) * DRAW_LEVEL_STRIDE)`. Internally,
  `geometry_depth_bias` calls `level_sublane_depth_bias` with
  `DRAW_LEVEL_GEOMETRY_START_SUBLANE + level_ordinal`. A batch is one draw and
  blends in buffer order, so each z-level gets a fixed screen lane.
- **OIT (world) view:** `oit_depth_offset = (ordinal − text_anchor) ×
  OIT_DEPTH_STEP`, computed by `text_anchored_oit_depth_offset`. Per-fragment sort
  means submission order is irrelevant; the offset alone resolves coplanar order.

**Per-level band layout.** Within level `L`:
- The **SDF fill batch** takes `DRAW_LEVEL_FILL_SUBLANE = 0` via
  `fill_batch_depth_bias(z_level)`.
- Per-command ordering uses `DRAW_LEVEL_GEOMETRY_START_SUBLANE..=
  DRAW_LEVEL_GEOMETRY_START_SUBLANE + DRAW_LEVEL_GEOMETRY_LANES - 1`
  (`1..=64`) through `geometry_depth_bias(z_level, level_ordinal)`.
- The **panel-shape line batch** takes sub-lane `64` via
  `line_batch_depth_bias(z_level)`.
- The **text batch** takes `DRAW_LEVEL_TEXT_SUBLANE = 65` via
  `text_batch_depth_bias(z_level)`.
- `DRAW_LEVEL_STRIDE = 66`, so the next `DrawZIndex` level starts after text.

**Batched vs. individual draws.** SDF fills/borders (`render/fill_batch.rs`), panel
shapes (`render/panel_shapes/batching.rs`), and text
(`render/panel_text/batching.rs`) are vertex-pulled batches. Each splits its
batches by z-level (`z_level` on `SdfBatchKey` or `PathBatchKey`), so a
raised/lowered run spawns its own batch in the matching band. Per-record
`depth_nudge` disambiguates within a batch on the non-OIT depth-buffer axis;
per-record `oit_depth_offset` disambiguates within a batch on the OIT axis.
Images and precomposed LDR leaves remain individual panel children stamped with
their command's full `DrawCommandDepth`.

**Reconcile identity.** A z-index or step change affects ordering only — it re-keys
a record, never respawns the entity. Text-run identity stays `(PanelElementId,
line_index)`; image identity is `command_index`; SDF fill records retain
`draw_depth: DrawCommandDepth` before upload and are keyed by
`SdfRecordKey { panel, command_index }`. Reuse signatures store the whole
`DrawCommandDepth` (not a bare ordinal) so a `text_anchor` shift — toggling text
on/off — invalidates reuse instead of leaving a stale `oit_depth_offset`.

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
  draw-participating commands at one z-level. At `level_ordinal == 63`, a command
  reaches the panel-shape line batch sub-lane. At `level_ordinal == 64`, it reaches
  the text batch sub-lane. At `level_ordinal == 65`, it spills into the next
  `DrawZIndex` band. `DRAW_LEVEL_STRIDE = DRAW_LEVEL_TEXT_SUBLANE + 1 = 66`. This
  is a real screen-side limit, separate from the OIT budget.
- **OIT budget.** `oit_depth_budget() = floor(OIT_FOCUS_DEPTH / OIT_DEPTH_STEP)` ≈
  1000 commands panel-wide (`OIT_FOCUS_DEPTH = 0.001`, `OIT_DEPTH_STEP = 1e-6`).
- **Overflow guard.** `warn_panel_draw_order_limits` in
  `render/draw_order_limits.rs` checks `per_level_band_overflows` (busiest single
  z-level vs. `per_level_band_capacity()`) and `oit_total_overflows` (panel total
  vs. `oit_depth_budget()`) independently. Per-level occupancy comes from
  `DrawOrderProjection::level_occupancy() -> Vec<(i8, usize)>`. The per-level
  warning is conservative: it counts commands, not bounding-box overlap.
- **Shader floor.** `OIT_MIN_DEPTH = 3e-6` is hard-coded in the SDF and analytic
  fragment shaders (`shaders/sdf_panel.wgsl`, `render/analytic_paths/analytic_path.wgsl`).
  The `EXPECTED_SHADER_FNV1A` tripwire (`text/slug/glyph/coverage_probe.rs`) hashes
  **only** `analytic_path.wgsl`; editing that shader requires pasting the printed
  new hash in the same change.
- **Lane-boundary tie.** A command at `level_ordinal == 63` shares the panel-shape
  line batch sub-lane. The warning starts at that boundary because further
  per-command depth-bias values enter the shared batch sub-lanes and then the next
  `DrawZIndex` band.
- **Two-axis split is the precedent.** Panel shapes and text are the worked examples of
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
  preserves the pre-existing calibration. Content above
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

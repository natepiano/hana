# Panel draw order

> **Status: DESIGN — ready to implement, phased.** A successor to the current
> draw-layer model recorded in
> [`as-built/text-draw-layer.md`](as-built/text-draw-layer.md). It replaces the
> `draw_slot` emission counter, the `DEFAULT_DRAW_LAYER = 64` text default, and
> the OIT clamp with a CSS-style model: one ordering axis with a fixed
> per-element draw order (background → fills → text) plus a single signed
> `DrawZIndex` override that applies to any element. All product decisions
> (D1–D6) are resolved; the types and the 6-commit phase plan below are
> concrete. The flag-day type rename (`DrawLayer` → `DrawZIndex`) goes through
> the editor — get explicit API approval before landing it.

## Why ordering exists at all

Panel content is translucent, and that is the only reason any draw-order
machinery exists.

- **Opaque fragments sort themselves.** The GPU depth test keeps the nearest
  fragment and discards the rest; draw order does not change the result.
- **Translucent fragments must blend in order.** "Dim shade over text over
  background" only looks right blended back-to-front. Two translucent fragments
  at the *same* depth blend in an undefined order — the failure mode behind the
  OIT clamp, the `OIT_MIN_DEPTH` floor, and the flashing-squares lineage.

Three things make panel content translucent:

1. `TextAlpha` defaults to `AlphaMode::Blend` (`cascade/resolved.rs`), so every
   text run blends.
2. Panel fills are `AlphaMode::Blend` (`render/panel_geometry.rs`) and are
   usually authored with alpha < 1.
3. A glyph is a coverage mask — its edges are partial alpha. Smooth text
   *requires* blending; an opaque alpha mode would give hard, aliased edges. So
   text is translucent at its edges regardless of the default.

If panels were fully opaque the depth buffer would order everything and none of
this would be needed. They are not, so the order things blend in is visible and
must be controlled.

## What is wrong with the current model

The current model (see the as-built doc) puts everything on one flat integer
axis, `DrawOrdinal(i32)`:

- **Geometry** takes an emission-order `draw_slot` (0, 1, 2 …).
- **Text** takes an authorable `DrawLayer`, default `64`.
- Both convert into `DrawOrdinal`, which yields the screen sort bias
  (`depth_bias`) and the OIT depth offset.

Three problems:

1. **Overflow risk.** A complex panel — future widgets, many primitives — can
   emit more than 64 geometry commands. Their slots then reach or pass the text
   default and text stops being reliably on top. The flat axis has a ceiling.
2. **Two reconciled axes.** Geometry uses paint order; text uses an authorable
   layer pinned high. The `64` default and the OIT clamp
   (`min(ordinal − 64, 0)`) exist only to glue these two axes together so text
   stays above geometry without counting.
3. **Text-only authorability.** Only text carries a settable position on the
   axis. Geometry is locked to the low slots, so you cannot author a geometry
   overlay *above* text within one panel — the original intent behind this
   feature. The `text_draw_layer` example fakes it with a second floating panel.

## The borrowed model: CSS paint order

Browsers solve this without a global emission counter. We borrow three of the
concepts and deliberately leave out the fourth (stacking contexts).

**Fixed draw order.** Inside any one element, its own draws always order the
same way: fill/border, then panel lines, then text. The implemented steps are
`Fill → Lines → Text` (see [Concrete types](#concrete-types)). "Child boxes"
order *between* a parent's fill and its text, but they are not a separate step —
a child box is just another element's `Fill` emitted later in the tree, so tree
order places it above the parent fill and below all text automatically. "Raised"
is likewise not a step — it is the effect of `DrawZIndex > 0`.

**Later-wins.** When two draws sit in the same step, the one emitted later in the
layout tree draws on top.

**`DrawZIndex` (raise number).** An optional **signed** integer on any element.
Positive raises it, negative lowers it. When set, it is the primary sort key, so
it overrides the fixed draw order; unset elements compete at the implicit zero
level. It is symmetric: "background over text" can be reached either by raising
the background or by lowering the text. It competes **panel-wide** — one
element's fill can be raised above another element's text without restructuring
the tree.

**What we leave out: stacking contexts (sealed groups).** CSS seals a
translucent subtree so it composites as one unit and cannot interleave with
outside fragments. That only changes the result when a single opacity is applied
to a whole subtree of *overlapping* elements, which the panel system has no way
to express (alpha is per-element). bevy's OIT is a single global per-pixel depth
sort with no group concept, so a sealed group is not even implementable on it
without separate render targets. So the whole panel is one ordering axis — a
single implicit stacking context. (See D1 for the full rationale.)

These map to CSS's painting order (CSS 2.1 Appendix E): the 7 CSS buckets
collapse to our 3 steps because panels have no floats and no separate
positioned/inline distinction, and we run a single context instead of nesting
them.

## How it maps to a panel

- Each element contributes draws in the fixed draw order (fill/border → lines →
  text).
- Ties within a step break by tree order (later-wins).
- The whole panel is one axis — no per-group reset. A set `DrawZIndex` sorts
  ahead of the fixed step, panel-wide; lowered content sinks behind every fill
  it overlaps, including a sibling's.
- The per-element ordinal feeds the same two outputs as today — a screen sort
  bias and an OIT depth offset — but is built from a `HierarchicalDrawKey`
  `(z_index, step, tree_order)` projected to one dense ordinal, instead of a
  flat emission counter.

The overflow ceiling is gone not because numbering is bounded, but because text
sits at the *text step*, always ahead of the *fill step* by construction. The
current ceiling came from text being pinned at a fixed number (`64`) that
geometry slots could climb to; a semantic step has no number to reach. (The OIT
budget still caps how many *distinct coplanar* ordinals one panel can resolve —
that bound is preserved as the overflow check, see Phase 5.)

Panel lines render text-like (batched, vertex-pulled), so they sit in a fixed
step between fills and text rather than carrying a per-command material depth.
They do not gain `DrawZIndex` unless a later pass adds it; only their step
placement changes.

## What changes from the current model

| Change | Piece today | Today | Becomes |
|---|---|---|---|
| Remove | `DEFAULT_DRAW_LAYER = 64` | text's starting number | gone — text is on top by the fixed text step |
| Remove | OIT clamp `min(ordinal − 64, 0)` | squashes high text numbers | gone — symmetric offset around the text anchor (D5) |
| Remove | global `draw_slot` counter | one running count per panel | gone — `tree_order` is the command's index in the stream |
| Replace | `RenderCommand::draw_slot: usize` | emission slot | `z_index: Option<DrawZIndex>` + derived `step` + index-as-`tree_order` |
| Replace | `consumes_draw_slot()` | which kinds advance the counter | `draw_step()` → which fixed `DrawStep` a command belongs to |
| Replace | `DrawLayer(i8)`, text-only | layer on `TextStyle` | signed `DrawZIndex(i8)` on any element, panel-wide |
| Replace | `BATCH_PANEL_LINE_DEPTH_BIAS = 63` | fixed line lane under text | coarse lane rederived from `DrawStep::Lines` (above fills, below text) |
| Replace | `DrawOrdinal(i32)` + converters | flat axis | enumerated ordinal from `HierarchicalDrawKey`, projected to `depth_bias` + `oit_depth_offset` unchanged |
| Add | — | geometry order automatic only | geometry raisable above text via `DrawZIndex` |
| Keep | `OIT_DEPTH_STEP`, `LAYER_DEPTH_BIAS` | step magnitudes | kept; same focus-depth budget, degrades gracefully past it |
| Keep/rename | cascade verbs `override_draw_layer` … | text-layer verbs | become the any-element `DrawZIndex` verbs (auto-generated by `cascade_attr!`) |

## OIT depth offset — calibration to preserve

Carried from the as-built doc, because the successor still needs it. The world
view renders under OIT (`StableTransparency` on the orbit camera); the OIT
fragment offset is added to `position.z` in the shader before `oit_draw`, since
pipeline `depth_bias` does not affect `in.position.z`.

- `bevy_lagrange` syncs the perspective near plane to `radius × 0.001`, so a
  fragment at the camera's focus distance has `position.z = near / d ≈ 0.001`.
- The largest offset magnitude must stay well below that focus depth, or the
  offset drives `position.z` non-positive and `pack_24bit_depth_8bit_alpha`
  saturates it to the cleared-background depth, where bevy's resolve pass drops
  every fragment with alpha < 1.
- At `OIT_DEPTH_STEP = 1e-6`, a 64-ordinal span totals `6.4e-5` (6.4% of the
  focus depth) and one step spans ~17 quanta of the 24-bit OIT depth packing, so
  adjacent ordinals stay distinct.
- The `OIT_MIN_DEPTH` floor in `sdf_panel.wgsl`, `analytic_path.wgsl`, and
  `panel_line_batch.wgsl` keeps far-panel fragments storable; past the bound
  (z = near/d crosses the budget at ~15.6× the orbit radius) coplanar ordering
  collapses to OIT-list insertion order rather than going invisible.

**Successor note:** layering *between steps* no longer depends on the offset
budget — text sits at the text *step*, always ahead of the fill step, so the
budget governs only fine coplanar disambiguation between same-step fragments and
the `DrawZIndex` raise/lower span. Past the focus-depth bound those collapse to
OIT-list insertion order — the same graceful degradation far panels already
show — never a step inversion. Keep each panel's *used* z-index range small
(≈±5) so the per-panel ordinal span fits the budget; the `i8` width is a type
bound, not a per-panel budget.

## Concrete types

The single load-bearing requirement: ordering is **fully encoded in depth**,
never delegated to draw-submission order. Panel text and lines are *batched* —
glyph and line records concatenate in ECS archetype-storage (query-iteration)
order, not tree order (`render/panel_text/batching.rs`,
`render/analytic_paths/batching.rs`). bevy's `oit_resolve.wgsl` does tie-break
equal-depth fragments by insertion order, but insertion order is archetype
order, not tree order, so any ordering left to it would be wrong on OIT world
panels. Every intended order difference must therefore land in `depth_bias` and
`oit_depth_offset`.

**`DrawStep`** — the fixed per-command step, derived from `RenderCommandKind`:

```rust
enum DrawStep { Fill, Lines, Text }   // ordinal() = 0, 1, 2

// RenderCommandKind::draw_step(&self) -> Option<DrawStep>
//   Rectangle | Border | Image => Some(Fill)
//   Lines                      => Some(Lines)
//   Text                       => Some(Text)
//   ScissorStart | ScissorEnd  => None   // do not draw, do not order
```

Use an explicit `ordinal()` mapping (a `match`), not the derived discriminant,
so reordering variants cannot silently invert the ladder. `Fill < Lines < Text`
is confirmed against the current coarse lanes (lines at
`BATCH_PANEL_LINE_DEPTH_BIAS = 63`, just under text `64`). No `ChildBoxes`
variant: a child box is a child element's `Fill` emitted later, so `tree_order`
already places it above the parent fill and below text. No `Raised` variant:
that is the `DrawZIndex > 0` effect, carried by the key's primary axis.

**`DrawZIndex`** — `i8`, signed, `Option`-wrapped on the element:

```rust
struct DrawZIndex(i8);   // i8 avoids the bevy::prelude::ZIndex(i32) clash
// authored as Option<DrawZIndex>: None = the implicit zero level, never a 0 sentinel
```

A set `DrawZIndex(0)` and unset are behaviorally identical (both compete at the
zero level via step); `Option` keeps authoring intent explicit and stops a `0`
sentinel leaking through reflection/BRP.

**`HierarchicalDrawKey`** — per command, with a custom 2-level `Ord`:

```rust
struct HierarchicalDrawKey {
    z_index:    Option<DrawZIndex>,  // None = auto, treated as level 0
    step:       DrawStep,
    tree_order: u32,                 // command index in the RenderCommand stream
}

// Ord: lexicographic (z_level, step.ordinal(), tree_order), z_level = z_index.unwrap_or(0)
//   za.cmp(&zb)
//     .then(self.step.ordinal().cmp(&other.step.ordinal()))
//     .then(self.tree_order.cmp(&other.tree_order))
```

The 2-level key is required, not the single-axis `z_index.unwrap_or(step.ordinal())`:
collapsing z-level and step onto one axis makes a set `z = 2` tie with unset
`Text` (both → 2) instead of sitting above it. With the 2-level key:

- unset `Text` `(0, Text)` beats `z = 0` `Fill` `(0, Fill)` — text over fills.
- `z = 2` `Fill` `(2, Fill)` beats unset `Text` `(0, Text)` — raise above text (D5).
- `z = −1` `Text` `(−1, Text)` sinks below unset `Fill` `(0, Fill)` — lower behind fills.

**`tree_order`** is the command's index in the flat `RenderCommand` stream — the
layout DFS down/up traversal in `layout/engine/positioning.rs`, not ECS child
order. This is the only definition of "later-wins" stable through batching.

**Projection to one ordinal.** Per panel, sort the draw-participating commands
(`draw_step().is_some()`) by `HierarchicalDrawKey`, then assign each a dense
enumerated ordinal `0..N`. That single ordinal feeds **both**
`depth_bias = ordinal × LAYER_DEPTH_BIAS` and
`oit_depth_offset = (ordinal − text_anchor) × OIT_DEPTH_STEP`, exactly as the
current `DrawOrdinal` does — so sorted/OIT parity is preserved by construction,
not reduced. `text_anchor` is the lowest ordinal among `Text`-step commands, so
default text lands at OIT offset `0.0` (preserving the current calibration) and
raised content goes positive, lowered negative — the D5 symmetric offset, no
clamp.

## Invariants to preserve

Regression guards the implementation must not break:

- **Sorted/OIT parity.** Any two commands order the same way on the sorted
  screen view (`depth_bias`) and the OIT world view (`oit_depth_offset`). The
  enumerated-ordinal projection preserves this; the
  `sorted_and_oit_orderings_agree_for_every_layer_pair` test
  (`render/constants.rs:196`) generalizes to `HierarchicalDrawKey` pairs.
- **Cross-panel anchoring.** `DrawZIndex` is panel-scoped and must never reorder
  one panel's children against another panel's. The per-panel `depth_bias` span
  (max ordinal × `LAYER_DEPTH_BIAS`) must stay below the minimum panel-distance
  `Transparent3d` separation the screen-anchoring feature relies on. Keep *used*
  z-index levels compressed into a small span; do not map the full `i8` ±127
  range directly. Re-derive against the as-built 64-pixel threshold.
- **OIT focus-depth budget.** The per-panel ordinal span × `OIT_DEPTH_STEP` must
  stay inside `6.4e-5`. Tie `OIT_MIN_DEPTH` to `OIT_DEPTH_STEP` (e.g.
  `3 × OIT_DEPTH_STEP`) so the floor tracks calibration instead of a hard-coded
  `2e-7`.
- **Callout band separation.** Callouts keep their own positive-offset OIT axis
  and stay above all panel content; the panel `HierarchicalDrawKey` does not
  cover callouts.
- **Reconcile identity.** A `DrawZIndex` or step change affects ordering only;
  text-run identity stays keyed on `(PanelFieldId, line_index)` and image
  identity on `element_idx` (the move rebuilds the material, never respawns the
  entity). Per-line scope: `override_draw_zindex` on a wrapped run applies per
  line entity; whole-run changes must address all lines by run id.

## Implementation phases

Six separable commits, each building green (`cargo build && cargo +nightly fmt`,
`cargo nextest run`). The `DrawLayer` → `DrawZIndex` rename is the final phase,
so phases 1–5 keep the current name and the model stays semantically honest
(default `64` belongs to the old text-layer model and is only deleted when the
new model lands).

### Phase 1 — `DrawStep`, inert

*Commit:* add the step enum and per-kind mapping beside the existing bool; no
reads, no behavior change.

- `layout/render.rs` — add `DrawStep { Fill, Lines, Text }` + `ordinal()`; add
  `RenderCommandKind::draw_step(&self) -> Option<DrawStep>` (`:50–87` for the
  enum, beside `consumes_draw_slot()` at `:94–102`). Keep `consumes_draw_slot()`.
- Unit test: every `RenderCommandKind` variant maps to the expected step (and
  scissors to `None`).

*Gate:* compiles; nothing reads `draw_step()` yet; existing tests unchanged.
*Status:* done — committed `474382b` (bundled with Phase 2).

### Phase 2 — `Option<DrawLayer>` on `El`/`Element` + emission stamps `z_index`, inert

*Commit:* the any-element authoring field and emission plumbing, still unread by
render.

- `layout/builder.rs` — add `draw_layer: Option<DrawLayer>` to `El`
  (struct `:63–82`) + a `.draw_layer(self, DrawLayer) -> Self` builder mirroring
  `.draw()` (`:250–253`).
- `layout/element.rs` — add `pub(super) draw_layer: Option<DrawLayer>` to
  `Element` (`:76–121`); plumb `El → Element`.
- `layout/render.rs` — add `z_index: Option<DrawLayer>` to `RenderCommand`
  (`:17–33`), beside the still-present `draw_slot`.
- `layout/engine/positioning.rs` — `push_command` (`:41–58`) stamps `z_index`
  from the element's field; keep the `draw_slot` counter
  (`EmissionCounters`, `:33–36`) running in parallel.
- `render/clip.rs` — scissor construction sets `z_index: None` (`:118–134`).

*Gate:* compiles; render still reads `draw_slot`; field is inert.
*Status:* done — committed `474382b`.

### Phase 3 — `HierarchicalDrawKey` + projection, computed in parallel and validated

*Commit:* the key, its `Ord`, and the panel-level enumeration — computed and
asserted equal to the current ordering, but not yet driving render.

- `render/constants.rs` — add `HierarchicalDrawKey` + the 2-level `Ord`; add a
  panel-level `fn enumerate_ordinals(&[RenderCommand]) -> Vec<DrawOrdinal>` that
  sorts draw-participating commands by key and assigns dense ordinals, with the
  `text_anchor`-relative `oit_depth_offset`. `tree_order` = the command's index
  in the stream (`.enumerate()`), not the `draw_slot` counter.
- Parity test: for representative panels, the new enumeration reproduces the
  current `draw_slot`/`DrawLayer` relative order (so the flip in Phase 4 is a
  no-op for existing content, and only new `DrawZIndex` authoring changes order).

*Gate:* compiles; new ordinal computed and asserted against the old; render
still reads `draw_slot`.
*Status:* done — uncommitted in tree (`render/constants.rs` + a `pub(crate) use
render::DrawStep` re-export in `layout/mod.rs`). `enumerate_ordinals` is
`#[cfg_attr(not(test), expect(dead_code, …))]` until Phase 4 reads it.

#### Retrospective (Phases 1–3)

**What worked:**
- Inert-by-phase sequencing held: each phase compiled green with zero render
  reads, so the new model accreted beside the current `draw_slot` path without
  touching behavior. 404 tests pass.
- The current order turned out to already be step-grouped at the *coarse* lanes
  (`Fill` `draw_slot` `0..62` < `Lines` `63` < `Text` `64`), so the new
  `(z_level, step, tree_order)` key reproduces it — the parity oracle keys `Fill`
  by `draw_slot`, `Lines` by the `63` batch lane, `Text` by `64`, and compares
  order (pairwise sign), not magnitudes.

**What deviated from the plan:**
- `DrawStep` was private to `layout`; Phase 3 added a `pub(crate) use
  render::DrawStep` re-export in `layout/mod.rs` (one file beyond the planned
  `constants.rs`-only scope) so `HierarchicalDrawKey` can store `step: DrawStep`.
- `enumerate_ordinals` returns `Vec<Option<DrawOrdinal>>` (index-aligned, `None`
  for scissors), not the doc's sketched `Vec<DrawOrdinal>`, so Phase 4 can recover
  each command's ordinal by stream position.
- Phase 2's `classify_element_change` ignores the new field (`draw_layer: _`)
  while inert; the comparison is deferred to Phase 4 (already recorded in the
  Phase 4 bullets) so a `.draw_layer()`-only change re-emits once render reads it.

**Surprises:**
- Lane-collision boundary: the overflow guard (`panel_geometry.rs:237`) rejects
  `draw_slot ≥ 64` but *allows* `63`, where a `Fill` ties the `Lines` lane. Old
  code leaves that tie to submission order; the new key deterministically orders
  `Fill` below `Lines` (the documented lane intent). So Phase 4 is a true no-op
  only for `draw_slot < 63`; the `== 63` case is a deliberate tie-resolution, now
  pinned by `level_zero_fill_stays_below_lines_at_lane_boundary`.

**Implications for remaining phases:**
- Phase 4 must read `enumerate_ordinals` (the `expect(dead_code)` attr comes off
  then) and wire the `classify_element_change` `draw_layer` comparison.
- Phase 4's "existing panels render unchanged" gate should be stated as holding
  for `draw_slot < 63` (the lane boundary is an intended, tested resolution).

#### Review of remaining phases (post-Phase-3)

An architect pass over phases 4–6 changed the plan as follows:

- **Phase 4 — line ordering source.** The per-record line offsets are NOT "fine
  disambiguation" — they are `draw_slot`-derived (`PanelLinePaintOrder::Normal`,
  `line.rs:525–532`). Phase 4 must re-derive them from the enumerated ordinal.
- **Phase 4 — reconcile re-keying widened.** `PanelSdfSurface`, `PanelTextChild`,
  and `PanelImageChild` each key on `draw_slot`; all move to the ordinal, not just
  the image rebuild.
- **Phase 4 — single z-index source (user decision).** Every command takes its
  level from its own element's `z_index`; the per-label `DrawLayer` cascade read
  (`batching.rs:238`) is deleted. No inheritance — that was old-model baggage.
- **Phase 4 — `OIT_MIN_DEPTH` retuned (user decision).** Honor the invariant:
  `3 × OIT_DEPTH_STEP` in the 3 shaders + `EXPECTED_SHADER_FNV1A` refresh, same
  commit, for codebase consistency.
- **Phase 4 — gate strengthened.** Add a render-level material-value equivalence
  check (CPU ordinal parity from Phase 3 does not cover the per-pass rewrite).
- **Phase 5 — `draw_slot` deletion inventory completed.** `positioning.rs:314`,
  `PanelLinePaintOrder::Normal`/`NORMAL_*_STEP`, and the per-carrier fields were
  missing; overflow count now sourced from `enumerate_ordinals`.
- **Phase 6 — estimates refreshed.** Blast radius ~172 refs/17 files (not 227/21);
  re-verify line citations at rename time; example path corrected; FNV refresh
  moved to Phase 4 (no longer a Phase-6 no-op).

### Phase 4 — Flip render reads to the enumerated ordinal (behavior change)

*Commit:* render derives depth from `HierarchicalDrawKey`; in-panel overlay and
D5 (raise above text on OIT) start working. `draw_slot` survives only as the
emission-order input feeding `tree_order`.

- `render/panel_geometry.rs` — `:473/475/552` derive `depth_bias` /
  `oit_depth_offset` from the enumerated ordinal instead of
  `DrawOrdinal::from_draw_slot`.
- `render/panel_text/batching.rs` — replace the per-run `depth_nudge` from
  `draw_slot` (`:264`) and the per-run `oit_depth_offset` from
  `DrawOrdinal::from(draw_layer)` (`:255`) with the unified ordinal; rederive the
  coarse batch lane (`:749` `DrawOrdinal::from(DrawLayer(key.layer))`) from
  `DrawStep::Text` instead of `DEFAULT_DRAW_LAYER`.
- `render/panel_lines/batching.rs` + `render/analytic_paths/batching.rs` —
  rederive the coarse `BATCH_PANEL_LINE_DEPTH_BIAS` lane (`:614`) from
  `DrawStep::Lines`. **The per-record line offsets are NOT fine disambiguation —
  they are `draw_slot`-derived coarse offsets** (`PanelLinePaintOrder::Normal {
  draw_slot }` seeded at `positioning.rs:314` → `line.rs:525–532` derives
  `depth_bias = draw_slot × NORMAL_DEPTH_BIAS_STEP(1.0)` and
  `oit_depth_offset = (draw_slot+1) × NORMAL_OIT_DEPTH_STEP(−1e-6)`, applied at
  `:654–677`/`:72–75`). They read `draw_slot` (deleted in Phase 5), so Phase 4
  must re-derive the per-record line/part depth from the enumerated ordinal (or
  `tree_order` within the `Lines` step), not retain the `draw_slot` formula.
- `render/constants.rs` — delete the `min(ordinal − 64, 0)` clamp in
  `oit_depth_offset` (`:86–88`); the symmetric `text_anchor`-relative offset
  from Phase 3 replaces it (D5).
- **Retune `OIT_MIN_DEPTH` (decision: honor the invariant).** Replace the
  hard-coded `OIT_MIN_DEPTH = 2e-7` with `3 × OIT_DEPTH_STEP` in all three
  shaders (`sdf_panel.wgsl`, `analytic_path.wgsl`, `panel_line_batch.wgsl`) so the
  floor tracks calibration. The `.wgsl` text changes, so refresh
  `EXPECTED_SHADER_FNV1A` (`coverage_probe.rs ~:871`) **in this same commit** —
  Phase 4 must stay green. (Numerically the old `2e-7` was adequate; this is for
  codebase consistency — the floor and the step now derive from one constant.)
- `render/panel_text/reconcile.rs` — re-key image-material rebuild on the
  ordinal/step instead of `draw_slot` (`:587–589`, material build `:642`); text
  reuse key `(PanelFieldId, line_index)` is unchanged.
- **Three reconcile-identity carriers also key on `draw_slot` and must move to the
  ordinal/step** (the "Reconcile identity" invariant names text/image but missed
  the SDF surface): `PanelSdfSurface.draw_slot` in the geometry-eq signature
  (`panel_geometry.rs:48/131/561`), `PanelTextChild.draw_slot`
  (`panel_text/layout.rs:26`), and `PanelImageChild.draw_slot` in the
  `visuals_unchanged` reuse test (`reconcile.rs:427/494/588`). Re-key each on the
  enumerated ordinal (or drop it) in this phase, or reconcile reuse/respawn breaks
  when Phase 5 deletes the field.
- `layout/element.rs` — `classify_element_change` must compare `draw_layer`
  (Phase 2 left it destructured as `draw_layer: _`, inert). Once render reads
  `z_index`, a `.draw_layer()`-only authoring change must classify as a
  visual change so the command stream regenerates with the new ordinal —
  otherwise it takes the `Identical` skip (`panel/compute_layout.rs:96`) and the
  panel keeps stale depth. Acceptance: toggling only `draw_layer` re-orders the
  element on screen.
- **Single z-index source (resolved).** Every command — fill, text, line — takes
  its level from its own element's `z_index` (the `Element.draw_layer` field,
  Phase 2), feeding `enumerate_ordinals` directly. The base order is declaration
  order (`tree_order`) + the fixed `DrawStep` ladder; `z_index` is the override.
  There is **no inheritance** — that was an artifact of the old text-only
  `DrawLayer` cascade (a default-`64` layer propagated to label entities), which
  is retired here, not carried forward. So Phase 4 **deletes the per-label cascade
  read** (`render/panel_text/batching.rs:238` reading `Resolved<DrawLayer>`); text
  level no longer comes from `Override`/`Resolved<DrawLayer>`. The `with_draw_layer`
  authoring verb + `glyph_cascade.rs` `Override`/`Resolved<DrawLayer>` resolution +
  the propagation/reconcile arms are old-model machinery removed in Phase 5/6 (add
  them to that deletion inventory). Ergonomic API: one `.draw_zindex(n)` builder on
  any element.

*Gate:* compiles; the in-panel overlay renders above text on **both** the sorted
screen view and the OIT world view; existing panels render unchanged (for
`draw_slot < 63` — the lane boundary is the intended resolution, see
retrospective). The Phase 3 parity test is CPU-only (ordinal order); add a
**render-level equivalence acceptance check** here: for representative panels with
no override, the post-flip per-pass material values (`panel_geometry`
`depth_bias`/`oit_depth_offset`, the text batch lane + per-run nudge, the line
batch lane + per-record offset) must match their pre-flip values — the unified
ordinal replaces three differently-scaled derivations (`LAYER_DEPTH_BIAS`,
`NORMAL_*_STEP`, text `draw_layer`), which the CPU ordinal test does not cover.

### Phase 5 — Delete the dead mechanism + rework the overflow check

*Commit:* remove the now-unreachable old axis; keep the OIT-budget guard.

- Delete `RenderCommandKind::consumes_draw_slot()` (`render.rs:94–102`),
  `RenderCommand::draw_slot` (`:32`), `EmissionCounters.draw_slot`
  (`positioning.rs:33–58`), and `DEFAULT_DRAW_LAYER` (`cascade/constants.rs:20`).
- **Full `draw_slot`-reader inventory to delete/rework (the list above was
  incomplete):** the counter is also read at `positioning.rs:314` to seed
  `PanelLinePaintOrder::Normal { draw_slot }` — delete that variant field plus
  `NORMAL_DEPTH_BIAS_STEP`/`NORMAL_OIT_DEPTH_STEP` and the `line.rs:525–532`
  derivation (their ordering moves to the Phase-4 ordinal). Also drop the
  per-carrier `draw_slot` fields once Phase 4 re-keys reconcile:
  `PanelSdfSurface.draw_slot`, `PanelTextChild.draw_slot`,
  `PanelImageChild.draw_slot`. Deleting `RenderCommand::draw_slot` without these
  will not compile.
- `render/panel_geometry.rs` — rework the overflow check (`:237–253`): it stays,
  but warns when a panel's *distinct coplanar ordinal count* approaches the OIT
  budget (`≈ focus-depth / OIT_DEPTH_STEP`), not when `draw_slot ≥ 64`. Source the
  count from `enumerate_ordinals(...).iter().flatten().count()` (which spans
  fills, lines, AND text), not a reconstructed `draw_slot` max (which counts only
  slot-consuming kinds). Past the budget, ordering degrades to best-effort OIT
  insertion order (the same far-panel degradation the current model has) — no
  silent truncation.

*Gate:* compiles with `draw_slot`/`DEFAULT_DRAW_LAYER` gone; overflow warning
fires only near the OIT budget.

### Phase 6 — Flag-day rename + example + test/doc cleanup

*Commit:* the editor-driven rename and the user-facing deliverables. Blast
radius (re-derive at execution — the earlier "~227 refs / 21 files" had drifted):
as of this review, `DrawLayer` + `draw_layer` is ~172 refs across 17 files
(`DEFAULT_DRAW_LAYER` adds ~24 more), `glyph_cascade.rs` carries 40 (not 41).
`cascade_attr!` regenerates the verbs, `Reflect`, and the BRP type path
automatically, so no hand-written reflection sites. Re-confirm every cited line
number below at rename time — line citations across this plan have drifted.

- Rename via the editor: `DrawLayer` → `DrawZIndex`, `draw_layer` → `draw_zindex`
  (the `El`/`Element` field, `TextStyle` field `text_props.rs:218` + builder
  `:528` + setter `:610`, cascade declaration `cascade/resolved.rs:82–93`, verbs
  `cascade/attributes.rs:51/95/159`, the critical readers `reconcile.rs`,
  `glyph_cascade.rs`, `panel_text/batching.rs`, `constants.rs`). **Confirm the
  rename with the user first** (per the rename-through-editor convention).
- Rewrite the example (`crates/bevy_diegetic/examples/text_draw_layer.rs` → e.g.
  `panel_draw_order.rs`):
  one panel, a text child and a sibling overlay quad in the same tree, ordered
  with `DrawZIndex` and a hotkey toggle — not the current second-anchored-panel
  fake. Depends on the `El` field (Phase 2), so it lands here.
- Rewrite `sorted_and_oit_orderings_agree_for_every_layer_pair`
  (`constants.rs:196–218`) over `(HierarchicalDrawKey, HierarchicalDrawKey)`
  pairs: two unset commands at different steps; unset vs `z = 0` same step;
  unset vs set across steps; raised pairs (D5 symmetric offset).
- `text/slug/glyph/coverage_probe.rs` — `EXPECTED_SHADER_FNV1A` (`~:871`) is
  already refreshed in **Phase 4** (the `OIT_MIN_DEPTH` retuning edits the
  `.wgsl`). Phase 6's `DrawLayer → DrawZIndex` rename does not itself touch shader
  text — `draw_layer` is passed as a precomputed offset through the existing
  shader input — so no further FNV refresh here. The CPU `Probe` mirror models
  coverage, applied before the offset — no structural change.
- Delete `as-built/text-draw-layer.md` once the old mechanism is gone.

*Gate:* compiles under the new names; example demonstrates in-panel overlay;
parity test green over the new key; as-built doc removed.

## Implementation notes

- **Seed every `Resolved<DrawZIndex>` site.** The any-element field means
  emission reads `z_index` per command kind (element, text, line, image). The
  type system cannot catch a forgotten seed — enumerate the sites in the
  checklist; a missed one compiles but renders at the wrong level.
- **`Option<DrawZIndex>` vs `enum { Unset, Set(i8) }`.** The doc commits to
  `Option`. A two-variant enum is a marginally stronger guard against a
  `0`-sentinel leak across reflection/BRP — revisit only if BRP is observed to
  emit a `0` sentinel.
- **Coarse lanes are relative to `DrawStep`, not the deleted constant.** After
  Phase 5, restate the "lines just under text" relationship as
  `DrawStep::Lines < DrawStep::Text`, not as `63 < DEFAULT_DRAW_LAYER`.

## Resolved decisions

D1–D6 are resolved and reflected in the body above; kept here as the rationale
record so a later review does not relitigate them.

- **D1 — Sealed groups: dropped from the model.** Sealed-group compositing only
  matters when a single opacity (or effect) is applied to a whole subtree of
  *overlapping* elements — "fade the group as one unit" vs "fade each element,"
  which differ only in the overlap. The panel system has no group-level opacity
  (alpha is per-element), so sealed groups add no value; bevy's OIT
  (`oit_resolve.wgsl`) has no group concept, and offscreen-target compositing
  would bake resolution-independent text to a raster (aliasing on a world panel
  whose projected size changes every frame). The model is a single global
  ordering axis. **Drops `group` from `HierarchicalDrawKey`** → `(z_index, step,
  tree_order)`. Revisit only if group-level opacity is introduced.
- **D2 — What forms a sealed group: moot.** Dropped with D1.
- **D3 — `DrawZIndex` scope: panel-wide.** A set `DrawZIndex` competes across the
  whole panel axis, not just against siblings; lowered content sinks behind every
  fill it overlaps. In `HierarchicalDrawKey`, the z level is primary (ahead of
  step), unset competes at level 0 via step — the 2-level `Ord`.
- **D4 — Rewrite the example for in-panel overlay.** A deliverable, not a choice
  (Phase 6); the only sub-choice (replace vs rename the file) is minor — pick a
  name matching `DrawZIndex`.
- **D5 — Raise above text on the OIT world view: remove the clamp.** The current
  `min(ordinal − 64, 0)` clamp pins everything at/above text to OIT offset `0.0`,
  so raised content ties with text — and because batching makes OIT insertion
  order *archetype* order, that tie is unreliable, not merely insertion-ordered.
  The symmetric `text_anchor`-relative offset (Phase 3/4) makes raising above
  text work on the OIT world panels, the primary diegetic view. Feasible because
  a panel's *used* z range is small (±5 → span `10e-6`, well inside the `6.4e-5`
  budget); the `i8` ±127 is a type bound, not a per-panel budget.
- **D6 — Parent fill vs descendant text: per-element (CSS way).** `DrawZIndex` on
  a container reorders that container's own fill (and other per-element draws),
  not the whole subtree as a unit: a parent's fill sits below its children's
  content by default, and a `DrawZIndex` on the parent can lift that fill above a
  descendant's text (and, per D3, above unrelated siblings' text). This is what
  the `HierarchicalDrawKey` projection yields naturally (the `Fill` step under
  the `Text` step, lifted by a set z level).

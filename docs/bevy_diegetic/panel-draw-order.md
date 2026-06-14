# Panel draw order

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Replaces the flat
> `draw_slot` emission counter, the `DEFAULT_DRAW_LAYER = 64` text default, and the
> OIT clamp with one CSS-style ordering axis: a fixed per-element draw step
> (`Fill < Lines < Text`) + natural declaration order (`tree_order`) + a single
> optional signed `DrawZIndex` override on any element. These project to one dense
> ordinal per command that feeds both `depth_bias` (sorted screen view) and
> `oit_depth_offset` (OIT world view). The `DrawLayer → DrawZIndex` rename is the
> final phase and goes through the editor with explicit API approval.

## Delegation Context
<!-- Shared across all phases. /delegate prepends this to every dispatch. -->

- **Project:** `bevy_diegetic` — custom Bevy panel renderer with diegetic layout and SDF-based text/geometry rendering.
- **Stack:** Rust 2024 edition + Bevy 0.19.0-rc.2; wgpu 29; batched vertex-pulled text (Slug); OIT (`StableTransparency`) for translucent world panels.
- **Layout:**
  - `layout/` — `builder.rs`, `element.rs`, `render.rs` (commands + `DrawStep`), `engine/positioning.rs`, `line.rs`, `text_props.rs`
  - `render/` — `constants.rs`, `panel_geometry.rs`, `panel_text/` (`batching.rs`, `reconcile.rs`, `layout.rs`, `glyph_cascade.rs`), `panel_lines/batching.rs`, `analytic_paths/batching.rs`, `clip.rs`
  - `cascade/` — `constants.rs`, `resolved.rs`, `attributes.rs`
  - `panel/compute_layout.rs`; `text/slug/glyph/coverage_probe.rs`
  - shaders — `shaders/sdf_panel.wgsl`, `render/analytic_paths/analytic_path.wgsl`, `render/panel_lines/panel_line_batch.wgsl`
  - `examples/text_draw_layer.rs`
- **Key files:**
  - `layout/render.rs` — `RenderCommand` (`:18–36`), `DrawStep`+`ordinal()` (`:51–73`), `RenderCommandKind::draw_step()` (`:119–131`), `consumes_draw_slot()` (`:132–145`)
  - `layout/builder.rs` — `El.draw_layer` field (`:81`), `.draw_layer()` builder (`:258`)
  - `layout/element.rs` — `Element` struct, `classify_element_change()` (`~:644`, `draw_layer` currently destructured `_`)
  - `layout/engine/positioning.rs` — `EmissionCounters` (`:35–36`), `push_command()` (`:43–60`), `PanelLinePaintOrder::Normal{draw_slot}` seed (`:314`)
  - `layout/line.rs` — `PanelLinePaintOrder` (`:111–128`), `NORMAL_DEPTH_BIAS_STEP`/`NORMAL_OIT_DEPTH_STEP`, layering derivation (`:526–532`)
  - `render/constants.rs` — `DrawOrdinal` (`:59`), `HierarchicalDrawKey`+`Ord` (`:62–127`), `enumerate_ordinals` (`:136`), `OIT_DEPTH_STEP` (`:47`), `LAYER_DEPTH_BIAS`, `BATCH_PANEL_LINE_DEPTH_BIAS`, `DrawOrdinal::oit_depth_offset` clamp (`:86–88`), `sorted_and_oit_orderings_agree_for_every_layer_pair` test (`~:557`)
  - `render/panel_geometry.rs` — `PanelSdfSurface`+`draw_slot` (`:42–60`/`:48`/`:131`), eq sig (`:561`), surface build from `cmd.draw_slot` (`:385/398/417`), depth derivation (`~:473/475/552`), overflow guard (`:237–253`)
  - `render/panel_text/batching.rs` — cascade read `cascades.draw_layer(label_entity)` (`:237`) → `DrawOrdinal::from(draw_layer)` (`:238`); per-run `depth_nudge` from `draw_slot` (`:264`) + `oit_depth_offset` (`:255/265`); batch lane `DrawOrdinal::from(DrawLayer(key.layer))` (`:749`)
  - `render/panel_text/reconcile.rs` — image identity (`:427/433`), image-material rebuild keying on `draw_slot` (`:494/588/642`)
  - `render/panel_text/layout.rs` — `PanelTextChild.draw_slot` (`:26`)
  - `render/panel_text/glyph_cascade.rs` — per-label `Override`/`Resolved<DrawLayer>` resolution + propagation (old-model machinery)
  - `render/panel_lines/batching.rs` + `render/analytic_paths/batching.rs` — coarse line batch lane (`~:614`), per-record offsets (`:654–677`)
  - `render/clip.rs` — scissor commands stamp `z_index: None` (`:118–134`)
  - `shaders/sdf_panel.wgsl` / `render/analytic_paths/analytic_path.wgsl` / `render/panel_lines/panel_line_batch.wgsl` — `OIT_MIN_DEPTH` floor (`= 2e-7`)
  - `text/slug/glyph/coverage_probe.rs` — `EXPECTED_SHADER_FNV1A` (`~:871`), hashes **only** `analytic_path.wgsl`
  - `cascade/constants.rs` — `DEFAULT_DRAW_LAYER = 64` (`:20`)
  - `cascade/resolved.rs` — `DrawLayer` cascade declaration (`~:90`); `cascade/attributes.rs` — `override_draw_layer`/`inherit_draw_layer` verbs (`:52/95`)
  - `panel/compute_layout.rs` — element-change classification gate, `Identical` skip (`:96`)
  - `examples/text_draw_layer.rs` — example to rewrite in Phase 6
- **Build:** `cargo build -p bevy_diegetic` (full: `cargo build --workspace --all-features --examples`)
- **Test:** `cargo nextest run -p bevy_diegetic` — **never `cargo test`**
- **Lint:** `cargo clippy -p bevy_diegetic --all-targets` (no new warnings); `cargo +nightly fmt`
- **Style:** `zsh ~/.claude/scripts/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_diegetic_gpu_meter` — obey `[non-negotiable]` rules + forbidden-words list; no rationale-justifying comments; state mechanisms literally.
- **Invariants:**
  - **Sorted/OIT parity.** Any two commands order the same way on `depth_bias` (sorted view) and `oit_depth_offset` (OIT view). The enumerated-ordinal projection preserves this by construction; the `sorted_and_oit_orderings_agree_for_every_layer_pair` test generalizes to `HierarchicalDrawKey` pairs.
  - **Cross-panel anchoring.** `DrawZIndex` is panel-scoped, must never reorder one panel's children against another's. Per-panel `depth_bias` span (max ordinal × `LAYER_DEPTH_BIAS`) stays below the minimum panel-distance `Transparent3d` separation (the as-built 64-pixel threshold). Keep *used* z levels compressed (≈±5); do not map the full `i8` ±127 range.
  - **OIT focus-depth budget.** Near plane = `radius × 0.001` → focus fragment `position.z ≈ 1e-3`. Per-panel ordinal span × `OIT_DEPTH_STEP (1e-6)` must stay inside `6.4e-5`; the offset must never drive `position.z` non-positive (the resolve pass drops alpha<1 fragments there). `OIT_MIN_DEPTH` is tied to `3 × OIT_DEPTH_STEP` so the floor tracks calibration. Past the budget, ordering degrades to OIT-list insertion order — never a step inversion.
  - **Callout band separation.** Callouts keep their own positive-offset OIT axis above all panel content; the panel `HierarchicalDrawKey` does not cover callouts (do not touch).
  - **Reconcile identity.** A `DrawZIndex`/step change affects ordering only: text-run identity stays keyed on `(PanelFieldId, line_index)`, image on `element_idx` (a move rebuilds the material, never respawns the entity). All `draw_slot`-keyed carriers (`PanelSdfSurface`, `PanelTextChild`, `PanelImageChild`) re-key to the ordinal in Phase 4 before the field is deleted in Phase 5.
  - **Build green each phase.** `cargo build && cargo +nightly fmt` + `cargo nextest run` pass before the next phase starts. Newly-unused helpers are gated `#[cfg_attr(not(test), expect(dead_code, …))]`, not deleted, until their deletion phase.
  - **Rename deferred.** `DrawLayer → DrawZIndex` / `draw_layer → draw_zindex` is **Phase 6 only**, through the editor with explicit user approval. Phases 1–5 keep the `DrawLayer` name and the `DEFAULT_DRAW_LAYER = 64` default intact (`64` belongs to the old text-layer model, deleted only when the new model fully lands).

## Phases

### Phase 4 — Flip render reads to the enumerated ordinal · status: implemented (uncommitted) — review found blockers A–D, addressed in Phase 4a

#### Work Order

**Goal:** Render derives all panel depth from the Phase-3 `HierarchicalDrawKey`
projection instead of `draw_slot`/the `DrawLayer` cascade; the in-panel overlay
and D5 (raise above text on the OIT world view) start working. Existing
no-override panels render byte-identical depth values (for `draw_slot < 63`).

**Spec:**

The projection (built in Phase 3, in `render/constants.rs`): per panel, sort the
draw-participating commands (`draw_step().is_some()`) by `HierarchicalDrawKey`
`(z_level, step.ordinal(), tree_order)` and assign each a dense ordinal `0..N`
(`enumerate_ordinals` returns these, index-aligned, `None` for scissors). That
single ordinal feeds **both**:
- `depth_bias = ordinal × LAYER_DEPTH_BIAS`
- `oit_depth_offset = (ordinal − text_anchor) × OIT_DEPTH_STEP`, where
  `text_anchor` is the lowest ordinal among `Text`-step commands (so default text
  lands at OIT offset `0.0`, preserving calibration; raised content positive,
  lowered negative — the D5 symmetric offset, **no clamp**).

`enumerate_ordinals` returns bare ranks; the `text_anchor`-relative offset
currently exists ONLY as a test helper (`text_anchor_rank`, `constants.rs:~384`,
plus the `(rank − text_anchor) × OIT_DEPTH_STEP` formula). **Promote that to
production.** Call `enumerate_ordinals` **exactly once per panel** over the full
`RenderCommand` stream and have every depth derivation source from that single
index-aligned result — do not recompute or approximate per site. The natural
mechanism: stamp each carrier with its command's ordinal at the point the stream
is iterated (where `draw_slot` is stamped today), replacing the stamped
`draw_slot` value with the rank. Another structure (a per-command projection
struct, a side vec threaded to consumers) is fine as long as there is ONE source
and the per-pass material values match pre-flip for no-override panels.

Per-site edits:

- `render/panel_geometry.rs` (`:473/475/552`) — derive `depth_bias` /
  `oit_depth_offset` from the enumerated ordinal instead of
  `DrawOrdinal::from_draw_slot`.
- `render/panel_text/batching.rs` — replace the per-run `depth_nudge` from
  `draw_slot` (`:264`) and the per-run `oit_depth_offset` from
  `DrawOrdinal::from(draw_layer)` (`:255/265`) with the unified ordinal; rederive
  the coarse batch lane (`:749` `DrawOrdinal::from(DrawLayer(key.layer))`) from
  `DrawStep::Text` instead of `DEFAULT_DRAW_LAYER`. **Trap:**
  `PanelTextChild.draw_slot` is currently the *next geometry* slot (a +1 trick so
  the run sits above prior fills — comment at `:261–263`). In the new model text
  is above fills/lines by `DrawStep::Text`, so `PanelTextChild` must carry the
  **text command's own enumerated ordinal**, not the next geometry slot.
- `render/panel_lines/batching.rs` + `render/analytic_paths/batching.rs` —
  rederive the coarse `BATCH_PANEL_LINE_DEPTH_BIAS` lane (`~:614`) from
  `DrawStep::Lines`. **The per-record line offsets are NOT fine disambiguation —
  they are `draw_slot`-derived coarse offsets** (`PanelLinePaintOrder::Normal{
  draw_slot }` seeded at `positioning.rs:314` → `line.rs:526–532` derives
  `depth_bias = draw_slot × NORMAL_DEPTH_BIAS_STEP(1.0)` and
  `oit_depth_offset = (draw_slot+1) × NORMAL_OIT_DEPTH_STEP(−1e-6)`, applied at
  `:654–677`). Re-derive per-record line/part depth from the enumerated ordinal
  (or `tree_order` within the `Lines` step) — do not retain the `draw_slot`
  formula (the field is deleted in Phase 5).
- `render/constants.rs` — delete the `min(ordinal − 64, 0)` clamp in
  `DrawOrdinal::oit_depth_offset` (`:86–88`); the symmetric `text_anchor`-relative
  offset replaces it (D5). The anchor moves from `DEFAULT_DRAW_LAYER` to the
  panel's `text_anchor`, so the offset is computed by the per-panel projection,
  not the bare per-ordinal method. Remove the `#[cfg_attr(not(test),
  expect(dead_code, …))]` gate on `enumerate_ordinals` now that render calls it.
- **Retune `OIT_MIN_DEPTH` (honor the invariant).** Replace the hard-coded
  `OIT_MIN_DEPTH = 2e-7` with `3 × OIT_DEPTH_STEP` (`= 3e-6`, since
  `OIT_DEPTH_STEP = 1e-6`) in all three shaders (`sdf_panel.wgsl`,
  `analytic_path.wgsl`, `panel_line_batch.wgsl`) so the floor tracks calibration —
  set the literal `3e-6` with a comment naming the `3 × OIT_DEPTH_STEP`
  relationship (prefer `3.0 * OIT_DEPTH_STEP` symbolically only if a shader already
  defines/imports that constant; they currently hard-code the floor). **Only
  `analytic_path.wgsl` is hashed** by the `EXPECTED_SHADER_FNV1A` tripwire
  (`coverage_probe.rs ~:871`); after editing it, run the test, read the printed
  new hash, paste it into `EXPECTED_SHADER_FNV1A` **in this same commit**. The
  other two shaders are not hashed. (Numerically `2e-7` was adequate; this is for
  codebase consistency — the floor and the step now derive from one constant.)
- `render/panel_text/reconcile.rs` — re-key image-material rebuild on the
  ordinal/step instead of `draw_slot` (`:587–589`, material build `:642`); text
  reuse key `(PanelFieldId, line_index)` unchanged.
- **Re-key three reconcile-identity carriers off `draw_slot`** (the invariant
  named text/image but missed the SDF surface): `PanelSdfSurface.draw_slot` in the
  geometry-eq signature (`panel_geometry.rs:48/131/561`), `PanelTextChild.draw_slot`
  (`panel_text/layout.rs:26`), `PanelImageChild.draw_slot` in the
  `visuals_unchanged` reuse test (`reconcile.rs:427/494/588`). Re-key each on the
  enumerated ordinal (or drop it) this phase, or reconcile reuse/respawn breaks
  when Phase 5 deletes the field.
- `layout/element.rs` — `classify_element_change` must compare `draw_layer`
  (Phase 2 left it destructured `draw_layer: _`, inert). Once render reads
  `z_index`, a `.draw_layer()`-only authoring change must classify as a visual
  change so the command stream regenerates with the new ordinal — otherwise it
  takes the `Identical` skip (`panel/compute_layout.rs:96`) and the panel keeps
  stale depth.
- **Single z-index source.** Every command — fill, text, line — takes its level
  from its own element's `z_index` (the `Element.draw_layer` field, Phase 2),
  feeding `enumerate_ordinals` directly. Base order is declaration order
  (`tree_order`) + the fixed `DrawStep` ladder; `z_index` is the override. **No
  inheritance** — the old text-only `DrawLayer` cascade (a default-`64` layer
  propagated to label entities) is retired, not carried forward. So **delete the
  per-label cascade read** at `render/panel_text/batching.rs:237`
  (`cascades.draw_layer(label_entity)`); text level no longer comes from
  `Override`/`Resolved<DrawLayer>`. Do NOT delete the `glyph_cascade.rs`
  `DrawLayer` machinery or the `with_draw_layer` verb here — only stop reading the
  cascade for text level; the machinery deletion is Phase 5/6. Keep the existing
  Phase-2 `.draw_layer(...)` builder; do not add a second differently-named
  builder (the `.draw_zindex` ergonomic name is a Phase-6 rename concern).
- **Keep green.** If removing a read leaves a helper unused
  (`DrawOrdinal::from_draw_slot`, `From<DrawLayer>`, `DEFAULT_DRAW_LAYER`), gate it
  `#[cfg_attr(not(test), expect(dead_code, reason = "…"))]` — deletion is Phase 5/6.
  `draw_slot` stays a field through Phase 4 (Phase 5 deletes it); Phase 4 only
  stops reading it for depth.
- The overflow guard (`panel_geometry.rs:237–253`) is reworked in **Phase 5**, not
  here — leave it reading `draw_slot` (still compiles, field survives).

**Files:** `render/panel_geometry.rs`, `render/panel_text/batching.rs`,
`render/panel_text/reconcile.rs`, `render/panel_text/layout.rs`,
`render/panel_lines/batching.rs`, `render/analytic_paths/batching.rs`,
`render/constants.rs`, `layout/line.rs`, `layout/engine/positioning.rs`,
`layout/element.rs`, `shaders/sdf_panel.wgsl`,
`render/analytic_paths/analytic_path.wgsl`,
`render/panel_lines/panel_line_batch.wgsl`,
`text/slug/glyph/coverage_probe.rs`.

**Constraints from prior phases:** Phase 1 (`474382b`) added `DrawStep`+`ordinal()`
and `RenderCommandKind::draw_step()`. Phase 2 (`474382b`) added the
`Option<DrawLayer>` authoring field (`El`/`Element`, named `draw_layer`), emission
stamps `RenderCommand.z_index` from it, scissors stamp `z_index: None`; the
`draw_slot` counter still runs. Phase 3 (`857b9a0`) added `HierarchicalDrawKey`
(2-level `Ord` `(z_level, step.ordinal(), tree_order)`, `z_level =
z_index.map_or(0, |z| z.0)`) and `enumerate_ordinals(&[RenderCommand]) ->
Vec<Option<DrawOrdinal>>` (gated dead-code), with parity tests asserting the new
order reproduces the current `draw_slot`/`DrawLayer` order. The lane-collision
boundary is a **true no-op only for `draw_slot < 63`**: at `draw_slot == 63` a
`Fill` ties the `Lines` lane in the old model; the new key deterministically
orders `Fill` below `Lines` (the documented lane intent, pinned by
`level_zero_fill_stays_below_lines_at_lane_boundary`).

**Acceptance gate:** `cargo build -p bevy_diegetic` clean, `cargo +nightly fmt`,
`cargo nextest run -p bevy_diegetic` green (Phase 3 parity tests still pass; the
FNV tripwire passes with the refreshed hash), `cargo clippy -p bevy_diegetic
--all-targets` no new warnings. Add a **render-level equivalence acceptance test**:
for representative no-override panels, post-flip per-pass material values
(`panel_geometry` `depth_bias`/`oit_depth_offset`, text batch lane + per-run nudge,
line batch lane + per-record offset) match their pre-flip values — the unified
ordinal replaces three differently-scaled derivations (`LAYER_DEPTH_BIAS`,
`NORMAL_*_STEP`, text `draw_layer`), which the CPU ordinal test does not cover.
Behavior: an in-panel overlay quad with a positive `DrawZIndex` renders above text
on **both** the sorted screen view and the OIT world view; toggling only
`draw_layer` re-orders the element on screen.

#### Phase 4 implementation note (dual review)

Codex built one `DrawOrderProjection` per `ComputedDiegeticPanel`
(`Vec<Option<DrawCommandDepth>>`, index-aligned with commands) feeding geometry,
text, lines, and reconcile from one source; `DrawCommandDepth { ordinal,
depth_bias, oit_depth_offset }` derives `PartialEq`; `oit_depth_offset` is
`text_anchor`-relative with the clamp removed; `OIT_MIN_DEPTH = 3e-6` in all three
shaders with the FNV refreshed; `classify_element_change` treats `draw_layer` as
`VisualOnly`; the per-label `DrawLayer` cascade read is deleted (text level comes
only from the element `z_index`); `draw_slot`/`DrawLayer`/`DEFAULT_DRAW_LAYER` left
intact for Phases 5/6. Builds clean, 408 tests pass, clippy clean.

The OIT (world) depth path is correct. Two reviewers (blind codex + Claude)
independently returned REQUEST CHANGES on the non-OIT screen path and
reconcile-on-text-toggle. Blockers carried into Phase 4a:

| # | Severity | File | Problem |
| --- | --- | --- | --- |
| A | blocker | `panel_text/batching.rs:735` | Text batch uses the fixed `DrawStep::Text` bias (2.0) on the screen view, not a per-command/per-level ordinal. Screen panels are non-OIT + `depth_bias`-ordered, so with ≥3 fills a fill (ordinal ≥2) sorts above text, and a `z_index`-raised fill (ordinal 1) cannot rise above text (2.0). Screen z-index ordering broken. |
| B | blocker | `panel_geometry.rs:568` | SDF reuse signature stores `draw_ordinal` but not `oit_depth_offset`. `oit_depth_offset` depends on `text_anchor`; toggling text on/off shifts `text_anchor` while a quad's ordinal/geometry hold, so the quad is reused with a stale OIT offset. |
| C | minor | `line.rs:526/598` | `PanelLineLayering` still derived from `draw_slot`, now a dead write (renderer uses `source.draw_depth`). Gate or remove; Phase 5 deletes it. |
| D | blocker | `constants.rs:691`, `batching.rs:1215` | The spec-required render-level equivalence acceptance test is missing — codex rewrote the old value-match tests to assert the new values instead. This is the gate that would have caught A and B. |

### Phase 4a — Text-batch z-index ordering on the screen view + reconcile/test fixes · status: todo

#### Work Order

**Goal:** Batched text orders correctly against per-command fill/line ordinals on
the non-OIT screen view — a `z_index` raise/lower on text or on a fill works on
screen, not only on the OIT world view — while default text stays a single shared
batch across all panels. Reconcile no longer reuses an SDF quad with a stale OIT
offset across a text toggle. The render-level equivalence and screen-ordering
acceptance tests exist.

**Spec:**

**Blocker A — text-batch screen ordering (the model).** Ordering is *level-major*:
sort first by z-level (the `i8` `z_index`, default `0`), then by the fixed
`DrawStep` ladder (`Fill < Lines < Text`) within a level. Fills and lines are
individual / per-command — each fill is its own SDF draw carrying its own
ordinal-derived `depth_bias`. Text is *batched* (vertex-pulled): a batch is one
draw with one `depth_bias` on the sorted screen view, so it cannot carry a
per-command ordinal the way a fill can.

That is acceptable because text needs no per-command ordering within a level —
within a level text always sits above that level's fills and lines (the ladder),
and same-level text runs do not overlap. So text needs exactly ONE depth number
per z-level.

Mechanism: **batch text per distinct z-level** — add `z_level` to the text
`BatchKey` (`panel_text/batching.rs`). Each level's text batch gets a screen
`depth_bias` on the SAME ordinal/`LAYER_DEPTH_BIAS` scale as fills, placed above
every same-level fill and line and below the next level up. Reserve a fixed
per-level band: level `L` occupies the `depth_bias` window
`[L × LEVEL_STRIDE, (L+1) × LEVEL_STRIDE)`; same-level fills/lines occupy the lower
part of the band by their per-command ordinal; the level's text batch takes a
reserved text sub-lane at the top of the band. `LEVEL_STRIDE` must be ≥ the
per-panel ordinal bound the overflow guard enforces, so a panel's fills never
reach the text sub-lane. This is the retired `DEFAULT_DRAW_LAYER = 64` text lane
generalized to one band per z-level — a NEW construct in the projection, not the
old global `64`.

Result:
- All default-level (`0`) text → ONE batch at one number → 1 draw regardless of
  panel count or nesting depth (the `diegetic_text_stress` 1-batch invariant holds).
- Text moved to a distinct level → its own batch, SHARED across all panels at that
  level (one batch per distinct level, never per panel).
- A `z=+1` background fill (individual draw, level-`+1` band) sits entirely above
  level-`0` text; a `z=−1` text run gets its own batch in the level-`−1` band,
  below level-`0` fills.

The OIT (world) path is already correct from Phase 4 and does NOT change: OIT
sorts per fragment by the per-record `oit_depth_offset`, so a single text batch
already orders per-command on the world view (D5 holds on OIT). Phase 4a changes
only the screen `depth_bias` derivation for the text batch and the `z_level` batch
split. Verify fills' screen `depth_bias` lands on the same level-banded scale
(the dense ordinal already sorts level-major because `z_level` is the high-order
sort term) so a higher-level fill outsorts lower-level text.

**Blocker B — stale SDF OIT offset across a text toggle.** Reuse is decided by
`signature == quad.signature` (`panel_geometry.rs:280`). The signature stores
`draw_ordinal` but not `oit_depth_offset`, which depends on `text_anchor` (the
lowest `Text`-step ordinal). Toggling text on/off shifts `text_anchor`, changing a
quad's `oit_depth_offset` while its `draw_ordinal`/geometry hold — so the quad is
judged `Identical` and reused with a stale offset. Fix: store the full
`DrawCommandDepth` (derives `PartialEq`) in the signature (`panel_geometry.rs:568`)
so an offset shift invalidates reuse.

**Blocker C — dead `PanelLineLayering` write.** `PanelLineLayering` is still
derived from `draw_slot` (`line.rs:526/598`) and stored on `ResolvedPanelLine`,
but the renderer no longer reads it for depth (lines use `source.draw_depth`). It
is a dead write. Gate it `#[cfg_attr(not(test), expect(dead_code, …))]` here; full
removal is Phase 5.

**Blocker D — missing acceptance tests.** Phase 4's gate required a render-level
equivalence test; codex instead rewrote the old value-match tests to assert the
new values, which proves nothing about order-equivalence and would not catch A/B.
Add three tests:
1. **Render-level equivalence** — for representative no-override panels, post-flip
   per-pass material values (`panel_geometry` `depth_bias`/`oit_depth_offset`, text
   batch lane + per-run nudge, line batch lane + per-record offset) match their
   pre-flip values.
2. **Screen ordering** — on the `depth_bias` (screen) axis: with ≥3 fills, text
   sorts above all default fills; a `z=+1` fill sorts above default text; a `z=−1`
   text run sorts below default fills. This is the test that catches Blocker A.
3. **Reconcile** — toggling text on/off changes each SDF quad's stored
   `oit_depth_offset` (catches Blocker B).

**Files:** `render/panel_text/batching.rs`, `render/panel_geometry.rs`,
`render/constants.rs` (per-level band / text sub-lane construct + tests),
`layout/line.rs`.

**Constraints from prior phases:** Phase 4 (implemented, uncommitted) built one
`DrawOrderProjection` per `ComputedDiegeticPanel` (`Vec<Option<DrawCommandDepth>>`,
index-aligned with commands) feeding geometry/text/lines/reconcile from one source;
`DrawCommandDepth { ordinal, depth_bias, oit_depth_offset }` derives `PartialEq`;
`oit_depth_offset` is `text_anchor`-relative with the clamp removed; `OIT_MIN_DEPTH
= 3e-6` in all three shaders + FNV refreshed; `classify_element_change` treats
`draw_layer` as `VisualOnly`; the per-label `DrawLayer` cascade read
(`batching.rs:237`) is already deleted — text level comes only from the element
`z_index`; `draw_slot`/`DrawLayer`/`DEFAULT_DRAW_LAYER` are intact (Phase 5/6).
The OIT/world depth path is correct; only the non-OIT screen text-batch path and
the reuse signature need 4a fixes.

**Acceptance gate:** `cargo build -p bevy_diegetic` clean, `cargo +nightly fmt`,
`cargo nextest run -p bevy_diegetic` green including the three new tests, `cargo
clippy -p bevy_diegetic --all-targets` no new warnings. Behavior: on a screen
(non-OIT) panel a `z=+1` element renders above text and a `z=−1` text run renders
below fills (the screen-ordering test); default text across N panels remains a
single batch.

### Phase 5 — Delete the dead mechanism + rework the overflow check · status: todo

#### Work Order

**Goal:** Remove the now-unreachable old draw-order axis; keep the OIT-budget
guard, re-pointed at the distinct-coplanar-ordinal count.

**Spec:**

- Delete `RenderCommandKind::consumes_draw_slot()` (`render.rs:132–145`),
  `RenderCommand::draw_slot` (`render.rs:~32`), `EmissionCounters.draw_slot`
  (`positioning.rs:35–60`), and `DEFAULT_DRAW_LAYER` (`cascade/constants.rs:20`).
- **Full `draw_slot`-reader inventory to delete/rework:** the counter is also read
  at `positioning.rs:314` to seed `PanelLinePaintOrder::Normal { draw_slot }` —
  delete that variant field plus `NORMAL_DEPTH_BIAS_STEP`/`NORMAL_OIT_DEPTH_STEP`
  and the `line.rs:526–532` derivation (their ordering moved to the Phase-4
  ordinal). Drop the per-carrier `draw_slot` fields once Phase 4 re-keyed reconcile:
  `PanelSdfSurface.draw_slot`, `PanelTextChild.draw_slot`,
  `PanelImageChild.draw_slot`. Deleting `RenderCommand::draw_slot` without these
  will not compile.
- `render/panel_geometry.rs` — rework the overflow check (`:237–253`): it stays,
  but warns when a panel's *distinct coplanar ordinal count* approaches the OIT
  budget (`≈ focus-depth / OIT_DEPTH_STEP`), not when `draw_slot ≥ 64`. Source the
  count from `enumerate_ordinals(...).iter().flatten().count()` (spans fills, lines,
  AND text), not a reconstructed `draw_slot` max (which counts only slot-consuming
  kinds). Past the budget, ordering degrades to best-effort OIT insertion order
  (same far-panel degradation the current model has) — no silent truncation.
- Restate "lines just under text" as `DrawStep::Lines < DrawStep::Text`, not
  `63 < DEFAULT_DRAW_LAYER`, wherever a comment references the deleted constant.

**Files:** `layout/render.rs`, `layout/engine/positioning.rs`, `layout/line.rs`,
`cascade/constants.rs`, `render/panel_geometry.rs`, `render/panel_text/layout.rs`,
`render/panel_text/reconcile.rs` (carrier fields).

**Constraints from prior phases:** Phase 4 re-keyed every depth read and every
reconcile carrier onto the enumerated ordinal, so `draw_slot` and
`DEFAULT_DRAW_LAYER` have no remaining readers except the overflow guard. The
`with_draw_layer` verb + `glyph_cascade.rs` `Override`/`Resolved<DrawLayer>`
resolution + propagation/reconcile arms are old-model machinery — delete here or
defer to Phase 6 with the rename (the cascade read was already removed in Phase 4).

**Acceptance gate:** `cargo build` clean with `draw_slot`/`DEFAULT_DRAW_LAYER`
gone; `cargo nextest run` green; overflow warning fires only near the OIT budget.

### Phase 6 — Flag-day rename + example + test/doc cleanup · status: todo

#### Work Order

**Goal:** Rename `DrawLayer → DrawZIndex` (editor-driven, user-approved), ship the
in-panel-overlay example, and finish test/doc cleanup.

**Spec:**

- **Confirm the rename with the user first** (rename-through-editor convention).
  Re-derive the edit scope at execution — citations across this plan have
  drifted; as of the last review `DrawLayer` + `draw_layer` was ~172 refs across
  17 files, `DEFAULT_DRAW_LAYER` adds ~24, `glyph_cascade.rs` carries ~40.
  `cascade_attr!` regenerates the verbs, `Reflect`, and the BRP type path
  automatically — no hand-written reflection sites.
- Rename via the editor: `DrawLayer → DrawZIndex`, `draw_layer → draw_zindex` —
  the `El`/`Element` field, `TextStyle` field (`text_props.rs:218`) + builder
  (`:528`) + setter (`:610`), cascade declaration (`cascade/resolved.rs:82–93`),
  verbs (`cascade/attributes.rs:51/95/159`), and readers (`reconcile.rs`,
  `glyph_cascade.rs`, `panel_text/batching.rs`, `constants.rs`). Re-confirm every
  cited line number at rename time.
- Rewrite the example `examples/text_draw_layer.rs` → e.g. `panel_draw_order.rs`:
  one panel, a text child and a sibling overlay quad in the same tree, ordered with
  `DrawZIndex` and a hotkey toggle — not the current second-anchored-panel fake.
- Rewrite `sorted_and_oit_orderings_agree_for_every_layer_pair`
  (`constants.rs:~557`) over `(HierarchicalDrawKey, HierarchicalDrawKey)` pairs:
  two unset commands at different steps; unset vs `z = 0` same step; unset vs set
  across steps; raised pairs (D5 symmetric offset).
- `coverage_probe.rs` `EXPECTED_SHADER_FNV1A` was already refreshed in Phase 4
  (the `OIT_MIN_DEPTH` retune). The rename does not touch shader text
  (`draw_layer` is passed as a precomputed offset through the existing shader
  input), so **no further FNV refresh here**.
- Delete `as-built/text-draw-layer.md` once the old mechanism is gone.

**Files:** `examples/text_draw_layer.rs` (→ renamed), `render/constants.rs`
(test), `layout/text_props.rs`, `cascade/resolved.rs`, `cascade/attributes.rs`,
plus every renamed reference (editor-driven), and `docs/bevy_diegetic/as-built/text-draw-layer.md` (delete).

**Constraints from prior phases:** Phase 4 already refreshed `EXPECTED_SHADER_FNV1A`.
Phase 5 already deleted `DEFAULT_DRAW_LAYER` and the `draw_slot` machinery; if the
`glyph_cascade.rs`/`with_draw_layer` machinery survived Phase 5, delete it as part
of the rename here. The new model is fully wired into rendering — the rename is cosmetic
(type/field names) and must not change ordering behavior.

**Acceptance gate:** compiles under the new names; example demonstrates in-panel
overlay; parity test green over the new key; as-built doc removed; `cargo nextest
run` green.

### Phase 7 — Design: universal element batching (fills join the batched path) · status: todo (design only)

#### Work Order

**Goal:** Produce a *design* (a new design doc under `docs/bevy_diegetic/`, not
implementation) for converting individual per-fill SDF draws into a batched
vertex-pulled path, so a UI with many elements (sliders, buttons, borders,
handles) across many panels does not emit one draw call per fill. The draw-order
ordinal projection is the ordering input. Motivation: `bevy_diegetic` is intended
to become a full UI crate — element counts per panel and panel counts are both
high, so the current "one draw per fill" cost is the scaling bottleneck (text and
lines already batch; fills do not).

**Spec (what the design must resolve):**
- Today each fill is its own SDF quad entity (`spawn_sdf_quad`, `panel_geometry.rs`)
  + its own `SdfPanelMaterial` carrying its own `depth_bias`/`oit_depth_offset`.
  Text and lines are already batched (vertex-pulled from a storage buffer). The
  design unifies fills onto that batched path.
- Per-quad material variety (size, color, corner radii, depth) moves into a
  per-quad storage buffer indexed by vertex/instance, like text/lines.
- Ordering input is the existing `DrawOrderProjection` ordinal:
  - **World (OIT) panels:** carry `oit_depth_offset` per quad; submission order is
    irrelevant (per-fragment sort) — batch freely.
  - **Screen (non-OIT) panels:** a batch is one draw and blends in buffer order, so
    CPU-sort fill records by ordinal per view, and place each batch on the
    level-banded `depth_bias` scale from Phase 4a so batches interleave correctly
    across levels. Cross-panel ordering for *overlapping* screen panels (a single
    global text/fill band cannot carry per-panel distance separation) is a known
    constraint the design must address.
- Buffer churn: a fill change rebuilds the buffer. Honor the ShaderBuffer rebind
  hazard — `set_data` with a changed byte length re-creates the wgpu buffer and
  material bind groups do not follow; pad to fixed capacity and swap in new buffer
  assets + rewrite material handles on growth.
- Reconcile: per-quad identity keyed on `element_idx`/ordinal (consistent with the
  Phase-4 reconcile carriers), so a z-index move re-keys the buffer record, never
  respawns the entity.
- Decide batch granularity: one element batch per `(view, z-level, material class)`
  vs a single per-panel mega-buffer — weigh draw-call count against buffer-rebuild
  cost.

**Files:** new design doc (e.g. `docs/bevy_diegetic/element-batching.md`); reads
`render/panel_geometry.rs`, `render/panel_text/batching.rs`, `render/constants.rs`.

**Constraints from prior phases:** Phases 4/4a established the ordinal projection
and the per-level screen `depth_bias` banding that any batched-fill path reuses.
Phase 5 deleted `draw_slot`; the projection ordinal is the sole ordering source.

**Acceptance gate:** a written, reviewed design doc covering the buffer layout, the
per-view ordinal sort, the OIT-vs-screen ordering split, the buffer-rebind/padding
strategy, and reconcile identity — approved by the user before any implementation
phase is scheduled. No code.

---

## Archive — completed phases

<!-- Done phases: the record of what was dispatched. Skipped at dispatch time. -->

### Phase 1 — `DrawStep`, inert · status: done (`474382b`)

#### Work Order

*Commit:* add the step enum and per-kind mapping beside the existing bool; no
reads, no behavior change.

- `layout/render.rs` — add `DrawStep { Fill, Lines, Text }` + `ordinal()`; add
  `RenderCommandKind::draw_step(&self) -> Option<DrawStep>` (`:50–87` for the
  enum, beside `consumes_draw_slot()` at `:94–102`). Keep `consumes_draw_slot()`.
- Unit test: every `RenderCommandKind` variant maps to the expected step (and
  scissors to `None`).

*Gate:* compiles; nothing reads `draw_step()` yet; existing tests unchanged.

### Phase 2 — `Option<DrawLayer>` on `El`/`Element` + emission stamps `z_index`, inert · status: done (`474382b`)

#### Work Order

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

### Phase 3 — `HierarchicalDrawKey` + projection, computed in parallel and validated · status: done (`857b9a0`)

#### Work Order

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

**Built types (record):**

```rust
enum DrawStep { Fill, Lines, Text }   // ordinal() = 0, 1, 2 via explicit match
// RenderCommandKind::draw_step(&self) -> Option<DrawStep>
//   Rectangle | Border | Image => Some(Fill); Lines => Some(Lines);
//   Text => Some(Text); ScissorStart | ScissorEnd => None

struct DrawZIndex(i8);                 // i8 avoids bevy::prelude::ZIndex(i32) clash
// authored as Option<DrawZIndex>: None = implicit zero level, never a 0 sentinel

struct HierarchicalDrawKey {
    z_index:    Option<DrawZIndex>,    // None = auto, treated as level 0
    step:       DrawStep,
    tree_order: u32,                   // command index in the RenderCommand stream
}
// Ord: lexicographic (z_level, step.ordinal(), tree_order), z_level = z_index.unwrap_or(0)
```

The 2-level key (not the single-axis `z_index.unwrap_or(step.ordinal())`) is
required: collapsing z-level and step makes a set `z = 2` tie with unset `Text`
instead of sitting above it. With the 2-level key: unset `Text` `(0,Text)` beats
`z=0` `Fill` `(0,Fill)`; `z=2` `Fill` `(2,Fill)` beats unset `Text` `(0,Text)`
(D5); `z=−1` `Text` `(−1,Text)` sinks below unset `Fill` `(0,Fill)`. `tree_order`
is the layout-DFS stream index (`positioning.rs`), the only "later-wins"
definition stable through batching (batched glyph/line records concatenate in
archetype order, not tree order, so order must land in `depth_bias` /
`oit_depth_offset`, never in submission order).

*Gate:* compiles; new ordinal computed and asserted against the old; render
still reads `draw_slot`. `enumerate_ordinals` is
`#[cfg_attr(not(test), expect(dead_code, …))]` until Phase 4 reads it; returns
`Vec<Option<DrawOrdinal>>` (index-aligned, `None` for scissors), not the
sketched `Vec<DrawOrdinal>`. `DrawStep` was private to `layout`; a `pub(crate)
use render::DrawStep` re-export was added in `layout/mod.rs`.

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
- Phase 4's "existing panels render unchanged" gate holds for `draw_slot < 63`
  (the lane boundary is an intended, tested resolution).

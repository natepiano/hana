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

### Phase 4a — z-index on the screen view for every element type + authoring + tests · status: done (uncommitted)

#### Work Order

**Goal:** The signed z-index (`DrawZIndex`, currently named `DrawLayer`) works
uniformly on the non-OIT screen view for **every** element type — fills, borders,
images (individual SDF/mesh draws), dividers/lines and text (batched). Raising or
lowering any element with `±z` reorders it on screen, not only on the OIT world
view; default content stays in single shared batches. Reconcile never reuses an
SDF quad with a stale OIT offset across a text toggle. Real render-level
equivalence and screen-ordering tests exist.

**Spec:**

**The model (level-major).** Sort first by z-level (the signed `i8`, default `0`),
then by the fixed `DrawStep` ladder (`Fill < Lines < Text`) within a level.
- **Fills, borders, images** are *individual* draws — each carries its own
  level-banded `depth_bias`, so z-index already moves them on screen. No change;
  the tests must confirm it.
- **Lines and text** are *batched* (vertex-pulled). A batch is one draw with one
  `depth_bias` on the sorted screen view, so it cannot carry a per-command ordinal.
  Within a level a batch needs no per-command ordering: text always sits above the
  level's fills/lines (the ladder), lines above the level's fills, and runs at one
  level do not need to interleave. So each batched type needs exactly ONE screen
  number per z-level.

**Per-level band layout.** Level `L` occupies the `depth_bias` window
`[L × DRAW_LEVEL_STRIDE, (L+1) × DRAW_LEVEL_STRIDE)` on the `LAYER_DEPTH_BIAS`
scale. Within a level: fills/borders/images take the low sub-lanes by per-command
ordinal; the **line batch** takes a reserved sub-lane near the top
(`DRAW_LEVEL_GEOMETRY_LANES − 1`, i.e. `63` — restores the pre-flip line lane); the
**text batch** takes the sub-lane just above it (`DRAW_LEVEL_TEXT_SUBLANE` = `64`).
`DRAW_LEVEL_STRIDE` ≥ the per-panel ordinal bound the overflow guard enforces, so a
panel's fills never reach the line/text sub-lanes. This generalizes the retired
`DEFAULT_DRAW_LAYER = 64` text lane to one band per z-level — a NEW construct in
the projection, not the old global `64`.

**Blocker A — batched lines + text honor z-index on screen.**
- Text: batch per distinct z-level (`z_level` on the text `BatchKey`); each level's
  text batch material `depth_bias = text_batch_depth_bias(z_level)` (sub-lane `64`).
  *(Implemented in the first 4a pass.)*
- Lines: **same treatment** — add `z_level` to the line batch key
  (`panel_lines/batching.rs`, `LineBatchKey`/`VisualBatchKey`) and set the line
  batch material `depth_bias` from a per-level line sub-lane
  (`line_batch_depth_bias(z_level)` = `level_sublane_depth_bias(z_level, 63)`),
  replacing the fixed `BATCH_PANEL_LINE_DEPTH_BIAS` lane. **Defect this fixes:** the
  Phase-4 change set `BATCH_PANEL_LINE_DEPTH_BIAS` to `LAYER_DEPTH_BIAS` (lane `1`,
  down from the pre-flip `63`), so on the screen view any panel with ≥2 level-0
  fills paints a fill over its dividers, and a z-raised line never rises. The
  per-level line sub-lane (`63` at level 0) restores correct order and makes
  z-raised lines rise per level, mirroring text.

Result: all default-level (`0`) text → ONE batch (the `diegetic_text_stress`
1-batch invariant holds); default-level lines → ONE batch at lane `63`; a `z=+1`
element of any type sits above level-`0` text; a `z=−1` text/line run gets its own
batch in the level-`−1` band, below level-`0` fills. The OIT (world) path is
unchanged from Phase 4 (per-fragment sort by the per-record `oit_depth_offset`).

**Blocker B — stale SDF OIT offset across a text toggle.** *(Implemented in the
first 4a pass.)* The SDF reuse signature now stores the full `DrawCommandDepth`
(`panel_geometry.rs:568`), so a `text_anchor` shift (text toggled on/off)
invalidates reuse instead of keeping a stale `oit_depth_offset`.

**Blocker E — text z-index authoring (the missing trigger).** The z-level batching
is mechanically correct but unreachable: text leaves are built with
`..Element::default()` (`builder.rs:494`), so the text command's `z_index` is
always `None`, and the only text-facing API (`TextStyle::with_draw_layer`) feeds
the retired old-model cascade. Wire text to the **same signed `Element.draw_layer`
field every other element uses** (the field Phase 2 added, renamed to `DrawZIndex`
in Phase 6; `None` = level 0, positive = forward, negative = back). Expose it on
the text-builder path so a text leaf can set `Element.draw_layer`; `push_command`
already stamps `RenderCommand.z_index` from the element field, so no positioning
change is needed once the leaf carries it. **Do NOT reconnect `TextStyle`'s
`draw_layer`/cascade for ordering** — that is the retired absolute-layer path. One
signed z-index field, uniform across fills, borders, images, lines, and text.

**Blocker C — dead `PanelLineLayering` write → deferred to Phase 5.** Left
unchanged in 4a (an `expect(dead_code)` gate is unfulfilled because the type is
public API). Phase 5 deletes the struct and its derivation.

**Blocker D — real acceptance tests.** The first 4a test checked projection math,
not the values handed to the GPU, and asserted the regressed line lane (`1`) as
correct. Replace with:
1. **Render-level equivalence** — for a representative no-override panel, the
   actual spawned material values match the **pre-flip shipped model**:
   `panel_geometry` `depth_bias`/`oit_depth_offset`, the text batch lane (`64`) +
   per-run nudge, and the line batch lane (`63`) + per-record offset. Assert against
   the pre-flip constants (line lane `63`, text lane `64`), not the new helpers, so
   the test would fail on a regression like Blocker A.
2. **Screen ordering** — on the `depth_bias` axis, for fills, lines, AND text: with
   ≥3 fills, lines and text sort above all default fills; a `z=+1` element of each
   type sorts above default text; a `z=−1` text/line run sorts below default fills.
3. **Authoring** — a text leaf authored with `±z` produces a non-zero
   `PanelTextZLevel` and lands in the matching level batch (catches Blocker E).
4. **Reconcile** — toggling text on/off changes each SDF quad's stored
   `oit_depth_offset` (catches Blocker B). *(Implemented.)*

**Module placement (follow-on to the extraction).** Move the material helpers that
remained in `render/constants.rs` (`apply_glyph_sidedness`, `default_panel_material`,
`resolve_material`) into a new `render/material.rs` — they are helpers, not
constants. `constants.rs` keeps only literal constants; wire `mod material;` into
`render/mod.rs` and update importers.

**Files:** `render/panel_text/batching.rs`, `render/panel_lines/batching.rs`,
`render/analytic_paths/batching.rs`, `render/draw_order.rs` (line sub-lane fn +
tests), `render/constants.rs` (band constants; material helpers leave),
`render/material.rs` (new), `render/mod.rs`, `layout/builder.rs` (text z authoring),
`layout/engine/positioning.rs` (verify text leaf z stamps), `render/panel_geometry.rs`.

**Constraints from prior phases:** Phase 4 (committed) built one
`DrawOrderProjection` per `ComputedDiegeticPanel` feeding geometry/text/lines/
reconcile from one source; `DrawCommandDepth { ordinal, z_level, depth_bias,
oit_depth_offset }` derives `PartialEq`; `oit_depth_offset` is `text_anchor`-relative
(no clamp); `OIT_MIN_DEPTH = 3e-6` in three shaders; the per-label `DrawLayer`
cascade read is deleted. The first 4a pass added the text `z_level` batch split,
`text_batch_depth_bias`, the per-level band constants, the full-`DrawCommandDepth`
SDF signature (Blocker B), and moved the projection engine to `render/draw_order.rs`.
`draw_slot`/`DrawLayer`/`DEFAULT_DRAW_LAYER` remain (Phase 5/6).

**Acceptance gate:** `cargo build -p bevy_diegetic` clean, `cargo +nightly fmt`,
`cargo nextest run -p bevy_diegetic` green including the new tests, `cargo clippy
-p bevy_diegetic --all-targets` no new warnings. Behavior on a screen (non-OIT)
panel: a `z=+1` element of any type renders above text; a `z=−1` text run renders
below fills; dividers render above same-level fills regardless of fill count;
default text and default lines each stay a single batch across N panels.

#### Retrospective

**What worked:**
- The text per-level-batch model generalized cleanly to lines: `line_batch_depth_bias(z_level)` at sub-lane 63 + `z_level` on `LineBatchKey`, mirroring `text_batch_depth_bias` at 64. Both batched types now honor z-index on the screen view with one batch per distinct level shared across panels.
- Text z-index authoring landed on the single signed `Element.draw_layer` field every other element uses (new `text_element`/`text_id_element` builders feed `El` to the text leaf); no positioning change needed since `push_command` already stamps `z_index` from the element field.
- 416 tests pass; the new tests assert spawned material `depth_bias` per z-level (not projection math), so a fixed-lane regression now fails.

**What deviated from the plan:**
- The original Phase-4 review framed Blocker A as text-only. Implementation found the *same* batched-screen-lane defect in lines (Phase 4 had dropped `BATCH_PANEL_LINE_DEPTH_BIAS` from 63 to 1), so 4a's scope widened to "every element type" — fills/borders/images were already correct as individual draws.
- `BATCH_PANEL_LINE_DEPTH_BIAS` was deleted, not gated dead-code, per the style guide's prefer-deletion rule.
- Module structure: the draw-order engine was extracted to `render/draw_order.rs` and material helpers (`apply_glyph_sidedness`, `default_panel_material`, `resolve_material`) to `render/material.rs`; `constants.rs` is now constants-only. (Not in the original Work Order — surfaced during review.)
- Two codex fix passes were needed: pass 1 (line lane + text authoring + tests + material module), pass 2 (raised/lowered text material-lane test + pinning the equivalence test to literal pre-flip lanes 63/64 instead of tautological helper-vs-itself assertions).

**Surprises:**
- `DrawCommandDepth` now carries `z_level`; `level_sublane_depth_bias(z_level, ordinal)` maps level `L` into the window `[L × DRAW_LEVEL_STRIDE, (L+1) × DRAW_LEVEL_STRIDE)` with `DRAW_LEVEL_STRIDE = 65`, `DRAW_LEVEL_GEOMETRY_LANES = DRAW_LEVEL_TEXT_SUBLANE = 64`. This is a real per-level screen cap (≤64 geometry items per level before spilling into the line/text sub-lanes) — a *new* ceiling Phase 5's guard rework must track, alongside the OIT budget.
- `TextStyle::with_draw_layer` is deliberately left disconnected from ordering (a test pins that it does not split batches); the old per-label cascade is dead for ordering. Phase 5/6 still delete the machinery.

**Implications for remaining phases:**
- Phase 5: the overflow guard must warn on the smaller of (a) per-level band capacity `DRAW_LEVEL_GEOMETRY_LANES` and (b) the OIT budget (already folded into the Phase 5 Work Order). Also delete the `PanelLineLayering` struct outright (4a left it as a dead write — `expect(dead_code)` was unfulfilled on a public type).
- Phase 6: the rename blast radius now includes `render/draw_order.rs` and `render/material.rs` (new files), the `text_element`/`text_id_element` builders, `PanelTextZLevel`, `line_batch_depth_bias`/`text_batch_depth_bias`, and the `DRAW_LEVEL_*` constants. The example should author raised/lowered text via `text_element(El::new().draw_layer(...))`, the now-working path.
- Phase 7 (fill batching design): the per-level screen sub-lane scheme (geometry low, lines at 63, text at 64) and `line_batch_depth_bias`/`text_batch_depth_bias` are the precedent a batched-fill path reuses; lines + text are now the two worked examples of "one batch per (z_level, …), CPU-fixed screen lane + per-record OIT offset."

### Phase 4a Review

Architect re-review of Phases 5/6/7. Applied automatically (minor):
- **Phase 5:** corrected `consumes_draw_slot` ref (`render.rs:130–132`); restated the overflow guard's current `draw_slot`/`DEFAULT_DRAW_LAYER` structure so a fresh codex doesn't reintroduce `draw_slot`; fixed the per-level count — `enumerate_ordinals` returns flat panel-global ranks, so the guard must group by `DrawCommandDepth.z_level()` (no per-level count API exists; add one); added the dead `BatchKey.layer` field/write deletions (`analytic_paths/batching.rs:60/432`, `panel_text/batching.rs:295`); fixed the acceptance gate to the smaller of band-capacity and OIT-budget ceilings.
- **Phase 6:** widened the rename surface to the post-extraction files (`render/draw_order.rs`, `render/material.rs`, `text_element`/`text_id_element`, `PanelTextZLevel`, `line_batch_depth_bias`/`text_batch_depth_bias`, `DRAW_LEVEL_*`); corrected drifted line refs (`with_draw_layer:534`, `set_draw_layer:611`, `resolved.rs:90`, `attributes.rs:52/95/159`); re-pointed the parity test to `draw_order.rs:~552` and reconciled it with the tests Phase 4a already added; added "verify `EXPECTED_SHADER_FNV1A` before relying on no-refresh."
- **Phase 7:** added the per-level 64-lane screen-band ceiling (`DRAW_LEVEL_GEOMETRY_LANES`) as a hard design constraint the batched-fill design must resolve, alongside the OIT budget.

User decisions:
- **Line-layering deletion is a public-API removal (approved full removal):** Phase 5 now enumerates the `pub` `PanelLineLayering` + `PanelLinePaintOrder`, their 4 re-exports, the `ResolvedPanelLineCommand.layering` field/accessor + `PanelLinePaintOrder::layering()` method, and the now-vestigial `PanelLinePaintOrder` enum collapse (+ `positioning.rs:313` seed and `integration_tests.rs:296/346` assertions).
- **Cascade teardown owner (approved Phase 6):** the entire `DrawLayer`-cascade machinery + the `DEFAULT_DRAW_LAYER` constant are deleted in Phase 6 with the rename, not Phase 5. Phase 5 keeps `DEFAULT_DRAW_LAYER` (the cascade default) alive and only removes its non-cascade readers; both Work Orders updated to remove the prior "delete here or defer" ambiguity.

### Phase 5 — Delete the dead mechanism + rework the overflow check · status: done (uncommitted in tree)

#### Work Order

**Goal:** Remove the now-unreachable old draw-order axis; keep the OIT-budget
guard, re-pointed at the distinct-coplanar-ordinal count.

**Spec:**

- Delete `RenderCommandKind::consumes_draw_slot()` (`render.rs:130–132`),
  `RenderCommand::draw_slot` (`render.rs:~32`), and `EmissionCounters.draw_slot`
  (`positioning.rs:35–60`). **`DEFAULT_DRAW_LAYER` is NOT deleted here** — it is the
  `DrawLayer` cascade's default and is torn down with the cascade machinery in
  Phase 6 (decision below). Phase 5 only removes `DEFAULT_DRAW_LAYER`'s *non-cascade*
  readers (the dead `BatchKey.layer` writes and the overflow-guard comparison), so
  the constant compiles cleanly until Phase 6.
- **Dead `layer` field on the batch keys (Phase 4a left these as pure writes).**
  Ordering now flows through `z_level`, so `BatchKey.layer` (set to
  `DEFAULT_DRAW_LAYER`) is dead: `analytic_paths/batching.rs:60` declares
  `layer: i8` and `:432` initializes it; `panel_text/batching.rs:295` writes it.
  The analytic-paths `BatchKey` is locally owned — delete its `layer` field +
  initializer (deleting `DEFAULT_DRAW_LAYER` will not compile until this is gone).
  The text-side `BatchKey` may be an external (Slug) struct; if so, drop only the
  `DEFAULT_DRAW_LAYER` write, leaving the field if it is not locally removable.
- **Full `draw_slot`-reader inventory to delete/rework:** the counter is also read
  at `positioning.rs:314` to seed `PanelLinePaintOrder::Normal { draw_slot }` —
  delete that variant field plus `NORMAL_DEPTH_BIAS_STEP`/`NORMAL_OIT_DEPTH_STEP`
  and the `line.rs:526–532` derivation (their ordering moved to the Phase-4
  ordinal). **Full line-layering public-API removal** (Phase 4a left it as a dead
  store — `expect(dead_code)` was unfulfilled because the types are `pub`; nothing
  reads `.layering()` for ordering, the renderer uses `source.draw_depth`). Remove,
  as a semver-visible change (crate is 0.x): the `pub` types `PanelLineLayering`
  *and* `PanelLinePaintOrder`; their 4 re-exports (`lib.rs:180–181`,
  `layout/mod.rs:73–74`); the `layering` field on `ResolvedPanelLineCommand`
  (`line.rs:171`), its `.layering()` accessor (`line.rs:458`), and the
  `PanelLinePaintOrder::layering()` method (`line.rs:524–532`); the computation at
  `line.rs:598`. After `draw_slot` leaves `PanelLinePaintOrder::Normal { draw_slot }`
  (the enum's only variant) it is a vestigial single fieldless variant — collapse or
  remove the enum and update its seed site (`positioning.rs:313`) and the
  `integration_tests.rs:296/346` assertions. Drop the per-carrier `draw_slot` fields once
  Phase 4 re-keyed reconcile: `PanelSdfSurface.draw_slot`, `PanelTextChild.draw_slot`,
  `PanelImageChild.draw_slot`. Deleting `RenderCommand::draw_slot` without these
  will not compile.
- `render/panel_geometry.rs` — rework the overflow check (`:237–253`). The guard
  currently reads `cmd.draw_slot` via `consumes_draw_slot()` and compares against
  `DrawOrdinal::from(DrawLayer(DEFAULT_DRAW_LAYER))`; do not reintroduce `draw_slot`.
  It must warn on the **smaller of two ceilings**, since Phase 4a's screen banding
  added a per-level cap: (1) the per-level band capacity — the count of draw
  commands **at a single z-level** must stay below `DRAW_LEVEL_GEOMETRY_LANES`
  (`= 64`) so geometry never reaches the line sub-lane (`63`)/text sub-lane (`64`)
  or spills into the next z-level's band; and (2) the OIT budget
  (`≈ focus-depth / OIT_DEPTH_STEP`). **`enumerate_ordinals(...)` returns panel-global
  ranks with the z-level discarded, so a flat `.flatten().count()` measures the whole
  panel, not a level's band occupancy.** Group by z-level instead: the per-command
  level is `DrawCommandDepth.z_level()` (`render/draw_order.rs`), and the projection
  exposes only flat ordinals + `z_level`/`ordinal_index` — there is no per-level
  count API, so add one (e.g. `DrawOrderProjection::level_occupancy()`) or count per
  `z_level` at the guard. Past either ceiling, ordering degrades to best-effort OIT
  insertion order — no silent truncation; emit the warning.
- Restate "lines just under text" as `DrawStep::Lines < DrawStep::Text`, not
  `63 < DEFAULT_DRAW_LAYER`, wherever a comment references the deleted constant.

**Files:** `layout/render.rs`, `layout/engine/positioning.rs`, `layout/line.rs`,
`lib.rs` + `layout/mod.rs` (drop `PanelLineLayering`/`PanelLinePaintOrder` re-exports),
`cascade/constants.rs`, `render/panel_geometry.rs`, `render/draw_order.rs` (per-level
count helper), `render/analytic_paths/batching.rs` (dead `layer` field),
`render/panel_text/batching.rs` (dead `layer` write), `render/panel_text/layout.rs`,
`render/panel_text/reconcile.rs` (carrier fields), `tests/integration_tests.rs`
(`PanelLinePaintOrder` assertions).

**Constraints from prior phases:** Phase 4/4a re-keyed every depth read and every
reconcile carrier onto the enumerated ordinal, so `draw_slot` and
`DEFAULT_DRAW_LAYER` have no remaining *ordering* readers except the overflow guard
and the dead `BatchKey.layer` writes. The draw-order engine now lives in
`render/draw_order.rs`; the per-command level is `DrawCommandDepth.z_level()`, and
the projection exposes only flat ordinals (`ordinal_index`) + `z_level` (no per-level
count API). The `with_draw_layer`/`set_draw_layer` verbs + `glyph_cascade.rs`
`Override`/`Resolved<DrawLayer>` resolution + propagation/reconcile arms are old-model
machinery still fully wired (a test pins that they do not split batches). **Decision:
Phase 6 owns the entire `DrawLayer`-cascade teardown** (verbs, `glyph_cascade.rs`
observer, `Override`/`Resolved<DrawLayer>` resolution, the cascade declaration at
`resolved.rs:90`, and the `DEFAULT_DRAW_LAYER` constant), done together with the
rename — do NOT delete any cascade machinery or `DEFAULT_DRAW_LAYER` in Phase 5.

**Acceptance gate:** `cargo build` clean with `draw_slot` gone and the dead analytic
`BatchKey.layer` field removed (`DEFAULT_DRAW_LAYER` + the cascade machinery survive
to Phase 6); `cargo nextest run` green; overflow warning fires at the **smaller** of
the per-level band-capacity ceiling (`DRAW_LEVEL_GEOMETRY_LANES`) and the OIT budget.

### Retrospective

**What worked:**
- Full removal landed clean: `PanelLineLayering`/`PanelLinePaintOrder`, their
  re-exports, `ResolvedPanelLineCommand.layering`, the carrier `draw_slot` fields, and
  the analytic `BatchKey.layer` field/write are all gone; `draw_slot` has zero
  references left in `src`. 415 tests pass, clippy clean.
- Two-ceiling guard implemented as intended: `DrawOrderProjection::level_occupancy()
  -> Vec<(i8, usize)>` plus pure predicates `per_level_band_overflows(busiest)` and
  `oit_total_overflows(panel_total)` in `panel_geometry.rs`, each `warn_once!`
  independently. Per-level band measures the busiest single z-level; OIT budget
  measures the panel-global sum — the two genuinely distinct quantities the Phase 4a
  review flagged.

**What deviated from the plan:**
- Added `ScreenDepthBias(f32)`/`OitDepthOffset(f32)` newtypes in `draw_order.rs`
  (user-requested mid-phase) — the depth helpers now return these, `.get()` only at
  material/buffer write boundaries. Not in the original Work Order; it is an
  ergonomics win the Phase 6 rename inherits.
- `OIT_FOCUS_DEPTH = 0.001` constant added to `constants.rs` to name the OIT-budget
  numerator (`oit_depth_budget() = floor(OIT_FOCUS_DEPTH / OIT_DEPTH_STEP) ≈ 1000`),
  with a doc comment on the 24-bit / ~17-quanta calibration so 1e-7 (→ layer merge)
  is not reintroduced.
- The "semver-visible public-API removal" framing in the Spec is moot — the crate is
  unpublished, so the public-symbol deletion carried no real constraint.

**Surprises:**
- `DEFAULT_DRAW_LAYER`'s only surviving non-test use is the `default =` in
  `cascade_attr!(DrawLayer(i8), default = DEFAULT_DRAW_LAYER, eq)` (`resolved.rs:88`).
  The cascade still resolves each text run's `DrawLayer` to 64, but **nothing consumes
  that resolved value for ordering** — it is fully inert machinery awaiting Phase 6.
  Its doc comment was rewritten to say so.

**Implications for remaining phases:**
- Phase 6 inherits the newtypes (`ScreenDepthBias`/`OitDepthOffset`) and the
  `OIT_FOCUS_DEPTH` constant in its rename blast radius.
- Phase 6's cascade teardown surface is unchanged from the Phase 4a review's
  assignment, now confirmed against shipped code: `resolved.rs:91` `default =`, the
  verbs, `glyph_cascade.rs`, the `Override`/`Resolved<DrawLayer>` path, and
  `DEFAULT_DRAW_LAYER` itself all still compile and are still inert.

### Phase 5 Review

Architect re-evaluation of Phases 6/7 against shipped Phase 5 code. All findings were
determined corrections (no open user decisions); applied to the Phase 6 Work Order:
- **Shared observer (was a rendering-break trap):** Phase 6 spec now says delete only
  the four `DrawLayer` arms of `seed_panel_text_child_glyph` (`glyph_cascade.rs:33/38/55–60/70`),
  not the observer — it also resolves `Resolved<Lighting>`/`Sidedness>`/`AntiAlias>`.
- **Missing teardown file:** added `render/panel_text/mod.rs` (production
  `CascadePlugin::<DrawLayer>` `:80` + `use … DrawLayer` `:40`) to Phase 6 Spec/Files —
  the teardown would not have compiled without it; `batching.rs` registrations are test-only.
- **Dead test citations:** parity test is `sorted_and_oit_orderings_agree_for_every_z_level_pair`
  at `draw_order.rs:493` using `representative_streams()` + `RAISED_LEVEL`/`LOWERED_LEVEL`;
  `ORDERED_LAYER_PAIRS`/`(i8,i8)` pairs no longer exist. Corrected.
- **Re-point heads-up narrowed:** the `glyph_cascade.rs mod tests` `DEFAULT_DRAW_LAYER`
  uses are deleted with the machinery, not re-pointed; no draw-order test references it.
- **`#[cfg(test)]` re-export constraint:** recorded that Phase 5 narrowed
  `cascade/mod.rs:139` to test-only; Phase 6 deletes it + the `glyph_cascade.rs:88` import.
- **Drifted refs / count:** `resolved.rs` declaration `:87`/`:88` (not `:90/:91`);
  added `text_props.rs:366` getter + `reconcile.rs:218` reader; rename count updated to
  ~214 across 16 files; flagged the `ScreenDepthBias`/`OitDepthOffset`/`OIT_FOCUS_DEPTH`
  newtypes as read-review only (no `DrawLayer` token, not renamed).
- **Phase 7:** unaffected — its per-level-band ceiling constraint matches shipped
  `constants.rs`/`draw_order.rs`; no change.

### Phase 6 — Flag-day rename + example + test/doc cleanup · status: todo

#### Work Order

**Goal:** Rename `DrawLayer → DrawZIndex` (editor-driven, user-approved), ship the
in-panel-overlay example, and finish test/doc cleanup.

**Spec:**

- **Confirm the rename with the user first** (rename-through-editor convention).
  Re-derive the edit scope at execution — citations across this plan have
  drifted; as of the post-Phase-5 review `DrawLayer` + `draw_layer` was **~214 refs
  across 16 files** (`grep -rho` count), `DEFAULT_DRAW_LAYER` adds 4 production+test
  sites. `cascade_attr!` regenerates the verbs, `Reflect`, and the BRP type path
  automatically — no hand-written reflection sites.
- Rename via the editor: `DrawLayer → DrawZIndex`, `draw_layer → draw_zindex` —
  the `El`/`Element` field, `TextStyle` field (`text_props.rs:218`) + builder
  `with_draw_layer` (`:534`) + setter `set_draw_layer` (`:611`) + getter
  `draw_layer()` (`:366`), cascade declaration (`cascade/resolved.rs:87`,
  `DrawLayer(i8)`), verbs (`cascade/attributes.rs:52/95/159`), and readers
  (`reconcile.rs:218` `config.draw_layer()`, `glyph_cascade.rs`,
  `panel_text/batching.rs`). **Post-extraction surface (Phase 4a)
  the rename must also cover:** `render/draw_order.rs` (the engine — `DrawLayer`
  import, `From<DrawLayer>`, `HierarchicalDrawKey.z_index: Option<DrawLayer>`, and the
  test module's `RAISED_LEVEL`/`LOWERED_LEVEL` consts — note **`ORDERED_LAYER_PAIRS`
  no longer exists**; the parity test now uses `representative_streams()`), the
  `text_element`/`text_id_element` builders (`layout/builder.rs`), and `PanelTextZLevel`
  (`panel_text/layout.rs:45`). The `ScreenDepthBias`/`OitDepthOffset` newtypes,
  `OIT_FOCUS_DEPTH`, `line_batch_depth_bias`/`text_batch_depth_bias`, and `DRAW_LEVEL_*`
  contain **no `DrawLayer` token** — they are NOT part of the rename; review only that
  they still read clearly afterward. Re-confirm every cited line number at rename time.
- Rewrite the example `examples/text_draw_layer.rs` → e.g. `panel_draw_order.rs`:
  one panel, a text child and a sibling overlay quad in the same tree, ordered with
  `DrawZIndex` and a hotkey toggle — not the current second-anchored-panel fake.
  Author raised/lowered text via the now-working `text_element(El::new()
  .draw_layer(...), …)` path (Phase 4a), and a sibling overlay quad via
  `El::draw_layer`. **The current example's hotkey handler calls the retired cascade
  verbs `override_draw_layer`/`inherit_draw_layer` (`:314/:317`) and `with_draw_layer`
  (`:374/:377`) — Phase 6 deletes those verbs, so the rewrite must drop that path
  entirely, not rename it.**
- The parity test is now `sorted_and_oit_orderings_agree_for_every_z_level_pair` at
  `render/draw_order.rs:493` (renamed + moved by the extraction); it iterates z-level
  pairs via `representative_streams()` + the `RAISED_LEVEL`/`LOWERED_LEVEL` consts
  (`:284/:295`), **not** `ORDERED_LAYER_PAIRS`/`(i8,i8)` layer pairs (those no longer
  exist). Phase 4a also added `hierarchical_depth_bias_and_oit_orderings_agree` and the
  no-override material equivalence test. Reconcile against those (do not describe the
  test as untouched): generalize remaining coverage over `(HierarchicalDrawKey,
  HierarchicalDrawKey)` pairs (unset at different steps; unset vs `z = 0` same step;
  unset vs set across steps; raised/lowered pairs) only where not already covered.
- `coverage_probe.rs` `EXPECTED_SHADER_FNV1A` was refreshed in Phase 4
  (the `OIT_MIN_DEPTH` retune). The rename does not touch shader text
  (`draw_layer` is passed as a precomputed offset through the existing shader
  input), so **no further FNV refresh here** — but first **verify
  `EXPECTED_SHADER_FNV1A` matches the current `analytic_path.wgsl`** (Phase 4a is
  uncommitted and moved through two fix passes; confirm the hash before relying on
  no-refresh).
- **Tear down the entire `DrawLayer` cascade machinery here** (Phase 5 deliberately
  left it wired): the `with_draw_layer`/`set_draw_layer` verbs (`text_props.rs`), the
  `Override`/`Resolved<DrawLayer>` resolution path, the cascade declaration
  (`resolved.rs:87`, `default = DEFAULT_DRAW_LAYER` at `:88`), the
  `override_draw_layer`/`inherit_draw_layer`/`resolved_draw_layer` verbs
  (`attributes.rs:52/95/159`), and the `DEFAULT_DRAW_LAYER` constant
  (`cascade/constants.rs:16`). **The production `CascadePlugin::<DrawLayer>`
  registration and the `seed_panel_text_child_glyph` observer wiring live in
  `render/panel_text/mod.rs` (`:80` plugin, `:82` observer, `:40` `use … DrawLayer`),
  NOT in `batching.rs` (those are test-only) — delete the `CascadePlugin::<DrawLayer>`
  line and its import there.** Also delete the now-`#[cfg(test)]`-gated re-export
  `cascade/mod.rs:139` (`pub(crate) use constants::DEFAULT_DRAW_LAYER`, narrowed to
  test-only in Phase 5) and the `glyph_cascade.rs:88` test import.
- **`seed_panel_text_child_glyph` (`glyph_cascade.rs:29`) is a SHARED observer** — it
  resolves and inserts `Resolved<Lighting>`, `Resolved<Sidedness>`, `Resolved<DrawLayer>`,
  AND `Resolved<AntiAlias>` together; the other three are consumed for rendering
  (`panel_text/batching.rs`, `panel_lines/batching.rs`). **Do NOT delete the observer.**
  Surgically remove only the four `DrawLayer`-specific lines: the `draw_layer_overrides`
  query (`:33`), `draw_layer_default` (`:38`), the `resolve_walk::<DrawLayer>` block
  (`:55–60`), and the `Resolved(draw_layer)` insert (`:70`). Deleting the whole
  observer would break lighting/sidedness/anti-alias.
- Doing the teardown with the rename keeps the cascade declaration, verbs, and
  `DEFAULT_DRAW_LAYER` coming out together so the rename lands on a clean post-cascade
  surface. The test pinning that `TextStyle` draw-layers do not split batches
  (`panel_text/batching.rs`) goes with them. The cascade-resolution tests inside
  `glyph_cascade.rs mod tests` (`:88/:159/:214/:246`) are **deleted with the
  machinery, not re-pointed** — no surviving draw-order test references
  `DEFAULT_DRAW_LAYER` (the line/text lane pins already use literal `63`/`64`).
- Delete `as-built/text-draw-layer.md` once the old mechanism is gone.

**Files:** `examples/text_draw_layer.rs` (→ renamed), `render/draw_order.rs`
(engine + parity test), `layout/text_props.rs`, `layout/builder.rs`
(`text_element`/`text_id_element`), `render/panel_text/layout.rs` (`PanelTextZLevel`),
`render/panel_text/reconcile.rs` (`config.draw_layer()` reader), `cascade/resolved.rs`,
`cascade/attributes.rs`, `cascade/constants.rs` (`DEFAULT_DRAW_LAYER` deletion at `:16`),
`cascade/mod.rs` (delete the `#[cfg(test)]` `DEFAULT_DRAW_LAYER` re-export at `:139`),
`render/panel_text/glyph_cascade.rs` (surgical `DrawLayer`-arm removal — keep the
shared observer), **`render/panel_text/mod.rs` (delete `CascadePlugin::<DrawLayer>`
`:80` + `use … DrawLayer` `:40`)**, plus every renamed reference (editor-driven), and
`docs/bevy_diegetic/as-built/text-draw-layer.md` (delete). (`render/constants.rs`
`DRAW_LEVEL_*` and the depth-bias newtypes carry no `DrawLayer` token — read-review
only, not edited.)

**Constraints from prior phases:** Phase 4 refreshed `EXPECTED_SHADER_FNV1A`. Phase 5
deleted the `draw_slot` machinery and the line-layering types but **left
`DEFAULT_DRAW_LAYER` and the whole `DrawLayer` cascade machinery intact** — Phase 6
owns deleting them (the Spec bullet above), together with the rename. Phase 5 also
**narrowed the `cascade/mod.rs` `DEFAULT_DRAW_LAYER` re-export to `#[cfg(test)]`**
(`:139`) — the only remaining production reader is `resolved.rs:88` (`default =`); every
other reference is test-only. The `seed_panel_text_child_glyph` observer is shared
across four cascade attributes — only the `DrawLayer` arms come out. The new model is
fully wired into rendering; the rename + cascade teardown must not change ordering
behavior (a parity test guards it).

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
- **Per-level screen-band ceiling (inherited from Phase 4a).** The screen `depth_bias`
  axis gives each z-level only `DRAW_LEVEL_GEOMETRY_LANES = 64` geometry sub-lanes
  (`DRAW_LEVEL_STRIDE = 65`; lines at `63`, text at `64`); `level_sublane_depth_bias`
  saturates into the next band past that. Batching many fills into one draw does NOT
  relax this — each fill still needs a distinct sub-lane on the screen view, so a
  panel with >64 fills at one z-level already overflows the band. The design must
  resolve how batched fills occupy the geometry sub-lanes (share them by per-record
  ordinal? a finer intra-batch screen ordering? widen the band?) — the OIT path has
  the focus-depth budget; the screen path has this hard 64-lane-per-level cap.

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

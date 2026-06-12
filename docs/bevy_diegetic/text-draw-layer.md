# Text draw layer

Status: **Phases 1–5 implemented** (Phase 1: audit, `DrawOrdinal` mapping,
backing OIT inversion fix, D2 diagnostic, tests; Phase 2: `TextDrawLayer`
cascade attribute, `TextStyle` integration, `From<TextDrawLayer>` routing;
Phase 3: `BatchKey.layer` routing + per-layer material derivation; Phase 4:
`examples/draw_layer.rs` two-view demo + the `OIT_DEPTH_STEP`
recalibration and `OIT_MIN_DEPTH` shader floor that its OIT verification
forced; Phase 5: geometry draw-slot compaction, merged from the draw-line
integration branch). Pauses the anchor-to-panel example work (Phase
4.3/4.4); resume that after this lands.

Naming: after Phase 5 the `text` prefix was dropped from the attribute,
its default constant, and the cascade verbs — now `DrawLayer`,
`DEFAULT_DRAW_LAYER`, `override_draw_layer` / `inherit_draw_layer` /
`resolved_draw_layer`. Historical sections below keep the names as
originally written.

## Intent

Panel text always draws above panel backings today because
`BATCH_TEXT_DEPTH_BIAS = 64.0` is hard-coded into every batch material
(`render/constants.rs:40`, applied at `render/panel_text/batching.rs:714`).
That guarantee is the right default, but it makes draw order unauthorable: a
sliding subpanel can never composite over a sibling's text, and no text can
tuck behind a backing for effect.

Make the text draw order an authorable per-run value with the current
behavior as the default. One authored ordinal — the **draw layer** — derives
both ordering mechanisms:

- sorted (non-OIT) views: `StandardMaterial::depth_bias` on the batch
  material (`Transparent3d` sort key)
- OIT views: the `oit_depth_offset` added to `position.z` in the fragment
  shader before `oit_draw` (pipeline `depth_bias` does not affect
  `in.position.z` — the depth_bias/OIT finding from the WorldText work)

The author writes one small integer; the two-mechanism duality stays an
implementation detail. This mirrors what backings already do internally:
`command_index` drives both `depth_bias` (`render/panel_geometry.rs:444`) and
`panel_backing_oit_depth_offset` (`render/constants.rs:48`).

## Semantics

- A draw layer is an ordinal on the same axis as geometry draw slots
  (Phase 5; originally raw backing `command_index`): text with layer `L`
  draws above every panel child whose draw slot is below `L` and below
  every child at or above it.
- Default layer = 64, reproducing today's constant and its documented
  headroom assumption (no panel has 64 slot-consuming geometry commands;
  before Phase 5's compaction the bound was on raw command count).
- The value is a bounded small integer (`i8`), not an `f32`. Bounding keeps
  the OIT offset within range — text currently sits at OIT offset `0.0` so
  real opaque geometry keeps depth authority over it, and large authored
  values would erode that. It also avoids the raw-float draw-order hazards
  identified in the screen-anchoring z-offset discussion.
- This is occlusion order, not clipping. A translucent backing drawn over
  text dims it; hard clipping stays on the existing clip-rect path.
- The layer orders a panel's children against each other on one view. Across
  panels, the `Transparent3d` distance term dominates: a layer never reorders
  text against another panel's children once the panels' depths differ by
  more than the bias span (the existing 64-px screen-anchoring interaction,
  documented on `PanelAnchorOffset`).
- Scope is text runs only. Backings and image children are not authorable;
  their ordinals are emission-order draw slots (Phase 5 — `command_index`
  survives only as the reconcile identity key).
- Panel-vs-panel stacking (screen anchoring depth) is out of scope; it
  resumes in anchor-to-panel Phase 4 and should derive its layer quantum
  from the constants this plan defines, so "text wins by default, authors
  can override" is stated once.

## Public API

Get explicit API approval for names and the default before landing.

New cascade attribute, following the `TextAlpha` / `TextLighting` /
`TextSidedness` template (`cascade/resolved.rs`, `cascade/attributes.rs`),
declared through the `eq` variant of `cascade_attr!` (`i8` is an
exact-equality type, matching `TextAlpha` / `TextLighting`):

```rust
cascade_attr!(
    /// Draw-order layer for a text run relative to its panel's backing layers.
    TextDrawLayer(i8),
    default = DEFAULT_DRAW_LAYER,
    eq
);
```

- `DEFAULT_DRAW_LAYER: i8 = 64` lives in `cascade/constants.rs` — the
  macro's default expression evaluates at the macro site, and `cascade/`
  must not import from `render/` (current dependency direction is
  `render/ → cascade/`, one-way). `render/constants.rs` re-expresses
  `BATCH_TEXT_DEPTH_BIAS` through it.
- `CascadeDefault<TextDrawLayer>` resource from the macro default.
- entity-command overrides `override_draw_layer` /
  `inherit_draw_layer`, matching the existing attribute commands.
- `TextStyle::with_draw_layer(TextDrawLayer)` — house style: authoring
  methods take the high-level type, never the raw scalar (`with_lighting`,
  `with_weight`). `TextStyle` gains `draw_layer: Option<TextDrawLayer>`
  beside `lighting` / `sidedness`, including the `PartialEq` impl update;
  the `glyph_cascade.rs` seeding observer inserts the override when present,
  same as `with_lighting` / `with_sidedness`.

Derivation lives as methods on the type, defined in an `impl TextDrawLayer`
block in `render/constants.rs` beside the constants they consume (the type
is from `cascade/`, the derivation is render arithmetic — the impl in
`render/` keeps the dependency direction one-way):

```rust
/// Shared draw-order axis (D3): text layers and backing command indices
/// both convert into it; the derivations below are the only bias/offset
/// code path.
pub(crate) struct DrawOrdinal(i32);

impl DrawOrdinal {
    pub(crate) fn depth_bias(self) -> f32;       // ordinal × LAYER_DEPTH_BIAS
    pub(crate) fn oit_depth_offset(self) -> f32; // min(0, ordinal − 64) × OIT_DEPTH_STEP (D1)
}
```

`TextDrawLayer` and backing `command_index` convert into `DrawOrdinal`
(`From<TextDrawLayer>`, plus a checked conversion for `usize` command
indices); `batch_material` and the backing material builders go through it
for `depth_bias`. Whether backing *OIT* offsets also route through
`oit_depth_offset` or keep `panel_backing_oit_depth_offset` is settled by
the Phase 1 backing-vs-backing audit.

## Batching

`BatchKey` (`render/panel_text/batching.rs:218`) gains a `layer` field.
Runs sharing a layer still batch together; the default population stays one
batch; each distinct override value costs one extra phase item (one draw
call + one material). `BatchKeyCascades` gains the `Resolved<TextDrawLayer>`
query, its default resource, and the `Changed` arm in the re-route set, so a
layer change moves the run to the new key's batch the same way an alpha
change does (`alpha_cascade_change_moves_the_run_to_the_new_keys_batch` is
the model test).

The per-record `depth_nudge` (`batching.rs:239`) is unchanged: it orders
coplanar glyphs *within* a batch and is independent of the batch's layer.
Each distinct layer is its own batch with its own coplanar glyph set; glyphs
in different batches order by the batch materials' layer-derived fields, not
by `depth_nudge`.

`BatchKey` derives `Clone, Debug, Eq, Hash, PartialEq`
(`text/slug/runtime/batch_store.rs`); `i8` satisfies all of them, no custom
impls. First-frame routing needs no `Changed` arm: `update_panel_text_batches`
routes any not-yet-routed run unconditionally, so a label spawned with an
override routes to the override batch on its first routed frame — the
changed-set only re-routes already-routed runs.

## Implementation Phases

### Phase 1 — ordering parity audit and unified mapping

No public API. Establish that one ordinal can order both view types
consistently before exposing it.

**Audit results (implemented):**

- Sorted-path mechanism confirmed in bevy 0.19: `queue_material_meshes`
  copies `material.properties.depth_bias` into
  `TransparentSortingInfo3d::Sorted` for `Transparent3d`/`Transmissive3d`
  only; `sort_distance = view_z + depth_bias`, ascending sort, drawn
  back-to-front — higher bias composites in front.
- Backing-vs-backing inversion confirmed: sorted biases
  (`command_index × LAYER_DEPTH_BIAS`) rise with command index while the old
  OIT offsets (`-(command_index + 1) × OIT_DEPTH_STEP`) fell with it
  (reverse-Z, positive = closer), so OIT views composited higher commands
  *behind* lower ones. Fixed by routing backing OIT offsets through
  `DrawOrdinal::oit_depth_offset` — `panel_backing_oit_depth_offset` deleted.
  Backing offsets are now `(command_index − 64) × OIT_DEPTH_STEP` (still all
  negative, so text keeps OIT depth authority); the magnitude at command 0
  grew from `−1 × OIT_DEPTH_STEP` to `−64 × OIT_DEPTH_STEP` — the
  pre-existing-correction visual change called out by the plan.
- Shadow/prepass: the Shadow phase reads only the light's
  `shadow_depth_bias`; prepass queueing reads no material depth bias.
  `oit_depth_offset` is applied only inside `#ifdef OIT_ENABLED` fragment
  branches (`sdf_panel.wgsl`, `slug_text.wgsl`), which shadow/prepass
  pipelines never define. A draw layer cannot move shadows; no pinning
  needed.
- Images: command-index bias routes through `DrawOrdinal` on the sorted
  path. On OIT views image children use the stock PBR shader (no offset
  uniform), so they tie with text at unmodified fragment depth — fixing that
  needs a custom image shader; out of scope, unchanged behavior.
- Callouts (`callouts/render.rs`) use their own ordinal axis with *positive*
  OIT offsets (`order × OIT_DEPTH_STEP`) — internally consistent across both
  paths but in front of the panel-content band; not panel children, out of
  scope, unchanged.
- D2 diagnostic added: `reconcile_sdf_quads` emits `warn_once!` when a
  panel's render-command count reaches `DEFAULT_DRAW_LAYER`.
- `DEFAULT_DRAW_LAYER: i8 = 64` added to `cascade/constants.rs`
  (Phase 2 macro site); `DrawOrdinal` lives in `render/constants.rs` with
  `From<i8>` and a saturating `from_command_index(usize)`.
  `BATCH_TEXT_DEPTH_BIAS` deleted; `batch_material` derives both fields from
  `DrawOrdinal::from(DEFAULT_DRAW_LAYER)` (bit-equal to the old
  constants, pinned by test).

**Phase 1 review (team_review, 1 cycle — 4 lenses: correctness, risk,
style, type system):**

- Correctness and style lenses: no findings; all Phase 1 acceptance items
  and house-style rules verified met.
- Mechanical (auto-recorded, applied):
  - `from_command_index` doc comment now states the `i32::MAX` saturation
    semantics (saturated ordinal sits above every text layer; unreachable
    in practice).
  - Comment above the D2 diagnostic notes `warn_once!` is per-callsite —
    only the first offending panel is named.
  - Phase 2 note: when `TextDrawLayer` lands, route conversions through
    `From<TextDrawLayer>` and revisit whether `From<i8>` should be deleted
    so the raw scalar cannot bypass the attribute type (recorded below in
    Phase 2).
- Risk lens, recorded as verification note: the backing OIT compositing
  change (pre-existing-correction) should be confirmed visually on an OIT
  world view with overlapping translucent backings the next time one is
  launched; the regression test pins material values, not pixels.
- Dropped: rewriting the D2 comparison as
  `render_commands.len() >= DEFAULT_DRAW_LAYER as usize` — the
  suggested form uses a banned `as` cast and the `DrawOrdinal` comparison
  keeps both operands on the one ordering axis by construction.
- Dropped: `debug_assert!` inside `from_command_index` — the saturation
  threshold (2^31 commands) is unreachable before memory exhaustion; the
  doc comment and saturation test cover the contract.

- Audit the three child orderings on both paths:
  - backings: `depth_bias = command_index × LAYER_DEPTH_BIAS` vs
    `oit_depth_offset = -(command_index + 1) × OIT_DEPTH_STEP`
  - batched text: `depth_bias = 64` vs `oit_depth_offset = 0.0`
  - images (`render/panel_text/reconcile.rs:617`): command-index
    `depth_bias`, no OIT offset
- Verify the relative order of backing/text/image materials matches between
  the sorted sort key and the OIT depth offsets. Text-vs-backing parity
  holds today (negative backing offsets behind text's `0.0`, reverse-Z
  positive-is-closer; `sdf_panel.wgsl` and `slug_text.wgsl` both apply
  `oit_pos.z += offset`). The open case is *backing-vs-backing*: sorted
  biases put higher commands closer while the OIT offsets
  `-(command_index + 1) × OIT_DEPTH_STEP` put higher commands farther. The
  concrete audit: a panel with overlapping backing layers at two command
  indices (background + border), asserted on a sorted view and an OIT view.
  If the orders disagree, that is a pre-existing inversion on OIT views —
  fix it here with a regression test before building on the axis, and call
  the visual change out as a pre-existing correction, not a layer side
  effect.
- Audit whether `StandardMaterial::depth_bias` participates in any
  non-`Transparent3d` phase for these materials (shadow/prepass
  `DepthBiasState`). Draw layer must not move shadows: if shadow passes
  consume the bias, pin the shadow-side value to the default regardless of
  layer, and keep a test that a layer change leaves the shadow silhouette
  unchanged.
- Define the `TextDrawLayer::depth_bias` / `oit_depth_offset` methods in
  `render/constants.rs` such that:
  - the default layer reproduces today's exact material values (bit-equal)
  - any two layers order the same way on both paths
  - the text-at-`0.0` OIT depth-authority invariant holds at the default
  - the OIT formula follows decision D1 (see Proposed user decisions)
- Re-express `BATCH_TEXT_DEPTH_BIAS` and `panel_backing_oit_depth_offset`
  through the mapping (or document why backings stay on their own formula).
  The mapping methods are stateless arithmetic; they import nothing from
  `cascade/` beyond the type.

Tests:

- table-driven: for layer/command pairs, sorted order and OIT order agree
- default layer produces bit-equal `depth_bias` and `oit_depth_offset`
  against the current constants
- backing/text/image cross-ordering parity (the audit, kept as a regression
  test), including the overlapping background+border backing-vs-backing case
- shadow/prepass: a non-default layer leaves shadow-relevant material state
  identical to the default's

### Phase 2 — `TextDrawLayer` cascade attribute

- Add the attribute beside `TextAlpha` / `TextLighting` / `TextSidedness`:
  `cascade_attr!` with `eq`, `CascadeDefault` registration in the cascade
  plugin, override/inherit entity commands, reflection registration,
  `DEFAULT_DRAW_LAYER` in `cascade/constants.rs`.
- `TextStyle` integration in full: `draw_layer: Option<TextDrawLayer>`
  field, `with_draw_layer(TextDrawLayer)` builder, `PartialEq` impl update,
  and the `glyph_cascade.rs` seeding observer inserting the override —
  same call sites as `with_lighting` (`panel/diegetic_panel.rs`,
  `render/panel_text/glyph_cascade.rs`).
- No render-side consumption yet; resolved values exist but nothing reads
  them. This keeps the phase mechanically verifiable through the cascade
  test patterns.
- Add `From<TextDrawLayer> for DrawOrdinal` and route all layer
  conversions through it; revisit whether the Phase 1 `From<i8>` impl
  should be deleted so the raw scalar cannot bypass the attribute type
  (Phase 1 review note).

Tests:

- override → resolved propagation, inheritance, and removal follow the
  existing attribute tests
- `TextStyle::with_draw_layer` lands the override on the label entity

**Phase 2 results (implemented):**

- `cascade_attr!(TextDrawLayer(i8), default = DEFAULT_DRAW_LAYER, eq)`
  in `cascade/resolved.rs`; `override_draw_layer` /
  `inherit_draw_layer` entity commands and the public
  `resolved_draw_layer` reader in `cascade/attributes.rs`; exports from
  `cascade/mod.rs` and `lib.rs`; `CascadePlugin::<TextDrawLayer>` registered
  in `TextRenderPlugin` (reflection registration comes from the plugin).
  The override verb and reader take/return `TextDrawLayer`, not raw `i8` —
  the house-style "high-level type, never the raw scalar" rule.
- `TextStyle` integration: `draw_layer: Option<TextDrawLayer>` field,
  `draw_layer()` getter, `with_draw_layer` / `set_draw_layer`, `PartialEq`
  update. The field follows the `alpha_mode` model (cascade-only routing):
  cleared by `for_shaping()`, excluded from `gating_eq` /
  `hash_layout` / `layout_eq_excluding_visuals`, captured by
  `reconcile_panel_text_children` before `for_shaping()` and inserted/removed
  as `Override<TextDrawLayer>` on the label (spawn and reuse arms).
- `seed_panel_text_child_glyph` also seeds `Resolved<TextDrawLayer>` at
  label spawn, beside lighting and sidedness.
- `From<i8> for DrawOrdinal` **deleted**; replaced by
  `From<TextDrawLayer> for DrawOrdinal`. Both prior `From<i8>` call sites
  (`batch_material`, the D2 diagnostic) and the ordering-parity tests now
  construct `TextDrawLayer(...)` — the raw scalar cannot bypass the
  attribute type.
- No render-side consumption of resolved values yet, per the phase scope:
  `batch_material` still derives from the default layer; Phase 3 moves that
  to the batch key.
- Tests (4 new, in `glyph_cascade.rs`): default resolution without an
  override; `with_draw_layer` lands `Override<TextDrawLayer>` + resolves on
  the label; a tree edit dropping the style value removes the override and
  re-inherits the default through reconcile; `override_draw_layer` /
  `inherit_draw_layer` round-trip with `Resolved` self-heal. The
  batching `pipeline_app` test fixture gained
  `CascadePlugin::<TextDrawLayer>` (the seed observer now requires its
  `CascadeDefault`). Full suite 335/335 passed; build, clippy
  (`--all-targets`), and fmt clean.

**Phase 2 review (team_review, 1 cycle — 4 lenses: correctness, risk,
style, type system):**

- Correctness and risk lenses: no findings. All Phase 2 plan items verified
  present and following the `alpha_mode` model; observer/resource
  registration chains complete (`TextRenderPlugin`, both test fixtures);
  default-layer arithmetic bit-equal to pre-change constants (pinned by
  test); `From<i8>` confirmed deleted with no surviving call sites.
- Mechanical (auto-recorded, applied):
  - `resolved_draw_layer` doc comment now states why it returns
    `TextDrawLayer` rather than the inner `i8` (siblings return inner
    values; the bare scalar never crosses the API).
  - `backing_oit_offsets_stay_behind_default_text_and_rise_with_command_index`
    derives its loop bound from `DEFAULT_DRAW_LAYER` instead of a
    hard-coded `64` (derive-test-values-from-production-constants rule).
- Type-system lens, recorded as notes (no code change):
  - `layout/text_props.rs` now imports `cascade::TextDrawLayer` while
    `cascade/resolved.rs` imports layout types — the first two-way
    cascade ↔ layout module dependency. Legal intra-crate and accepted:
    `TextDrawLayer` is the first attribute whose inner type is a bare
    scalar, so `TextStyle` stores the wrapper itself. Revisit the pattern
    only if more scalar-wrapped attributes accumulate.
  - Phase 3 note: `cascade_attr!`'s `eq` variant does not derive `Hash`, so
    `BatchKey` stores `layer: i8` per the plan — Phase 3 unwraps
    `TextDrawLayer.0` at the key boundary (mirrors `TextAlpha` →
    `BatchAlphaMode` re-encoding).
- 0 proposed user decisions; nothing surfaced to `/adhoc_review`.

### Phase 3 — batch routing and material derivation

- `BatchKey` gains `layer: i8`; `BatchKeyCascades` gains the resolved query,
  default, and changed-set arm.
- `batch_material` derives both fields from the key's layer through the
  Phase 1 mapping instead of the flat constant; `oit_depth_offset` stops
  being hard-coded `0.0`.

Tests:

- two runs with different layers route to different batches; same layer,
  same other key fields → one batch; three layers → three batches
- a layer cascade change re-routes the run to the new key's batch, the
  emptied batch entity despawns, the new key's entity spawns (extends the
  `alpha_cascade_change_moves_the_run_to_the_new_keys_batch` model with
  batch-entity-count assertions)
- a label spawned with a layer override routes to the override batch on its
  first routed frame (the unrouted-run path, no `Changed` dependence)
- default-layer batch material is bit-equal to pre-change output
- a run with a layer below a backing's command index sorts below that
  backing and above lower commands, on both the sorted sort key and the OIT
  offset (material-value assertions, not pixels)

**Phase 3 results (implemented):**

- `BatchKey` gains `layer: i8` after `sidedness`
  (`text/slug/runtime/batch_store.rs`) — the resolved `TextDrawLayer`
  unwrapped to its inner `i8` at the key boundary, since the `eq` attribute
  variant derives no `Hash` (mirrors `TextAlpha` → `BatchAlphaMode`).
- `BatchKeyCascades` gains the `Resolved<TextDrawLayer>` query,
  `CascadeDefault<TextDrawLayer>`, a `draw_layer()` accessor returning
  `TextDrawLayer` (siblings return inner types; `i8`'s wrapper is kept until
  the key boundary), and `Changed<Resolved<TextDrawLayer>>` in the changed
  arm. Key construction unwraps: `layer: cascades.draw_layer(label).0`.
- `batch_material` derives both ordering fields from
  `DrawOrdinal::from(TextDrawLayer(key.layer))`; the flat
  `DEFAULT_DRAW_LAYER` derivation and its hard-coded-`0.0`-equivalent
  OIT offset are gone from the routing path (the default layer reproduces
  them bit-exactly, pinned by test).
- `TextExtension` gains a `#[cfg(test)] oit_depth_offset()` reader
  (`text/slug/render/material.rs`) — the uniform struct is module-private,
  and the batching ordering tests need the material's stored offset.
- Tests (5 new, in `batching.rs`): three distinct layers → three batches
  with the same-layer pair sharing one (entity-count asserted); a label
  spawned with a layer override routes to the override batch through the
  unrouted-run path (no `Changed` dependence); a live
  `override_draw_layer` re-keys the run — new key's entity spawns, and
  the follow-up `inherit_draw_layer` despawns the emptied batch entity
  while the default batch entity survives; default-layer batch material
  bit-equal to the pre-change `BATCH_TEXT_DEPTH_BIAS = 64.0` / `0.0` OIT
  pair; a layer-5 batch's material values sort strictly between backing
  commands 3 and 7 on both the sorted bias and the OIT offset. The
  `batch_store` test key helper stamps `layer: DEFAULT_DRAW_LAYER`.
  Full suite 340/340 passed; build, clippy (`--all-targets`), and fmt clean.

**Phase 3 review (team_review, 1 cycle — 4 lenses: correctness, risk,
style, type system):**

- Correctness and style lenses: no findings. All five plan test bullets
  verified present; no call site still derives batch ordering from the flat
  default; accessor naming and fallback consistent with the siblings; no
  banned words.
- Risk lens raised one note, reviewed and recorded without a code change:
  the seed-observer → routing frame order looked implicit. The mechanism is
  already enforced three ways: `apply_cascade_override` heals `Resolved`
  itself at command flush, the seed observer covers non-override labels,
  and a late `Resolved` insertion still matches the
  `Changed<Resolved<TextDrawLayer>>` arm (added counts as changed), so the
  worst case re-routes next frame rather than sticking on the default key.
  Same structure as alpha/lighting/sidedness; no new schedule edge.
- Type-system lens: no structural findings (BatchKey Hash/Eq composition,
  wrapper discipline, and the i8 key boundary all confirmed). One note
  recorded here per the no-design-decision-comments rule instead of as a
  source comment: `DrawOrdinal`'s only constructors are
  `From<TextDrawLayer>` and `from_command_index(usize)` — Phase 2's
  `From<i8>` deletion is what keeps every layer-derived ordering value
  traceable to the attribute type; any future `impl From<…> for
  DrawOrdinal` should be reviewed as a new ordering source. The
  `#[cfg(test)]` `oit_depth_offset()` reader was confirmed the right
  visibility tool (the uniform struct stays module-private).
- 0 proposed user decisions; nothing surfaced to `/adhoc_review`.

### Phase 4 — demo

- Example demonstrating the artistic case: a subpanel sliding over sibling
  text, the covered text dimming behind the translucent backing; one text
  run authored above everything as the contrast. Render the same content on
  a sorted screen view and an OIT world view (`StableTransparency`) to show
  the orderings agree.
- Decide during implementation whether this extends an existing example or
  adds `examples/draw_layer.rs`; keep the example layout convention
  (primary-API code first).

**Phase 4 results (implemented):**

- New `examples/draw_layer.rs` (the layout engine has no in-panel
  sibling overlap, so "subpanel sliding over sibling text" is a second
  textless panel attached via `AnchoredToPanel`, re-inserted per frame with
  a sinusoidal x offset). One `ViewSide` enum builds both views from the
  same content tree: a world panel (Mm units, OIT via
  `with_stable_transparency`) and a screen panel (Px units, sorted ortho
  view, `screen_panel_material`). Three tiers: body paragraph at layer 8
  (dims under the shade), one default-layer run and a layer-72 run (both
  stay bright over it — layer 72 clamps to the default's OIT offset per D1
  and out-sorts it on the screen view). Verified live on both views: the
  orderings agree.
- Shade depth sits between the tiers on each view's own axis: 16 logical px
  on the sorted view (8 < 16 < 64 exactly); 6 mm on the world view, where
  `bevy_lagrange` syncing the near plane to `radius × 0.001` makes the
  shade's NDC delta one `OIT_DEPTH_STEP` per millimeter per meter of orbit
  radius — in-band (8 < steps < 64) for radii of 0.094–0.75 m (camera-home
  lands at ~0.24 m, ≈25 steps).
- **The demo exposed a Phase 1 regression on OIT views** — the exact
  visual-change risk the audit called out. With `OIT_DEPTH_STEP = 1e-4`,
  the new `(command_index − 64)` offsets reach −6.4e-3 while a fragment at
  the camera's focus only has `position.z = near/d ≈ 1e-3` (the lagrange
  near-radius sync makes this distance-independent). The offset drove z
  negative; `pack_24bit_depth_8bit_alpha` saturates depth to 0, and bevy's
  OIT resolve (no `DepthPrepass` on the camera, so the manual-depth-test
  path) compares packed `(depth << 8) | alpha` values against the cleared
  background `(0 << 8) | 255` — every saturated fragment with alpha < 1.0
  packed below it and was silently dropped: panel fills, dividers, shade
  quads, and layer-8 text all invisible; only alpha-exactly-1.0 and
  offset-0 draws survived. Fix: `OIT_DEPTH_STEP` 1e-4 → 1e-6
  (`render/constants.rs`; 64 steps = 6.4 % of focus depth, one step ≈ 17
  quanta of the 24-bit packing, doc comment records the calibration), plus
  an `OIT_MIN_DEPTH = 2e-7` floor on the offset z before `oit_draw` in
  `sdf_panel.wgsl` and `slug_text.wgsl` so an out-of-calibration offset
  degrades to wrong ordering instead of invisibility. The slug shader-hash
  tripwire was updated (coverage math untouched). `panel_anchoring`'s world
  view shared the regression and renders correctly after the fix.
- D4 follow-up: `K` hotkey toggles the default-layer run between an
  explicit `DIM_TEXT_LAYER` override (dims behind the shade) and
  inheriting the cascade default (bright above it again) via
  `override_draw_layer` / `inherit_draw_layer`, addressing the
  run's wrapped line entities by `text_id` run id. Verified live in both
  directions on both views.
- Verification: full suite 340/340 (`cargo nextest run`), build, clippy
  (`--all-targets`), fmt clean; live screenshots of both views confirm the
  three-tier behavior and the sliding dim effect.

**Phase 4 review (team_review, 1 cycle — 4 lenses: correctness, risk,
implementation quality/style, demo ergonomics):**

- Correctness lens: all Phase 4 spec bullets verified delivered; the
  depth-band arithmetic (steps = shade mm / radius m; 6 mm in-band for
  0.094–0.75 m), the constants.rs calibration claims (64 steps = 6.4 % of
  focus depth, one step ≈ 17 quanta), and the `OIT_MIN_DEPTH` floor's
  packed value (3 quanta → packs above the cleared background for any
  alpha) all check out. D1's clamp behavior confirmed against the
  `sorted_and_oit_orderings_agree_for_every_layer_pair` test.
- Risk lens, auto-recorded (doc-only, applied): the far-distance bound —
  a panel much farther than the camera focus shrinks `position.z` below
  the 64-step budget at ~15.6× the orbit radius; past it the floor keeps
  fragments visible but coplanar ordering collapses to OIT-list insertion
  order. Documented on `OIT_DEPTH_STEP` and cross-referenced from both
  shaders' `OIT_MIN_DEPTH` comments. f32 precision of `z + k×1e-6` at
  focus depths verified non-issue; callouts' positive-offset axis
  unaffected by the step change; no test encodes the old magnitude.
- Risk lens, considered and dropped: a debug diagnostic when an offset
  hits the floor — fragment depth is per-pixel GPU state with no CPU
  feedback channel, and a CPU-side approximation would need the live
  camera distance per panel at material-build time; not implementable
  where the information exists.
- Style lens: no findings (conventions match `panel_anchoring` /
  `text_alpha`; no banned words; comments state mechanisms; no dead
  code or stale references).
- Ergonomics lens: two findings surfaced as D4/D5 below; status-line
  wording and a `DIM_TEXT_LAYER` doc expansion dropped (the module doc
  and the adjacent `SHADE_DEPTH_MM` comment already carry the math; the
  OIT/sorted vocabulary is this library's own).

### End-of-implementation discussion — inheritance of the text draw layer

Implementing the D4 hotkey surfaced how the draw layer's cascade behaves
at runtime; recording it here because the semantics constrain any future
runtime-layer API.

- **Authored styles are overrides.** `TextStyle::with_draw_layer` in a
  tree does not produce a distinct "authored" state: reconcile inserts
  the same `Override<TextDrawLayer>` component on the run's label
  entities that `override_draw_layer` would. The cascade resolution
  chain is label override → panel override → `CascadeDefault` (64);
  there is no fourth slot holding the tree's value.
- **`inherit` is destructive of authored values.** Removing an override
  has no memory — a run authored at layer 72 that is toggled
  `override(8)` → `inherit()` lands on the default (64), not back at 72.
  Restoring an authored non-default layer requires re-overriding with
  the original value, which the caller must have kept. This is why the
  demo's `K` toggle targets the default-layer run: it starts with no
  override, so `override`/`inherit` round-trips its true state exactly.
- **Runtime verbs act per line entity.** A wrapped run spawns one label
  entity per line; each line carries its own override and resolves
  independently. `DiegeticPanel::text_child(id)` resolves only the
  run's first line, so a whole-run runtime change must address all line
  entities — the demo authors the run with `text_id` and matches
  `PanelTextLayout.id` across entities. A verb applied to one line of a
  wrapped run silently splits the run's layering (the demo's first
  implementation did exactly this and only line 0 dimmed).
- **Implication for a future run-scoped API.** If runtime layer changes
  become a real authoring surface (beyond demos), the per-line and
  no-memory semantics argue for a run-scoped verb pair keyed by
  `PanelFieldId` (apply to every line of the run) and, if "restore the
  authored value" is wanted, an explicit stored authored-layer slot —
  both out of scope here, recorded as the natural next step.

### Phase 5 — geometry draw-slot compaction (implemented)

Merged from the draw-line integration branch (`bb3edde`, after the Phase 4
merge); recorded here because it modifies the Phase 1–3 ordinal mapping
directly. Motivation: raw `command_index` counts every render command, so
text-heavy panels burn ordinal headroom on commands that never draw
geometry — `diegetic_text_stress`'s status overlay reached ~74 commands and
tripped the D2 warning while emitting only ~5 geometry draws.

- `RenderCommand` (layout/render.rs) gains `pub draw_slot: usize`, stamped
  at emission (`layout/engine/positioning.rs`, `EmissionCounters` +
  `push_command`). Rectangle, border, divider, image, and lines commands
  each consume one slot; text and scissor commands record the next slot
  without consuming it (`RenderCommandKind::consumes_draw_slot()`).
- `DrawOrdinal::from_command_index` → `from_draw_slot`; every depth-bias /
  OIT-offset derivation feeds from slots. `From<TextDrawLayer>` is
  unchanged — text layers and geometry slots share one ordinal scale as
  before.
- Identity vs. ordering split: reconcile reuse keys keep raw
  `command_index` (`PanelSdfSurface`, dividers) so entities survive slot
  shifts; ordering comes from `draw_slot`, which also joins the SDF
  signature so a slot move rebuilds the material (where `depth_bias`
  lives) without respawning the entity.
- `PanelTextLayout.command_index` → `draw_slot`;
  `PanelImageChild.command_index` → `draw_slot`; text
  `RunRecord.depth_nudge` is now `draw_slot × LAYER_DEPTH_BIAS` — the
  former `+1` is gone because the recorded slot already equals the next
  geometry slot, preserving relative order exactly.
- The D2 warning in `panel_geometry.rs` checks the highest geometry slot,
  not `render_commands.len()`; it no longer fires in
  `diegetic_text_stress`.
- Supersedes the Semantics scope line "backings and image children keep
  their command-index ordinals" (amended above) and D2's command-count
  framing (noted on the decision).
- Verified on `bb3edde`: cargo build, `cargo +nightly fmt`, 527/527
  `cargo nextest run`, stress example log clean.

## Risks

- The Phase 1 audit may find OIT/sorted ordering disagreement in existing
  backing offsets; fixing it could shift rendering on OIT views that
  happened to depend on the current order. The regression test pins the
  corrected ordering.
- Batch count growth is bounded by distinct authored layers; no silent cap.
  If an app authors many layers the cost is visible as phase items, not
  corruption.
- `i8` saturation: layers beyond backing command depth are legal and just
  mean "above everything" / "below everything"; document rather than clamp.
- Layer changes at runtime re-route through `upsert_run` +
  `reconcile_batch_entities` in the same pass; the Phase 3 batch-entity
  tests assert the move is atomic per update (no frame where the run is in
  zero or two live batches).

## Proposed user decisions

- **D1 — OIT offset formula for non-default layers** (critical, Risk +
  Architecture lenses; class: design-improvement; status: proposed).
  Problem: `oit_depth_offset` shifts the fragment's stored z, so a layer
  above the default would move text *closer* in OIT space and can interleave
  with unrelated world geometry at nearby depth — eroding the depth-authority
  invariant that text-at-`0.0` preserves. Options: (a) full linear mapping
  `(layer − 64) × OIT_DEPTH_STEP`, symmetric with sorted views, bounded by
  i8 to ±0.0127; (b) clamp at `0.0` — layers below the default move text
  behind backings on OIT views, layers above are OIT-equal to the default
  (sorted views still honor them); (c) keep all text at `0.0` in OIT —
  layers affect sorted views only, the artistic demo works only on screen
  panels. Recommendation: (b) — under-default layers are the artistic case
  (text behind a sliding backing) and work on both view types; above-default
  never compromises depth authority.
  **Decision: (b) clamp at `0.0`** —
  `oit_depth_offset(layer) = min(0.0, (layer − 64)) × OIT_DEPTH_STEP`;
  layers above the default are OIT-equal to the default.
- **D2 — backing command-count bound** (important, Risk lens; class:
  design-improvement; status: proposed). Problem: the default layer's
  "above everything" guarantee assumes < 64 backing commands per panel;
  `gather_surfaces` (`panel_geometry.rs`) enumerates commands with no bound,
  so a 65-command panel silently breaks the guarantee — pre-existing, but
  this plan re-states the assumption. Options: (a) add a debug-mode
  diagnostic when a panel's command count crosses the default layer;
  (b) raise the default to 127 (max i8 headroom, halves the under-default
  artistic range); (c) document only, as today. Recommendation: (a) —
  cheap, fires exactly when the assumption breaks, no semantics change.
  **Decision: (a) debug diagnostic** — warn when a panel's render-command
  count reaches the default layer; no semantics change. *Superseded in
  part by Phase 5:* the diagnostic now checks the highest geometry draw
  slot rather than the render-command count — text and scissor commands no
  longer consume ordinals, so command count alone overstated the pressure
  on the default layer's headroom.
- **D3 — shared ordinal newtype** (minor, Type System lens; class:
  design-improvement; status: proposed). Problem: the layer (`i8`) and
  backing `command_index` (`usize`) are one ordering axis in two integer
  types; Phase 1/3 comparisons cross them, with sign-extension and
  swapped-axis hazards in test code. Options: (a) a shared `DrawOrdinal`
  newtype both convert into for comparisons; (b) keep two types, route all
  comparisons through one documented `i32` widening in the mapping module.
  Recommendation: (b) — backings stay non-authorable in this plan, so a
  shared public axis type buys little for the plumbing it touches.
  **Decision: (a) shared `DrawOrdinal` newtype** (user choice over the
  recommendation: newtypes self-document the axis). Internal
  (`pub(crate)`), defined beside the mapping in `render/constants.rs`;
  `TextDrawLayer` and backing `command_index` both convert into it, and the
  depth-bias / OIT-offset derivations take `DrawOrdinal` so the two sources
  share one code path.
- **D4 — runtime layer-override hotkey in the demo** (important, demo
  ergonomics lens; class: design-improvement; status: proposed). Problem:
  `examples/draw_layer.rs` demonstrates only the static authoring API
  (`TextStyle::with_draw_layer`); the public surface also has the runtime
  verbs `override_draw_layer` / `inherit_draw_layer` (Phase 3
  tests them, no example shows them; `cascade.rs` establishes the
  hotkey-toggle teaching pattern for `TextAlpha`). Options: (a) add a
  hotkey that drops the layer-72 run to layer 8 and back via the runtime
  verbs (title-bar control listed); (b) keep the demo static — the Phase 4
  spec bullet asks only for the artistic case. Recommendation: (a) — it
  completes the API surface the example teaches at small cost and shows
  the live re-batching working.
  **Decision: (a) add the hotkey**, amended during implementation (user
  choice between the two faithful variants): the toggle targets the
  *default-layer* run, not the layer-72 run. A tree-authored
  `with_draw_layer` materializes as `Override<TextDrawLayer>` on the run's
  label entities, so `inherit_draw_layer` cannot restore 72 — it
  resolves to the cascade default. The default run starts with no override
  and round-trips exactly: `K` overrides it to the body layer (it dims
  behind the shade) and inherits back. The run is authored with `text_id`
  and its wrapped lines are found by run id in `PanelTextLayout` (each
  line is its own label entity). See the inheritance discussion below.
- **D5 — layer-72 on-panel copy vs the OIT clamp** (important, demo
  ergonomics lens; class: design-improvement; status: proposed). Problem:
  the run reads "AUTHORED AT LAYER 72 - ABOVE ALL", but on the OIT view
  layers above the default clamp to the default's offset (D1) — the run
  ties with default text there; "above all" holds only on the sorted view.
  The caveat lives in the module doc and this plan, not on screen. Options:
  (a) amend the run's text (e.g. "AUTHORED AT LAYER 72 - TOP ON SORTED");
  (b) leave the copy — the run never composites *below* anything on either
  view, so the claim is not observably false. Recommendation: (b) — the
  visible behavior matches the words on both views; precision about the
  tie belongs in the docs, and the shorter line keeps the demo legible.
  **Decision: (b) leave the copy** — the on-panel line stays as authored;
  the OIT-tie caveat stays in the module doc and this plan.

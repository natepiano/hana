# Panel Text Rebuild — Flash Fix & Per-Run Hardening

## Status

| Item | State |
| --- | --- |
| Scheduler-ordering flash fix | **Done** (on `update/0.19.0-rc.2`) |
| Per-run rebuild (this plan) | **Planned** |
| Font-load relayout | **Deferred** — not reachable through the current API |

---

## 1. Background — the flash

**Symptom.** On Bevy `0.19.0-rc.2`, updating a value on the screen-space camera
control panel (e.g. the `zoom_to_fit` example: a control chip flips yellow on
orbit start, grey on orbit end) made the *entire* panel blink — all text
vanished for one frame on each transition. It did not happen on `main`
(Bevy `0.18`).

**Root cause.** The code is byte-identical between `main` and the branch (only
the mechanical `ShaderStorageBuffer` → `ShaderBuffer` etc. renames). The
regression came entirely from Bevy `0.19`'s scheduler.

`build_panel_text_meshes` (`render/panel_text/mesh_spawning.rs`) and
`world_text::render_world_text` spawn their glyph mesh entities via deferred
`Commands`, but declared **no ordering edge** to `PostUpdate`'s transform and
visibility propagation (`TransformSystems::Propagate`, `VisibilityPropagate`,
`CalculateBounds`, `CheckVisibility`). A freshly-spawned mesh entity needs its
`GlobalTransform`, `InheritedVisibility`, `Aabb`, and `ViewVisibility` computed
the same frame it appears, or it stays `HIDDEN`/untransformed for a frame.
`reset_view_visibility` also resets every entity's `ViewVisibility` to `HIDDEN`
at the start of each visibility pass.

- On `0.18` the scheduler happened to run the mesh-spawn systems *before* the
  propagation systems, so a new mesh became visible the same frame → the
  despawn-old / spawn-new swap was seamless.
- On `0.19` the reworked scheduler picked a different (equally valid)
  topological order — running the spawn *after* propagation. The new mesh
  missed that frame's transform + visibility pass while the old mesh was already
  despawned → one blank frame → the flash.

This is the exact class the `0.19` migration commit already flagged elsewhere:
*"make one latent system-ordering invariant explicit (exposed by the 0.19
scheduler)."* That commit pinned one case in `bevy_lagrange`; this one in
`bevy_diegetic` slipped through.

**Fix applied** (`render/panel_text/mod.rs`, the `PostUpdate` system block):

```rust
build_panel_text_meshes
    .after(shape_panel_text_children)
    .before(TransformSystems::Propagate),
world_text::render_world_text.before(TransformSystems::Propagate),
```

`Propagate` is the earliest of the four propagation passes, so one edge puts the
spawn ahead of all of them. The scheduler inserts a sync point, so the new mesh
entities exist and acquire transform + visibility the same frame the old ones
are despawned. Verified live: the flash is gone.

---

## 2. Why the flash was possible at all — per-panel rebuild churn

The ordering fix removes the *visible* flash. But the underlying reason the
panel was sensitive to ordering is that a single value change rebuilds **every**
text run on the panel — mesh entities, materials, and GPU buffers — not just the
one that changed.

Pipeline for one update:

1. `repaint_panel_display` / `rebind_panel_on_route_change` call `set_tree`
   → `ComputedDiegeticPanel` marked `Changed`.
2. `reconcile_panel_text_children` (`reconcile.rs:96-131`) reuses the
   `PanelChild` entities by `(element_idx, command_index)` — good — but
   **unconditionally re-inserts** `WorldText` / `WorldTextStyle` /
   `PanelTextLayout` on every child → all marked `Changed`.
3. `shape_panel_text_children` processes every changed child and re-inserts
   `PanelText` on all → all `Changed<PanelText>`.
4. `build_panel_text_meshes` (`mesh_spawning.rs:61-95`) marks the whole panel
   dirty if *any* child changed, then despawns **all** `DiegeticTextMesh` and
   respawns one per child — each with a fresh `Mesh` + three `ShaderBuffer`s
   (`curves`/`bands`/`glyphs`) + material.

The run storage key is a monotonic counter (`next_run_storage_key`,
`text/slug/runtime/backend.rs:187`), so `ensure_run_storage`'s cache is a
guaranteed miss on every rebuild — even unchanged text gets brand-new GPU
buffers re-uploaded each time.

So one chip color flip rebuilds every glyph mesh and every storage buffer on the
panel. That entity/buffer churn is the only reason the ordering hazard could
manifest as a whole-panel flash.

---

## 3. Planned change — rebuild per run, not per panel

Goal: a value change rebuilds only the run(s) that actually changed, leaving
every other run's mesh, material, and buffers untouched. This makes the flash
*structurally* impossible (not just ordering-dependent) and removes the
full-panel buffer re-upload on every update.

### Edit 1 — gate reconcile with conditional writes (`reconcile.rs`)

- Widen the `existing_children` query to read `&WorldText`, `&WorldTextStyle`,
  `&PanelTextLayout`.
- On a reused child, compare incoming vs current and write only when something
  differs:
  - `WorldText` — compare `.text()`.
  - `WorldTextStyle` (`= TextProps<ForStandalone>`) — has a manual `PartialEq`
    (`text_props.rs:224`).
  - `PanelTextLayout` — add `#[derive(PartialEq)]` (and to `BoundingBox` if it
    lacks one).
- Apply the same gate to the `Override<TextAlpha>` branch.

Effect: unchanged runs stay un-`Changed` → `shape_panel_text_children` skips
them → no `Changed<PanelText>`.

**Decision: change detection, not a dirty-flag marker.**
- The three components are written *only* by reconcile, so `Changed<>` on them is
  already precise once reconcile writes conditionally.
- The geometry-vs-appearance distinction is already encoded by two separate
  signals — `Changed<PanelText>` (glyph geometry) vs `Changed<Resolved<TextAlpha>>`
  (appearance). A single dirty flag would collapse the two into one bit; to
  recover the split you'd need two flags plus a clear-every-frame lifecycle.
- A flag would not save the component write reconcile must do anyway.

### Edit 2 — per-run mesh rebuild, reparent, and alpha short-circuit (`mesh_spawning.rs`)

- **Source tag + reparent.** Tag each `DiegeticTextMesh` with its source
  `PanelChild` entity and spawn it as a *child of that `PanelChild`* instead of
  the panel. When reconcile despawns a `PanelChild` (a row removed), Bevy's
  recursive despawn takes its mesh for free — no separate orphan-cleanup pass.
  Safe because:
  - `RenderLayers` is set explicitly on the mesh (not inherited).
  - Glyph positions are baked into the prepared run, not applied as a transform.
  - `PanelChild` carries only a default (identity) `Transform` and is a direct
    child of the panel, so the mesh's `GlobalTransform` is unchanged.
- **Drive the rebuild per run, split by change kind:**
  - `Changed<PanelText>` (text / layout / size / font changed) → despawn +
    respawn that run's mesh; new buffers + material; `remove_run_storage` its
    old key. *Geometry rebuild.*
  - `Changed<Resolved<TextAlpha>>` only, no `PanelText` change → keep the mesh
    entity and buffers; `materials.get_mut()` the run's `SlugTextMaterial` and
    update `alpha_mode` in place. *Appearance update — no mesh, no buffers.*

### Edit 2b — same alpha short-circuit on the world-text path

`render_world_text` (`render/world_text/mod.rs:36-40`) triggers on
`Or<(Changed<WorldText>, Changed<WorldTextStyle>, Changed<Resolved<TextAlpha>>,
Changed<Resolved<FontUnit>>)>` and rebuilds the full mesh + buffers — including
on an alpha-only change. Alpha only affects the material's `alpha_mode`; the
glyph mesh and the three buffers are alpha-independent. Apply the same
appearance-update short-circuit here.

The "despawn-all" pattern in the world-text path is otherwise left alone: each
world-text entity is typically a single run (one cube-face label), so there is
no multi-run partial flash like the panel has. The alpha rebuild is the only
real issue there.

### Edit 3 — image-children per-run + tint split, and a shared material builder (folded in via R9)

`reconcile_panel_image_children` (`reconcile.rs:152-260`) rebuilds every image
child's `Mesh` + `StandardMaterial` unconditionally on each panel rebuild, with
no tint-vs-geometry split — the same per-panel churn this plan removes for text.
Apply the parallel treatment:

- Gate the image reconcile with conditional writes (compare incoming `handle` /
  `tint` / bounds vs current), so an unchanged image isn't re-touched.
- Split rebuild by kind: bounds/handle change → rebuild the rectangle mesh +
  material; tint-only change → mutate `base_color` on the existing material in
  place (guarded like R5).
- Images carry no `SlugRunStorageKey`, so they need no run-storage cleanup; the
  R2 observer is text-only.

Separately, factor the duplicated `slug_text_material(...)` setup shared by
`panel_text/mesh_spawning.rs` and `world_text/mesh_spawning.rs` into one builder,
so the two paths can't drift.

### Decisions recorded
- Reparent under `PanelChild` (not source-tag + cleanup pass).
- Change detection (not a dirty flag).
- Include the world-text alpha short-circuit; leave its despawn-all otherwise.
- Per-run gating comparison uses a dedicated bit-equality `gating_eq`, not a
  derived/manual `PartialEq` (R1).
- Storage cleanup via an `On<Remove, DiegeticTextMesh>` observer; the
  panel-parent despawn loop is removed (R2).
- Geometry and alpha handled by two systems, not one branching loop (R3).
- No explicit `Entity` source-tag; locate meshes via `ChildOf` (R4).
- Alpha (and image tint) writes are value-guarded (R5).
- Images get the same per-run + tint-split treatment, and the text-material
  builder is shared (R9).

---

## 4. Why this is safe with respect to relayout

Measurement and `Fit` sizing happen in `compute_panel_layouts`
(`panel/compute_layout.rs:37`), in `Update`, in `PanelSystems::ComputeLayout` —
after `ApplyTreeChanges` (an `ApplyDeferred`) and before `ResolveWorldFit`. It
is one-directional: the text/render pipeline never writes `ComputedDiegeticPanel`
and never marks the panel `Changed` (verified — the only writers are in
`compute_layout.rs` plus a fixed-size screen set in `screen_space/mod.rs:482`).

What counts as "needs re-measure" is decided by `classify_content_change`
(`layout/element.rs:476`):

- text differs, or `!config.layout_eq_excluding_visuals(next)` → `LayoutAffecting`
  → full `engine.compute()` re-measure + relayout + re-fit.
- config differs only visually → `VisualOnly` → `regenerate_commands` (same
  positions, no re-measure, no resize).
- identical → skipped.

`layout_eq_excluding_visuals` (`text_props.rs:472`) treats `font_id`, `size`,
`weight`, `slant`, `line_height`, letter/word spacing, `wrap`, `align`,
`anchor`, `font_features`, `unit`, `world_scale` as metric fields; `color`,
`render_mode`, `shadow_mode`, `sidedness`, `alpha_mode` are render-only.

Consequence for the per-run change: a font change is `LayoutAffecting`, so it
changes both the style *and* the measured `bounds` on (typically) every run →
Edit 1's per-run comparison flags all of them → all rebuild with correct
geometry. The gating only skips runs whose text, style, and bounds are
byte-identical — i.e. genuinely unchanged. **Per-run reuse can never starve a
relayout.** Conversely, a sibling chip flipping color is `VisualOnly` → only
that chip's style changes → only it rebuilds (its material), siblings keep their
meshes.

---

## 5. Files touched & tests

**Files**
- `render/panel_text/reconcile.rs` — Edit 1 (text gating) + Edit 3 (image
  gating + tint split).
- `render/panel_text/mesh_spawning.rs` — Edit 2: two systems
  (`update_panel_text_geometry` / `update_panel_text_alpha`), reparent, drop the
  panel-parent despawn loop.
- `render/panel_text/mod.rs` — register the two systems and the
  `On<Remove, DiegeticTextMesh>` storage-cleanup observer; ordering/observer
  notes (M3, M5).
- `render/panel_text/layout.rs` + `layout/text_props.rs` — the `gating_eq`
  comparator (R1; **not** a derived `PartialEq`).
- `render/world_text/mesh_spawning.rs` + `mod.rs` — Edit 2b alpha short-circuit;
  the shared text-material builder (R9).

**Tests**
- reconcile: an unchanged run is not marked `Changed` across a rebuild.
- `gating_eq`: bit-equality matches `layout_eq_excluding_visuals` on metric
  fields; `unit`/`world_scale` changes do not flag a rebuild; `-0.0`/`+0.0`
  bounds are treated correctly.
- geometry vs alpha: an unchanged run's mesh entity is preserved while only the
  changed run's mesh is swapped; an alpha-only change preserves the mesh and
  buffers and updates `base.alpha_mode` in place; a no-op alpha resolution does
  not trip `Changed<SlugTextMaterial>`.
- storage cleanup: removing a `PanelChild` frees its run storage (via the
  remove observer).
- new row: a newly-inserted run has a non-identity `GlobalTransform` by the
  second frame (R6 regression).
- world text: an alpha-only change does not respawn the mesh.
- images: a tint-only change updates `base_color` in place without rebuilding
  the rectangle mesh.

---

## 6. Deferred — font-load relayout

**Finding.** Async font loads do **not** re-trigger panel layout/measurement.
`consume_loaded_fonts` (`text/mod.rs:88`) registers a loaded font and fires
`FontRegistered`, but that event has no internal observer (only a doc-comment
example). `compute_panel_layouts` recomputes only on `panel_ref.is_changed()` or
a pending tree change — a font load sets neither. Glyph generation downstream is
one-directional and feeds nothing back into layout, so glyphs regenerating with
correct outlines does not cause a re-fit; the `PendingGlyphs` retry covers
glyph generation, not measurement.

**Why it is not a bug today.** Fonts are referenced by a baked `font_id: u16`
(`TextProps.font_id`, set via `with_font(u16)`), and ids are assigned densely by
`register_font` at load time. A tree therefore cannot reference a font before
its data exists — so the "measured with a fallback, corrects later under the
same id" scenario is unreachable through the current API. The only async window
(`PendingGlyphs` backend glyph preprocessing) happens *after* the font data is
already registered, so measurement was already correct.

**When it becomes relevant.** Only if the engine gains *lazy* font references
(a handle or name that resolves after the id is already in the tree). At that
point: on `FontRegistered { id }`, mark only panels whose tree carries a text
element with `config.font_id == id`, invalidate those `ShapedTextCache`
measurement entries (the measure key includes `font_id`), and let
`compute_panel_layouts` re-measure + re-fit. Building this before the feature
exists would guard a state nothing can produce.

---

## 7. Priority

The ordering fix already removed the visible flash. The per-run change in §3 is
hardening plus a per-update perf win (no full-panel buffer re-upload on each
value change) — not a correctness fix. Worthwhile if panels are large or update
frequently; lower priority if updates are rare.

---

## 8. Review log (team_review, strengthen posture)

### Mechanical (auto-recorded)

- **M1 — Correct §3 line 113.** `BoundingBox` already derives `PartialEq`
  (`layout/geometry.rs:8`), so the parenthetical "(and to `BoundingBox` if it
  lacks one)" is moot. (Interacts with R1: the gating comparison must not use a
  derived `PartialEq` at all — see R1.)
- **M2 — State the depth-bias source.** The Geometry-mode text depth bias is
  derived from `command_index` (`mesh_spawning.rs:161-163`), which is the index
  in the render-command stream and stable across element reorder. Per-run
  rebuild therefore keeps layering correct; note this in §4.
- **M3 — Add `render/panel_text/mod.rs` to "Files touched."** §3 will gain an
  observer registration (R2) and an ordering/observer comment; the file belongs
  in the list.
- **M4 — Expand the test list (§5).** Add: storage-cleanup-on-despawn (removing
  a `PanelChild` frees its run storage); panel alpha-only change does not
  respawn the mesh; world-text alpha-only change does not respawn the mesh;
  newly-inserted row has a non-identity `GlobalTransform` by the second frame
  (the R6 regression test).
- **M5 — Note the seed/observer ordering (§4).** `seed_panel_child_alpha` fires
  on `Add<PanelChild>` during the reconcile command flush, after the label's
  `ChildOf` is inserted, so `Resolved<TextAlpha>` is seeded before
  `build_panel_text_meshes` reads it. Verified safe; state it so a future change
  doesn't break it silently.

### Proposed user decisions

Status legend: `proposed` (open) · `dropped`/`superseded` (kept for the record).
Cycle 2 reconciled every entry against the code; sharpenings folded in.

- **R1 — Gate on a dedicated bit-equality comparator, not derived/manual
  `PartialEq`.** Severity: **critical**. Dimension: types & changeability / risk.
  Class: design-improvement. (Cycle 2: confirmed by all three lenses; merges the
  separate "manual `PartialEq` over-compares" finding.)
  Problem: a derived `PartialEq` on `PanelTextLayout`/`BoundingBox` compares
  `f32`s with `==`, which mishandles `-0.0`/`+0.0` and NaN and diverges from the
  layout layer's own `to_bits()` comparison (`layout_eq_excluding_visuals`,
  `text_props.rs:472`). And `WorldTextStyle`'s manual `PartialEq`
  (`text_props.rs:224`) compares *more* than the layout decision — it includes
  `unit` and `world_scale`, which are render-context, not measurement inputs —
  so reusing it for gating false-rebuilds on changes the layout layer treats as
  no-ops.
  Impact: unsound gating → false skips (visible corruption) or false rebuilds
  (defeats the perf goal). This is the soundness keystone of Edit 1.
  Recommendation: add a dedicated `gating_eq` that (a) compares metric floats via
  `to_bits()`, mirroring `layout_eq_excluding_visuals`'s field set, plus the
  render fields the mesh actually depends on (`color`/`render_mode`/
  `shadow_mode`/`sidedness` + `PanelTextLayout` bounds/scale/anchor/clip), and
  (b) excludes `unit`/`world_scale`. Do not derive `PartialEq` on the
  float-bearing layout types for gating. Status: **accepted**.

- **R2 — Delete the panel-parent despawn loop; move run-storage cleanup to an
  `On<Remove, DiegeticTextMesh>` observer.** Severity: **critical**.
  Dimension: correctness / architecture. Class: design-improvement.
  (Cycle 2: confirmed; observer pattern has repo precedent —
  `on_stable_transparency_removed` in `screen_space/mod.rs`. Sequenced with R4.)
  Problem: `mesh_spawning.rs:61-68` finds meshes by
  `child_of.parent() == panel_entity` and calls `remove_run_storage` before
  despawn. After reparenting meshes under their `PanelChild` (R4), that loop
  matches nothing (storage leaks) and the cleanup is lost when Bevy recursively
  despawns the mesh.
  Impact: GPU storage-handle leak; the old loop silently no-ops.
  Recommendation: remove the loop. Add an observer
  `trigger: On<Remove, DiegeticTextMesh>` that reads `&SlugRunStorageKey` from
  `trigger.entity` (`On<Remove>` fires before the component is dropped, so the
  key is readable) and calls `backend.remove_run_storage(key)` via
  `ResMut<SlugBackend>`. **Sequence R4 then R2** — R4 (reparent) breaks the old
  loop, so the observer must land in the same change. Status: **accepted**.

- **R3 — Split build into two systems (`update_panel_text_geometry` /
  `update_panel_text_alpha`) rather than one branching loop.** Severity:
  **important**. Dimension: architecture. Class: design-improvement.
  (Cycle 2: **supersedes** the cycle-1 "per-entity branch in one loop" framing —
  two of three lenses preferred the split.)
  Problem: a single loop on `Or<(Changed<PanelText>, Changed<Resolved<TextAlpha>>)>`
  must test both flags per entity and do a secondary lookup of the mesh child's
  `MeshMaterial3d` to mutate alpha — error-prone and noisy.
  Impact: without a clean split the alpha short-circuit tends to regress into a
  full rebuild.
  Recommendation: two systems, each reading only what it needs and gated by its
  own filter:
  - `update_panel_text_geometry`: `Changed<PanelText>` → despawn old mesh, spawn
    new (storage cleanup via R2's observer).
  - `update_panel_text_alpha`: `Changed<Resolved<TextAlpha>>` without
    `Changed<PanelText>` → query the run's `MeshMaterial3d<SlugTextMaterial>` and
    update alpha in place (R5).
  This also mirrors how the world-text path can be structured for Edit 2b.
  Status: **accepted**.

- **R4 — Drop the bare `Entity` source-tag; rely on `ChildOf`.** Severity:
  **important**. Dimension: types & changeability. Class: design-improvement.
  (Cycle 2: confirmed by all; depends-on/sequences-with R2.)
  Problem: tagging each `DiegeticTextMesh` with its source `PanelChild` `Entity`
  duplicates the `ChildOf` link created by reparenting and can go stale.
  Impact: redundant state + stale-entity bug surface.
  Recommendation: omit the tag; locate a run's mesh via `ChildOf`
  (`Query<(Entity, &ChildOf), With<DiegeticTextMesh>>` filtered by parent).
  Status: **accepted**.

- **R5 — Make the in-place alpha write idempotent on `material.base.alpha_mode`.**
  Severity: **important**. Dimension: implementation quality.
  Class: design-improvement. (Cycle 2: confirmed; field path + `AlphaMode:
  PartialEq` verified.)
  Problem: `SlugTextMaterial = ExtendedMaterial<StandardMaterial, TextExtension>`;
  alpha lives at `material.base.alpha_mode`. Writing it unconditionally trips
  `Changed<SlugTextMaterial>` and re-prep even when the resolved alpha is
  unchanged. `AlphaMode` implements `PartialEq`, so a guard is sound.
  Impact: partially defeats the short-circuit.
  Recommendation: `if material.base.alpha_mode != resolved { material.base.alpha_mode = resolved; }`
  (or a `set_if_neq`-style guard). Status: **accepted**.

- **R6 — (superseded) New-row transform/visibility timing.** Severity: minor.
  Dimension: risk. Class: design-improvement.
  Resolution: cycle 2 confirmed the chain
  `reconcile → shape_panel_text_children → build_panel_text_meshes
  (.before(TransformSystems::Propagate))` already inserts the needed sync point,
  so a newly-spawned `PanelChild` is flushed before build parents a mesh under
  it. No code change. **Superseded** — folded into M4 as a regression test
  (non-identity `GlobalTransform` by frame 2).

- **R7 — Document the reuse-key reorder limitation.** Severity: **minor**.
  Dimension: correctness. Class: design-improvement. (Cycle 2: confirmed.)
  Problem: reconcile reuses children by `(element_idx, command_index)`
  (`reconcile.rs:64-72`). Row reorder invalidates keys → unchanged-text runs
  respawn. Reuse is layout-stability-stable, not content-stable.
  Impact: no correctness bug; the perf guarantee doesn't hold across reorders.
  Recommendation: note in §3 (and that command-index order is assumed stable for
  a static layout). Status: **accepted**.

- **R8 — Doc completeness.** Severity: **minor**. Dimension: doc.
  Class: design-improvement.
  Resolution: the concrete items are deterministic and were **auto-recorded as
  M3-M5** (files-touched, test list, observer note). The one judgment item —
  whether to factor a shared text-material builder across the panel and
  world-text paths — moves to R9. **Superseded** by M3-M5 + R9.

- **R9 — (new) Same rebuild churn exists in the image-children path; and the
  panel/world-text material builders are duplicated.** Severity: **important**,
  but **out of this plan's stated intent** (panel *text*). Dimension:
  architecture. Class: design-improvement (adjacent follow-up, not part of the
  text plan).
  Problem: `reconcile_panel_image_children` (`reconcile.rs:152-260`) rebuilds
  every image child's `Mesh` + `StandardMaterial` unconditionally on each panel
  rebuild and has no geometry-vs-appearance (tint) split — the same per-panel
  churn this plan removes for text. Separately, `panel_text/mesh_spawning.rs`
  and `world_text/mesh_spawning.rs` duplicate the `slug_text_material(...)` setup.
  Impact: images remain a rebuild hot-spot; duplicated builders can drift.
  Recommendation: record as a follow-up (apply the same gating + per-run reuse +
  appearance split to images; factor a shared material builder).
  Status: **accepted — folded into the plan** as Edit 3 (images) and the shared
  text-material builder; see §3 and §5.

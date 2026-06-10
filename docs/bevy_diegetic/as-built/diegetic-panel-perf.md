# Panel Text Rebuild — Flash Fix & Per-Run Hardening

## Status

| Item | State |
| --- | --- |
| Scheduler-ordering flash fix | **Done** (on `update/0.19.0-rc.2`) |
| Per-run rebuild (this plan) | **Done** — all phases landed (P2 was already satisfied) |
| Font-load relayout | **Deferred** — not reachable through the current API |

> **Archived 2026-06-06.** Every implementation phase landed. The per-run
> runtime machinery built in Phases 4–6 — the `DiegeticTextMesh` reparent and
> storage observers, the `update_panel_text_geometry` / `update_panel_text_alpha`
> split, and the world-text mirror (`render_world_text` / `WorldTextMesh` /
> `update_world_text_alpha`) — was later replaced by the batched-records path
> ([`glyph-instancing.md`](glyph-instancing.md)); its Step 4b deleted the
> per-run mesh path on 2026-06-06. Phases 1, 3, and 7 — the `gating_eq`
> comparator, the conditional reconcile writes, and the image gating + tint
> split — remain live code and supply the change detection the batching path
> consumes. §6 (font-load relayout) is still correctly deferred:
> `FontRegistered` has no internal observer and fonts are still referenced by
> baked id. §10 (runtime settings ergonomics) was a separate track; since this
> doc was written, `override_text_alpha` / `inherit_text_alpha` /
> `override_font_unit` and `TextProps::set_alpha_mode` landed, closing gap 2
> and part of gap 1; gap 3 (spawn-only `TextStyle.alpha_mode` / `.unit`) is
> still open.

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
   **unconditionally re-inserts** `WorldText` / `TextStyle` /
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

- Widen the `existing_children` query to read `&WorldText`, `&TextStyle`,
  `&PanelTextLayout`.
- On a reused child, compare incoming vs current and write only when something
  differs:
  - `WorldText` — compare `.text()`.
  - `TextStyle` (`= TextProps<ForStandalone>`) — has a manual `PartialEq`
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
    entity and buffers; `materials.get_mut()` the run's `TextMaterial` and
    update `alpha_mode` in place. *Appearance update — no mesh, no buffers.*

### Edit 2b — same alpha short-circuit on the world-text path

`render_world_text` (`render/world_text/mod.rs:36-40`) triggers on
`Or<(Changed<WorldText>, Changed<TextStyle>, Changed<Resolved<TextAlpha>>,
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
- Images carry no `RunStorageKey`, so they need no run-storage cleanup; the
  R2 observer is text-only.

Separately, factor the duplicated `text_material(...)` setup shared by
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
  not trip `Changed<TextMaterial>`.
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
  on `Add<PanelTextChild>` during the reconcile command flush, after the label's
  `ChildOf` is inserted, so `Resolved<TextAlpha>` is seeded before
  `build_panel_text_meshes` reads it. Verified safe; state it so a future change
  doesn't break it silently.
- **M6 — `gating_eq` spans three components (sharpens R1).** The comparator
  covers `WorldText` (`.text()`), `TextStyle` (metric fields via `to_bits`,
  excluding `unit`/`world_scale`), **and** `PanelTextLayout`'s `bounds`/`scale_x`/
  `scale_y`/`anchor_x`/`anchor_y`/`clip_rect` (floats via `to_bits`).
  `command_index` is part of the reuse key `(element_idx, command_index)`, so it
  is constant within a reused slot and is *not* part of the comparator.
- **M7 — Alpha-system gating mechanism + no inter-system ordering (corrects
  cycle-1 wording; sharpens R3).** `Without<Changed<PanelText>>` is **not**
  expressible — `Without<T>` takes a component, `Changed<T>` is a query filter.
  `update_panel_text_alpha` instead queries `Changed<Resolved<TextAlpha>>` and
  reads `Ref<PanelText>`, skipping the run when `panel_text.is_changed()`. With
  that skip, the both-changed case is handled in *any* run order (the geometry
  system rebuilds with correct alpha), so no explicit edge between the two
  systems is required.
- **M8 — Images need no reparenting (sharpens Edit 3).**
  `reconcile_panel_image_children` already reuses children by `element_idx` and
  despawns orphans synchronously (`reconcile.rs:252-258`), so per-element reuse
  needs no reparent and no remove-observer. Image gating compares *inputs*
  (`handle`/`tint`/`bounds`) — not the `StandardMaterial`, so the
  `classify_material_change` caveat (`element.rs:464`) does not apply — and
  compares `bounds` via `to_bits` for consistency with text gating.
- **M9 — Shared text-material builder signature (sharpens R9).**
  `build_text_material(base, alpha_mode, fill_color, render_mode, curves, bands,
  glyphs) -> TextMaterial`; callers set `depth_bias`/`sidedness` on `base`
  first. Place it in the `text` module so both `panel_text` and `world_text`
  import it without a new cross-module coupling.

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
  `text_props.rs:472`). And `TextStyle`'s manual `PartialEq`
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
  `trigger: On<Remove, DiegeticTextMesh>` that reads `&RunStorageKey` from
  `trigger.entity` (`On<Remove>` fires before the component is dropped, so the
  key is readable) and calls `glyph_cache.remove_run_storage(key)` via
  `ResMut<GlyphCache>`. **Sequence R4 then R2** — R4 (reparent) breaks the old
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
    `Changed<PanelText>` → query the run's `MeshMaterial3d<TextMaterial>` and
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
  Problem: `TextMaterial = ExtendedMaterial<StandardMaterial, TextExtension>`;
  alpha lives at `material.base.alpha_mode`. Writing it unconditionally trips
  `Changed<TextMaterial>` and re-prep even when the resolved alpha is
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
  and `world_text/mesh_spawning.rs` duplicate the `text_material(...)` setup.
  Impact: images remain a rebuild hot-spot; duplicated builders can drift.
  Recommendation: record as a follow-up (apply the same gating + per-run reuse +
  appearance split to images; factor a shared material builder).
  Status: **accepted — folded into the plan** as Edit 3 (images) and the shared
  text-material builder; see §3 and §5.

- **R10 — (new, review 2) The geometry system must handle `PanelText`
  *removal*, not just `Changed`.** Severity: **important**. Dimension:
  correctness. Class: design-improvement.
  Problem: when a run's text goes empty, `shape_panel_text_children` calls
  `clear_panel_text_output`, which **removes** `PanelText` (`shaping.rs:120-125`).
  `Changed<PanelText>` does not fire on component removal, so a geometry system
  gated only on `Changed<PanelText>` (R3) would leave the emptied run's mesh in
  place — a stale glyph. The current monolithic `build_panel_text_meshes` avoids
  this because it despawns all of a dirty panel's meshes and then respawns only
  the children that still carry `PanelText`; the two-system split loses that
  coverage.
  Impact: stale mesh on a run whose text empties (regression vs. current code).
  Recommendation: have `update_panel_text_geometry` also react to
  `RemovedComponents<PanelText>` (despawn that run's mesh), or add an
  `On<Remove, PanelText>` observer that despawns the run's `DiegeticTextMesh`
  child. Either composes with R2's storage-cleanup observer.
  Status: **accepted**.

---

## 9. Implementation phases (commit sequence)

Each phase is one commit: dependency-ordered, independently buildable and
testable. The §3 Edits and §8 decisions (R/M) each map to exactly one phase.
Dependency spine: P1 → P3 (gating_eq before reconcile uses it); P4 before P5
(reparent before the split locates meshes via `ChildOf`). (Phase 2, the shared
material builder, is already satisfied in this branch — see below.)

### Phase 1 — `gating_eq` comparator (R1, M6) — Done
Pure addition, no behavior change (nothing calls it yet).
- Add a bit-equality `gating_eq` spanning `WorldText.text()`, `TextStyle`
  metric fields (via `to_bits`, excluding `unit`/`world_scale`), and
  `PanelTextLayout` `bounds`/`scale_x`/`scale_y`/`anchor_x`/`anchor_y`/`clip_rect`
  (via `to_bits`). Not a derived/manual `PartialEq`.
- Files: `layout/text_props.rs`, `render/panel_text/layout.rs`.
- Tests: matches `layout_eq_excluding_visuals` on metric fields; `unit`/
  `world_scale` changes don't flag; `-0.0`/`+0.0` treated correctly.

#### Retrospective

**What worked:**
- Two inherent methods, no trait: `TextProps::<C>::gating_eq` (`text_props.rs`,
  `pub(crate)`) and `PanelTextLayout::gating_eq` (`layout.rs`, `pub(super)`) +
  a private `bbox_bits` helper. 11 tests green.
- Tests prove the R1 keystones directly: `gating_eq_ignores_unit` asserts
  `layout_eq_excluding_visuals` returns `false` while `gating_eq` returns `true`
  for a unit-only difference; `gating_eq_distinguishes_signed_zero` proves
  `to_bits` (not `==`); `gating_eq_detects_color_change` shows the render-field
  inclusion diverging from `layout_eq_excluding_visuals`.

**What deviated from the plan:**
- `WorldText` got **no** `gating_eq` method — its `.text()` comparison is
  trivial and stays inline in reconcile (Phase 3). This matches the planned
  two-file scope (`text_props.rs`, `layout.rs`); the doc's M6 phrasing ("spans
  three components") describes the *combined* reconcile gate, not three methods.
- `TextProps::gating_eq` explicitly **excludes `alpha_mode`** (the R1 included-set
  named only `color`/`render_mode`/`shadow_mode`/`sidedness`). Rationale: alpha
  is gated separately through `Override<TextAlpha>` (Edit 2 / Phase 5). This puts
  a hard requirement on Phase 3 — see Implications.

**Surprises:**
- `dead_code` warnings (3): both `gating_eq` methods and `bbox_bits` have no
  non-test caller until Phase 3. `cargo build` warns; `cargo nextest` does not
  (tests exercise them). Open decision: accept the transient warnings on this
  commit vs. a reviewed `#[allow(dead_code, reason = …)]` removed in Phase 3 vs.
  fold Phase 1 into Phase 3.

**Implications for remaining phases:**
- **Phase 3** clears the `dead_code` warnings by calling all three: `WorldText`
  `.text()` (inline), `style.gating_eq`, `layout.gating_eq`.
- **Phase 3** must gate the `Override<TextAlpha>` branch on its own
  (alpha comparison), because `style.gating_eq` deliberately ignores
  `alpha_mode`; otherwise an alpha-only style change would be skipped and never
  reach the Phase 5 alpha short-circuit.

#### Phase 1 Review

Architect re-review of the remaining phases against the shipped Phase 1 code.
Edits applied:
- **Phase 2 — dropped** (user decision): the shared material builder already
  exists in this branch (`text_material` / `text_material`) and both paths
  call it; marked "Already satisfied," removed from the dependency spine.
- **Phase 6 — expanded** (user decision): re-scoped from a one-line guard to
  Phase 5's two-signal split + a query for the existing `WorldTextMesh` child's
  material, since `render_world_text` has no per-entity alpha branch.
- **Phase 3** — widen the `existing_children` query and split the single
  unconditional component-tuple insert into per-component conditional writes;
  the `Override<TextAlpha>` insert/remove (unconditional today) must also become
  value-conditional, else the per-run alpha win is lost.
- **Phase 4** — noted that labels *and* meshes are flat children of the panel
  today (located via `child_of.parent() == panel_entity`), so the reparent must
  also update `build_panel_text_meshes` / `collect_dirty_panels` parent-matching.
- **Phase 5** — R10's emptied-run path must *despawn* the mesh (firing Phase 4's
  `On<Remove, DiegeticTextMesh>` observer) to free run storage, not clear in
  place.
- **Phase 7** — `PanelImageChild` carries only `element_idx` today; gating on
  inputs (M8) requires caching prior `handle`/`tint`/`bounds` and widening the
  query to hold the material handle.

No findings rejected. One confirmation finding (depth bias from `command_index`
is safe under the `gating_eq` reuse-key exclusion) needed no change.

### Phase 2 — shared text-material builder (R9 builder part, M9) — Already satisfied (no commit)
The shared builder already exists in this branch and both paths route through
it, so there is nothing to build. `text_material(TextMaterialInput { base,
fill_color, render_mode, curves, bands, glyphs })` lives in
`text/slug/render/material.rs` (re-exported as `text_material` /
`TextMaterialInput`, `text/mod.rs:42-43`); `panel_text/mesh_spawning.rs:200`
and `world_text/mesh_spawning.rs:116` both call it, with `alpha_mode` set on
`base` by the caller — the M9 design, already in place. No Phase 2 commit.
Phase numbers below are left stable to avoid churning the cross-references.

### Phase 3 — gate reconcile (Edit 1, R7) — Done
- `reconcile_panel_text_children` reads the existing `WorldText`/`TextStyle`/
  `PanelTextLayout` and writes only when `gating_eq` differs; same gate on the
  `Override<TextAlpha>` branch. Note the `(element_idx, command_index)` reorder
  limitation in a comment (R7).
- Widen the query: `existing_children` is today `Query<(Entity, &PanelTextLayout,
  &ChildOf)>` (`reconcile.rs:25`) and the reused-child path re-inserts
  `(WorldText, style, panel_text_child, PanelChild)` as **one unconditional
  tuple** (`reconcile.rs:96-101`). Add `&WorldText`/`&TextStyle` to the
  query and split that tuple into conditional per-component writes: `WorldText`
  by `.text()`, `TextStyle` via `gating_eq`, `PanelTextLayout` via
  `gating_eq`.
- **Make the alpha write conditional (required, not optional).** The
  `Override<TextAlpha>` insert/remove (`reconcile.rs:102-118`) runs
  unconditionally on every reused child today. Because `gating_eq` excludes
  `alpha_mode` (Phase 1), this insert/remove must compare incoming `label_alpha`
  against the current `Override<TextAlpha>` and write only on change — otherwise
  every run fires `Changed<Resolved<TextAlpha>>` on every rebuild and Phase 5's
  alpha system processes all runs, silently losing the per-run win.
- Files: `render/panel_text/reconcile.rs`.
- Tests: an unchanged run is not marked `Changed` across a rebuild; an
  alpha-unchanged reused child does not re-touch `Override<TextAlpha>`.

#### Retrospective

**What worked:**
- The widened `existing_children` query + a private `ReusableChild<'a>` snapshot
  struct (holds `&WorldText`/`&TextStyle`/`&PanelTextLayout`/
  `Option<&Override<TextAlpha>>` borrowed for one pass) let each reused child be
  compared and written component-by-component. The single unconditional
  `(WorldText, style, panel_text_child, PanelChild)` tuple insert is now four
  guarded writes; `PanelChild` is dropped from the reused path (already present,
  never changes).
- Both Phase-1 `gating_eq` methods now have a caller (`reusable.style.gating_eq`,
  `reusable.layout.gating_eq`) — the 3 `dead_code` warnings are gone.
- Two integration tests over a real `HeadlessLayoutPlugin` app prove the gate
  with a `Ref`-based change probe chained after reconcile's command flush:
  `unchanged_run_is_not_rewritten_across_a_visual_only_rebuild` (recolor one of
  two runs → only its `TextStyle` is `Changed`; the byte-identical sibling
  is untouched) and `alpha_unchanged_reused_child_keeps_its_override` (recolor a
  label whose alpha is unchanged → `TextStyle` rewritten, `Override<TextAlpha>`
  not re-touched). Full suite 181 green.

**What deviated from the plan:**
- The alpha gate compares the inner `TextAlpha`, not `Override<TextAlpha>`:
  `Override<A>` derives no `PartialEq` (only `Clone/Copy/Debug/Reflect`), while
  `TextAlpha(pub AlphaMode)` does. The guard is
  `reusable.alpha.map(|o| o.0) != Some(TextAlpha(alpha_mode))`.
- `WorldText` is compared by `.text()` (a `&str`) inline as planned — no method
  added; matches the Phase-1 decision that `WorldText` got no `gating_eq`.

**Surprises:**
- `TextStyle::gating_eq` excludes `alpha_mode` (Phase-1 decision), so on an
  alpha-only style change the reused `TextStyle` component keeps its stale
  `alpha_mode` field — by design: panel-label alpha rides the cascade
  (`Override`/`Resolved<TextAlpha>`), not `TextStyle.alpha_mode`. Nothing
  downstream reads that field for panel children, so the staleness is inert.
- The change probe relies on a tick-ordering fact: reconcile's `with_child`
  spawns flush at the chained `ApplyDeferred` *before* the probe runs, so an
  unchanged component's change tick is older than the probe's prior run and
  reads `is_changed() == false` on the next frame. This is exactly the signal
  Phase 5's geometry/alpha split consumes (`Changed<PanelText>` vs
  `Changed<Resolved<TextAlpha>>`).

**Implications for remaining phases:**
- **Phase 5** can now trust that `Changed<PanelText>` fires only on a genuine
  geometry change and `Changed<Resolved<TextAlpha>>` only on a genuine alpha
  change — the per-run win the split depends on is real, not theoretical.
- **Phase 4** is unaffected: reconcile still spawns labels as flat children of
  the panel via `with_child` (the reused-child path never reparents), so the
  flat-parentage assumption Phase 4 must change is intact.

#### Phase 3 Review

Architect re-review of Phases 4–7 against the shipped Phase 3 code. Edits
applied:
- **Phase 4** — added the storage-cleanup invariant (the
  `On<Remove, DiegeticTextMesh>` observer is the *sole* run-storage freer; no
  inline `remove_run_storage` survives) and a note that screen-space layer
  propagation now descends through the reparented `PanelChild` label.
- **Phase 5** — named the two-hop material lookup (the alpha system reaches
  `MeshMaterial3d<TextMaterial>` on the mesh *child* via `ChildOf`, not on the
  `PanelChild`); confirmed the both-changed `Ref<PanelText>::is_changed()` skip
  is sound given Phase 3's conditional writes; flagged that the R6
  `GlobalTransform` test needs the propagation pass, not Phase 3's
  bare-`ApplyDeferred` probe; required the R10 `RemovedComponents<PanelText>`
  reaction to be idempotent against recursive `PanelChild` despawn; recorded the
  perf-stat split (geometry system owns `mesh_build_ms`).
- **Phase 6** — kept narrow (user decision): for standalone world text the only
  runtime alpha signal is the global-default `Changed<Resolved<TextAlpha>>`;
  `TextStyle.alpha_mode` is spawn-only and opacity rides with `color`.
  Phase 6 left as scoped, with a note added; the broader per-entity runtime
  blend-mode gap is split off to §10.
- **Phase 7** — clarified that only `bounds` compares via `to_bits`; `tint`
  (a `Color`) and `handle` use their own equality.

One confirmation finding (Phase 4's "monolithic build stays, adapted to new
parentage" premise is intact) needed no change. Runtime settings ergonomics was
raised as a gap and split to its own track (§10) at the user's direction — not
folded into any perf phase.

### Phase 4 — reparent text meshes + storage observer (R4, R2, M5) — Done
- Spawn each `DiegeticTextMesh` as a child of its `PanelChild` (not the panel);
  drop the `Entity` source-tag, locate meshes via `ChildOf`. Remove the
  panel-parent despawn loop. Add the `On<Remove, DiegeticTextMesh>` observer
  that frees run storage. The monolithic build stays, adapted to the new
  parentage. Document the `seed_panel_child_alpha` ordering (M5).
- Current parentage to change: today both labels and meshes are **flat children
  of the panel** — labels via `with_child` on `panel_entity` (`reconcile.rs`)
  and meshes under `panel_entity` (`mesh_spawning.rs`), located via
  `child_of.parent() == panel_entity`. Reparenting the mesh under its
  `PanelChild` therefore also requires updating the mesh parent-matching in
  `build_panel_text_meshes` / `collect_dirty_panels`, which currently key off
  `panel_entity`.
- **Storage-cleanup invariant (governs Phase 4↔5).** Once the
  `On<Remove, DiegeticTextMesh>` observer owns run-storage freeing, it is the
  *sole* path: no inline `remove_run_storage` survives in the geometry system or
  anywhere else. All three despawn triggers free storage only through the
  observer — (a) reconcile despawning a removed `PanelChild` (recursive despawn
  takes the mesh), (b) Phase 5's geometry rebuild on `Changed<PanelText>`,
  (c) Phase 5's R10 emptied-run despawn. A second inline cleanup would double-free;
  a missing observer would leak.
- **Screen-space layer propagation now descends one level deeper.**
  `propagate_screen_space_render_layers` / `propagate_layers_recursive`
  (`screen_space/mod.rs:287-330`) inserts `RenderLayers` on any panel child
  missing the component and recurses into grandchildren. After the mesh moves
  under its `PanelChild`, the recursion still reaches the mesh, but it now also
  inserts a layer on the intervening `PanelChild` label (previously a leaf).
  Benign — labels carry no mesh — but verify it does not perturb the mesh's
  explicit content-layer assignment during Phase 4.
- Files: `render/panel_text/mesh_spawning.rs`, `render/panel_text/mod.rs`.
- Tests: removing a `PanelChild` frees its run storage; whole-panel despawn
  cleans up; rendering unchanged.

#### Retrospective

**What worked:**
- Reparent by renaming `PanelTextSpawnRequest.panel_entity` → `mesh_parent` and
  spawning the mesh with `commands.entity(label).with_child(...)`. The
  `On<Remove, DiegeticTextMesh>` observer `free_run_storage_on_mesh_removal`
  reads `&RunStorageKey` off the triggering entity (`On<Remove>` fires before
  the drop) and is the sole storage freer — the inline `remove_run_storage` and
  the `Option<&RunStorageKey>` on the `old_meshes` query are both gone.
- A `despawn_label_mesh(label, &old_meshes, commands)` helper locates a run's
  mesh by `ChildOf` (`Query<(Entity, &ChildOf), With<DiegeticTextMesh>>`) — the
  same lookup Phase 5 reuses.

**What deviated from the plan:**
- The plan kept the "monolithic build, adapted to new parentage" for Phase 4.
  The adaptation was larger than a parent swap: the old despawn loop matched
  meshes by `child_of.parent() == panel_entity`, which matches *nothing* once
  meshes sit under labels. The loop moved *inside* the per-label loop
  (despawn this label's mesh, then respawn), so the panel-level mesh sweep is
  gone even before Phase 5.

**Surprises:**
- Screen-space layer propagation (`propagate_layers_recursive`) now inserts a
  `RenderLayers` on the intervening `PanelChild` label (previously a leaf) and
  still recurses to the mesh, which keeps its explicit content layer
  (`existing_layers.get(child).is_err()` is false for the mesh). Benign — labels
  carry no mesh — and confirmed in test.
- `render/world_text/panel_child.rs` is an orphan file (no `mod panel_child;`
  declaration) duplicating the `PanelChild` definition that actually lives in
  `world_text/mod.rs:160`. Not compiled; left untouched.

**Implications for remaining phases:**
- **Phase 5** reaches a run's material two hops down (label → mesh child) via the
  `despawn_label_mesh` lookup, and frees storage only through the observer.
- **Phase 6/7** are unaffected by the reparent (world text is single-run; images
  keep flat per-element reuse).

### Phase 5 — split into geometry + alpha systems (Edit 2, R3, R5, R10, M2, M7, R6) — Done
The core per-run change.
- Replace the monolithic build with `update_panel_text_geometry`
  (`Changed<PanelText>` **and** `RemovedComponents<PanelText>` [R10] → despawn +
  respawn that run's mesh) and `update_panel_text_alpha`
  (`Changed<Resolved<TextAlpha>>`, skip via `Ref<PanelText>::is_changed()` [M7],
  value-guarded `material.base.alpha_mode` write [R5]). Register both in
  `mod.rs`, `.before(TransformSystems::Propagate)`. Depth bias still from
  `command_index` (M2).
- **Alpha system reaches the material two hops down (after Phase 4 reparent).**
  `PanelText`/`Resolved<TextAlpha>` live on the `PanelChild`, but after Phase 4
  the `MeshMaterial3d<TextMaterial>` lives on the `DiegeticTextMesh` *child* of
  that `PanelChild`. So `update_panel_text_alpha` iterates `PanelChild` entities
  (`Changed<Resolved<TextAlpha>>` + `Ref<PanelText>`), then resolves the mesh
  child's material via `ChildOf` — the same
  `Query<(Entity, &ChildOf), With<DiegeticTextMesh>>` lookup Phase 4 introduces.
  M7's two-system framing ("query the run's `MeshMaterial3d`") predates the
  reparent; the material is not on the `PanelChild` itself.
- **Both-changed coordination is sound (M7), now confirmed by Phase 3.** When a
  run changes both geometry and alpha, the geometry system rebuilds with correct
  alpha and the alpha system skips via `Ref<PanelText>::is_changed()`. There is
  no inter-system ordering edge, so the alpha system may run after geometry has
  despawned the old mesh — but the `is_changed()` skip prevents any `get_mut` on
  the despawned material handle. This holds only because Phase 3's conditional
  writes guarantee `Changed<PanelText>` fires on a genuine geometry change.
- **Perf stats split — geometry owns the timer.** `build_panel_text_meshes`
  today brackets the whole pass and writes `perf.panel_text.mesh_build_ms` +
  `total_ms` (`mesh_spawning.rs:95-97`), with `shape_panel_text_children`
  setting `total_ms = shape_ms + mesh_build_ms` (`shaping.rs:109`). After the
  split, `update_panel_text_geometry` owns `mesh_build_ms` (it does the mesh
  rebuild); `update_panel_text_alpha` touches no mesh and adds nothing to it.
  `total_ms = shape_ms + mesh_build_ms` is unchanged, so an alpha-only frame
  reports ~0 mesh-build time — which is accurate.
- Files: `render/panel_text/mesh_spawning.rs`, `render/panel_text/mod.rs`.
- Tests: unchanged run's mesh preserved while only the changed run swaps;
  alpha-only change preserves mesh + buffers and updates `base.alpha_mode`
  in place; no-op alpha resolution doesn't trip `Changed<TextMaterial>`;
  an emptied run despawns its mesh; a newly-inserted run has a non-identity
  `GlobalTransform` by the second frame (R6).
- **Test discipline differs from Phase 3's reconcile tests.** Phase 3's probe
  chains after a bare `ApplyDeferred` because it only asserts component
  change-ticks. The R6 `GlobalTransform` test asserts a *propagated* transform,
  so both split systems must keep `.before(TransformSystems::Propagate)` and the
  test must observe after the propagation pass — not after a bare flush.
- **R10 path must despawn, not clear-in-place.** When text empties,
  `shape_panel_text_children` removes `PanelText` (`shaping.rs:120-125`). The
  geometry system's `RemovedComponents<PanelText>` reaction must **despawn that
  run's mesh entity** so Phase 4's `On<Remove, DiegeticTextMesh>` observer frees
  its run storage; clearing geometry in place would leak storage. (Depends on
  Phase 4's observer.) The reaction must be **idempotent**: recursive despawn of
  a removed `PanelChild` (Phase 4) *also* fires `RemovedComponents<PanelText>`
  for the same frame, so the handler must tolerate a mesh whose `PanelChild`
  parent is already gone (filter on a still-present parent, or no-op a missing
  mesh) rather than assume the signal only arrives on the clear-in-place path.

#### Retrospective

**What worked:**
- `update_panel_text_geometry` (`Changed<PanelText>` + `RemovedComponents<PanelText>`)
  and `update_panel_text_alpha` (`Changed<Resolved<TextAlpha>>`, `Ref<PanelText>`
  skip, value-guarded `material.base.alpha_mode`) replace the monolithic build;
  `collect_dirty_panels` is deleted. The geometry system owns the perf timer; an
  alpha-only frame reports ~0 `mesh_build_ms`.
- Idempotent R10: both the emptied-run path and a recursively despawned label
  route through `despawn_label_mesh`, which finds nothing when the mesh is
  already gone — no double-despawn.
- Eight integration tests over a real-font headless pipeline
  (`MinimalPlugins` + `AssetPlugin` + `TransformPlugin` + `FontRegistry::new()`,
  manual asset/resource init, no `MaterialPlugin`): per-run rebuild isolation,
  alpha-in-place, storage cleanup (label + whole panel), R10 emptied-run, R6
  propagated `GlobalTransform`, idle-frame no-op. A `#[cfg(test)]`
  `GlyphCache::run_storage_len` exposes the storage count.

**What deviated from the plan:**
- `Assets::get_mut` binds by value in this Bevy (`let Some(mut material) = …`),
  not as a `&mut`; the R5 value-guard still wraps the write. Calling `get_mut`
  marks the asset modified regardless, so the "no-op alpha doesn't trip
  `Changed<TextMaterial>`" guarantee actually comes from the upstream cascade
  guard (`propagate_cascade` only inserts `Resolved` on a real change, so this
  system never runs on a no-op), not from the in-system field guard.

**Surprises:**
- A recolor is a *geometry* rebuild, not an alpha update: `fill_color` rides in
  `PanelText`/the material, so `Changed<PanelText>` fires. Genuine alpha-only
  changes arrive only as `Changed<Resolved<TextAlpha>>` (global default or panel
  override) — exactly the narrow standalone story Phase 6 is scoped to. The
  alpha-only test drives it through `CascadeDefault<TextAlpha>`.
- Slug shaping is synchronous, so glyphs are `Ready` in a headless test with no
  render device — the full pipeline produces meshes without `MaterialPlugin`.

**Implications for remaining phases:**
- **Phase 6** mirrors this two-system split for world text (single-run, no
  reparent); its alpha-only signal is likewise only `Changed<Resolved<TextAlpha>>`
  from the global default (§10 records the missing per-entity runtime path).
- **Phase 7** (images) is independent of this split — no `PanelText`, no run
  storage, no reparent; it gates on cached inputs and mutates `base_color`.

#### Phase 4–5 Review

Architect re-review of Phases 6–7 against the shipped Phase 4/5 code. Edits
applied:
- **Phase 6 — scope expanded (user-approved):** delete the inline
  `clear_run_storage()` (it wipes the *shared* `GlyphCache.run_storage`, evicting
  panel runs) and add a `free_run_storage_on_world_mesh_removal`
  (`On<Remove, WorldTextMesh>`) observer, making the Phase-4 storage-cleanup
  invariant uniform across both text paths. Added a regression test.
- **Phase 7 — gating inputs widened (user-approved):** cache and compare
  `command_index`, treating a draw-slot shift as a material rebuild, so
  overlapping images keep correct `depth_bias` under reorder (text's M2 parity).
- **Phase 6 — minor notes folded in:** the in-place alpha write inherits Phase
  5's `get_mut`-marks-modified caveat (the cascade is the real no-op suppressor);
  the alpha branch no-ops when the entity has no `WorldTextMesh` child
  (empty/pending); the `Update`→`PostUpdate` change-tick survives without a
  run-order edge; world-text perf accounting mirrors "geometry owns the timer."
- **Phase 7 — minor notes folded in:** the image material is on the *same*
  entity (no two-hop `ChildOf` lookup like panel text); the reconcile-level input
  comparison is the real tint guard (no cascade for image tint), so the whole
  tint branch must be skipped on an unchanged input, not just the field write.

One confirmation finding (world text's `Without<PanelTextChild>` filtering and its
distinct `WorldTextMesh` marker are unaffected by the Phase-4 reparent) needed no
change.

### Phase 6 — world-text alpha short-circuit (Edit 2b) — Done
Larger than a one-line guard: it mirrors Phase 5's two-signal split, because
`render_world_text` (`world_text/rendering.rs:191-208`) has no per-entity
alpha branch — it always re-runs text shaping for the run, then on
Ready/Invisible/Failed does `clear_run_storage()` + `despawn_mesh_children` +
respawn, and its trigger already lumps alpha in with the rest
(`Or<(Changed<WorldText>, Changed<TextStyle>, Changed<Resolved<TextAlpha>>,
Changed<Resolved<FontUnit>>)>`, `mod.rs`).
- **Scope kept narrow (Phase-3 review, user decision).** For standalone world
  text the *only* runtime alpha signal this short-circuit can fire on is a
  global `CascadeDefault<TextAlpha>` change arriving as
  `Changed<Resolved<TextAlpha>>`; standalone entity overrides now use
  `override_text_alpha` and hit the same alpha-only branch. Opacity rides with
  `color` through the normal `Changed<TextStyle>` rebuild — not through
  this path.
- **Scope expansion — fix the shared-`GlyphCache` wipe + add a remove-observer
  (Phase-4/5 review, user-approved).** `render_world_text`
  (`world_text/rendering.rs:195`) calls `backend.clear_run_storage()`, which
  `backend.rs:172` implements as `self.run_storage.clear()` — it empties the
  *entire* shared `GlyphCache.run_storage` map, not just this entity's run.
  Phase 4 made the panel-text path depend on that same map staying keyed per-run
  and freed only by `free_run_storage_on_mesh_removal`. So any world-text rebuild
  evicts every panel run's storage (masked today only because the live app rarely
  mixes standalone world text with panels). Phase 6 must add
  `free_run_storage_on_world_mesh_removal` (`On<Remove, WorldTextMesh>`) mirroring
  the panel observer, and **delete the inline `clear_run_storage()`** so
  `despawn_mesh_children` frees storage per-run — making the storage-cleanup
  invariant (Phase 4) uniform across both text paths. (`clear_run_storage` may
  then be removable entirely if no caller remains.)
- Split the alpha-only case from the rest — a second system, or a `Ref`-based
  skip à la M7 that detects "only `Resolved<TextAlpha>` changed."
- For the alpha-only case, query the existing `WorldTextMesh` child's
  `MeshMaterial3d<TextMaterial>` and mutate `base.alpha_mode` in place,
  value-guarded (R5) — no re-run of text shaping, no `clear_run_storage`, no
  respawn.
- Keep the full text-shaping re-run + clear + despawn + respawn for `WorldText`
  / `TextStyle` / `Resolved<FontUnit>` changes. World text is single-run per
  entity, so no reparent needed. Depends on the Phase 5 pattern → stays after
  Phase 5.
- **The value-guard is belt-and-suspenders; the cascade is the real no-op
  suppressor (Phase-5 finding F3).** `materials.get_mut()` marks the
  `TextMaterial` asset modified on *access*, regardless of the `!=` guard. So the
  "alpha-only change doesn't needlessly re-prep" guarantee comes from
  `propagate_cascade` (`cascade/plugin.rs:92`) value-guarding the `Resolved`
  write so `Changed<Resolved<TextAlpha>>` never fires on a no-op — the alpha
  branch then never runs. The in-place field guard is a second line, not the
  primary one, exactly as Phase 5 found for panel text.
- **The alpha-only branch must no-op when the `WorldText` entity has no
  `WorldTextMesh` child (F4)** — empty text, or `PendingGlyphs` before the first
  ready build. Like Phase 5's panel alpha system (its inner `ChildOf` loop simply
  finds nothing), an absent mesh child is a no-op, not an error.
- **The `Update`→`PostUpdate` tick survives (F5).** `propagate_cascade` writes
  `Resolved<TextAlpha>` in `Update`/`CascadeSet::Propagate`; `render_world_text`
  reads `Changed<Resolved<TextAlpha>>` in `PostUpdate`. No run-order edge is
  needed — `PostUpdate` is strictly after `Update` in the same frame, so the
  change tick is visible (the same property Phase 5 relies on for panel text).
- **Perf-stat ownership across the split (F9).** `render_world_text` keeps its
  own `mesh_ms`/`total_ms` accounting (`rendering.rs`). Mirror Phase 5's
  "geometry/respawn owns the timer": the alpha-only branch touches no mesh, so an
  alpha-only frame reports ~0 world-text mesh time.
- **Invariant (Phase 6–7 review): keep the geometry trigger and the alpha skip
  in sync.** `ChangedWorldTextQuery`'s `Or` (the trigger, duplicated in the
  `mod.rs` wrapper query) and `update_world_text_alpha`'s skip
  (`WorldText` / `TextStyle` / `Resolved<FontUnit>` `is_changed()`) are
  three hand-maintained copies of the same signal set. The both-changed
  coordination — geometry owns the run, alpha no-ops — holds only while they
  match; adding a fourth geometry trigger without adding it to the skip reopens
  the despawn-vs-mutate race (the world-text twin of Phase 5's panel hazard).
  The alpha system's inner `run_meshes` scan is O(changed × world meshes), the
  same as the panel path — negligible while standalone world text is sparse.
- Files: `render/world_text/mod.rs`, `render/world_text/rendering.rs`,
  `render/world_text/mesh_spawning.rs`.
- Tests: a world-text alpha-only change mutates `base.alpha_mode` and does not
  respawn the mesh or re-touch run storage; a `WorldText`/`TextStyle`/
  `FontUnit` change still rebuilds; an alpha change on an entity with no mesh
  child (empty/pending) is a no-op; a world-text geometry rebuild frees only its
  own run's storage and leaves a co-existing panel run's storage intact (the
  shared-`GlyphCache` regression).

### Phase 7 — image per-run gating + tint split (Edit 3, R9 image part, M8) — Done
- Gate `reconcile_panel_image_children` on input equality (`handle`/`tint`/
  `bounds`, bounds via `to_bits`); a tint-only change mutates `base_color` in
  place (value-guarded), a bounds/handle change rebuilds the rectangle mesh +
  material. No reparent and no storage observer for images (M8).
- **Comparison semantics are per-field, not all `to_bits`.** Only `bounds`
  (four `f32`s) uses `to_bits`, mirroring the text gate. `tint` is a `Color`
  compared via its derived `PartialEq`, and `handle` via asset-`Handle`
  equality — neither is bit-compared. (Avoid reading "bounds via `to_bits`" as
  applying to the whole input set.)
- Cache the gating inputs: `PanelImageChild` today carries only `element_idx`,
  and the image reconcile query (`reconcile.rs:155`) reads only
  `PanelImageChild` + `ChildOf`. Since M8 gates on *inputs* (`handle`/`tint`/
  `bounds`), extend `PanelImageChild` to cache the prior inputs to compare
  against, and widen the query to hold the `MeshMaterial3d<StandardMaterial>`
  handle for the in-place `base_color` mutation.
- **Cache and compare `command_index` too — depth bias depends on it
  (Phase-4/5 review, user-approved).** Image `depth_bias` derives from `cmd_index`
  (`reconcile.rs:279`), but reuse is keyed by `element_idx` only
  (`reconcile.rs:255`). When a sibling image is inserted/removed, a reused image's
  draw-slot (`cmd_index`) shifts while its `handle`/`tint`/`bounds` do not — so a
  gate that ignores `cmd_index` keeps a stale `depth_bias` and overlapping images
  z-fight. Add `command_index` to `PanelImageChild`'s cached inputs and treat a
  `cmd_index` change as a **material rebuild** (the bias lives on the material).
  This gives images the layering-under-reorder correctness text gets from its
  `(element_idx, command_index)` reuse key (M2) — stronger than text's R7, which
  only loses a perf guarantee on reorder, because for images the stale bias is a
  visible artifact.
- **No two-hop lookup — the image material is on the same entity (Phase-5
  finding F6).** Unlike panel text, where Phase 4 pushed the material two hops
  down (`PanelChild` → `DiegeticTextMesh` child), `reconcile_panel_image_children`
  puts `Mesh3d` + `MeshMaterial3d<StandardMaterial>` directly on the
  `PanelImageChild` entity (`reconcile.rs:289-303`). Phase 7 reads the material
  handle off the same entity it iterates — do **not** copy panel text's `ChildOf`
  hop.
- **The reconcile-level input comparison is the real tint guard — there is no
  cascade here (F8).** Alpha is value-guarded upstream by `propagate_cascade`;
  image tint has no cascade, so the gating comparison in
  `reconcile_panel_image_children` itself is the *only* no-op suppressor. Because
  `materials.get_mut()` marks the `StandardMaterial` modified on access, the
  tint branch must be skipped entirely when the cached `tint` is unchanged — the
  R5 field-guard alone is insufficient. Gate at the comparison, not just at the
  write.
- **Depth-bias-under-reorder is the same concern as text, solved by the opposite
  mechanism (Phase 6–7 review).** Text keys reuse by `(element_idx,
  command_index)`, so a slot shift is a cache miss → respawn → fresh bias
  (M2/R7). Images key reuse by `element_idx` only, so the shift is instead caught
  by gating `command_index` and rebuilding the material. Both keep layering
  correct under reorder; the text path is not missing the image guard.
- Files: `render/panel_text/reconcile.rs`.
- Tests: a tint-only change updates `base_color` without rebuilding the mesh; an
  unchanged image is not re-touched (no `get_mut` call); inserting a sibling
  image above a reused one shifts its `cmd_index` and rebuilds its material so
  `depth_bias` stays correct (no z-fight on reorder).

**Doc-only (no commit of their own):** M1 is a §3 correction. M3 (add
`render/panel_text/mod.rs` to "Files touched") is satisfied and broader than
first scoped — `mod.rs` now registers two run-storage observers
(`free_run_storage_on_mesh_removal` + `free_run_storage_on_world_mesh_removal`)
and four `PostUpdate` systems (including `update_world_text_alpha`), not just the
single R2 registration. M4's four tests all exist and pass (panel + world
alpha-only no-respawn, storage-cleanup-on-despawn, R6 `GlobalTransform`), so M4
is satisfied with no separate fold needed.

**Follow-up (not in this plan):** the orphan `render/world_text/panel_child.rs`
(uncompiled, absent from `mod.rs`) is a stale second definition of `PanelChild` —
the live one lives in `world_text/mod.rs`. Delete it so a future edit cannot
touch the wrong copy.

#### Retrospective (Phases 6 & 7)

**What worked:**
- Phase 6 mirrored the Phase 4/5 split exactly: deleted the inline
  `clear_run_storage()` and the now-dead `GlyphCache::clear_run_storage` method,
  added `free_run_storage_on_world_mesh_removal` (`On<Remove, WorldTextMesh>`),
  and split alpha into `update_world_text_alpha` after narrowing both copies of
  the geometry trigger (`rendering.rs` `ChangedWorldTextQuery` alias + the
  `mod.rs` wrapper) to drop `Changed<Resolved<TextAlpha>>`.
- Phase 7 gated `reconcile_panel_image_children` on `handle`/`tint`/`bounds`/
  `command_index`: a tint-only change mutates `base_color` in place, anything
  else rebuilds mesh+material. Extracted `build_image_visuals` +
  `reconcile_existing_image` helpers (orchestrator pattern) and a local
  `bounds_bits` to bit-compare bounds.
- The shared-`GlyphCache` regression test passes: a standalone world-text rebuild
  with a co-existing panel run leaves the panel run's storage intact (was wiped
  by `clear_run_storage()` before).

**What deviated from the plan:**
- The two new world-text systems (`free_run_storage_on_world_mesh_removal`,
  `update_world_text_alpha`) are bare `pub` in the private `mesh_spawning`
  module and re-exported `pub(super)` from `world_text/mod.rs`, because the
  `TextRenderPlugin` that registers them lives in the sibling `panel_text`
  module — the leaf-module-visibility pattern, matching `WorldTextMesh`.
- The world-text alpha M7 skip needs three change-signals
  (`Ref<WorldText>` + `Ref<TextStyle>` + `Option<Ref<Resolved<FontUnit>>>`),
  not one, because the geometry trigger is a three-way `Or`. Panel text only
  needed `Ref<PanelText>`.

**Surprises:**
- `AssetEvent` is read with `MessageReader`, not `EventReader`, in this Bevy
  (0.19.0-rc.2) — buffered events are messages. The "unchanged image not
  re-touched" test uses a `MessageReader<AssetEvent<StandardMaterial>>` probe to
  assert no `Modified` event fires for the skipped image's material.
- `command_index` shift under reorder is testable purely through layout:
  toggling an image element's own `background` prepends a rectangle command,
  bumping the image's `command_index` from 0→1 while its `element_idx` stays
  stable — so reconcile reuses the child by `element_idx` and rebuilds its
  material for the new `depth_bias`.

**Implications for remaining phases:**
- No implementation phases remain. §10 (runtime settings ergonomics) is a
  separate track; M1/M3/M4 are doc-only corrections already folded in.

#### Phase 6–7 Review

Architect re-review against the shipped Phase 6/7 code. Phases 6 & 7 were the
last implementation phases, so the review weighted revealed gaps and new risks;
no `significant` findings (nothing changes scope, ordering, or architecture).
Minor edits applied:
- **Phase 6** gained an invariant note: the geometry trigger (`ChangedWorldTextQuery`
  `Or`, duplicated in the `mod.rs` wrapper) and `update_world_text_alpha`'s skip
  are three hand-maintained copies of one signal set that must stay in sync, or
  the despawn-vs-mutate race reopens; the alpha inner scan is O(changed × meshes)
  like the panel path.
- **Phase 7** gained a cross-reference: depth-bias-under-reorder is the same
  concern as text, solved oppositely (text respawns on a `(element_idx,
  command_index)` cache miss; images gate `command_index` and rebuild the
  material) — the text path is not missing the image guard.
- **Doc-only:** M3 marked satisfied-and-broader (`mod.rs` registers two observers
  + four systems, not just R2); M4 marked satisfied (all four tests exist and
  pass); added a follow-up to delete the orphan `world_text/panel_child.rs`
  (stale second `PanelChild` definition, not in this plan).

Confirmed clean (no change): deleting `clear_run_storage` left no orphaned
caller — all three text-despawn paths route through `On<Remove>` observers, and
the shared-`GlyphCache` regression test proves the cross-path leak is closed;
the both-changed coordination is sound on both text paths (geometry owns the
run, alpha `Option`-guards `get_mut`); §10 stays a correctly-scoped separate
track, with its gap-3 footgun (spawn-only `TextStyle.alpha_mode`/`.unit`)
reinforced but unchanged by Phase 6.

---

## 10. Runtime settings ergonomics (separate track — not in this plan)

Out of scope for the per-run perf work, recorded here because the Phase-3
review surfaced it: there is no single ergonomic way for user code to change a
text or panel setting after spawn. The per-attribute story is uneven — some
fields take a live mutation, some are spawn-only and silently no-op, some have
no public per-entity path at all. This section inventories what exists so a
later effort can design one uniform API.

### How each setting changes at runtime today

| Setting | Lives in | Per-entity runtime path | Notes |
| --- | --- | --- | --- |
| color (incl. opacity / the alpha channel) | `TextStyle.color` | **yes** — `set_color` (standalone) or `set_tree` (panel label) → `Changed<TextStyle>` rebuild | the renderer reads `style.color()` into the shader fill on each rebuild (`world_text/mesh_spawning.rs:118` → `material.rs:100`) |
| size / weight / slant / line-height / letter+word spacing / align / anchor / font_features / render_mode / shadow_mode / sidedness / lighting / world_scale | `TextStyle` | **yes** — direct `set_*` setter on standalone or `set_tree` for panel labels | re-read on the `Changed<TextStyle>` rebuild |
| blend mode (`AlphaMode`) | cascade `TextAlpha` | standalone: `override_text_alpha` / `inherit_text_alpha`; panel: `DiegeticPanel.text_alpha_mode`; labels can use `TextStyle::with_alpha_mode` | `Override`/`Resolved` stay crate-internal; public callers use typed verbs/readers |
| font unit | cascade `FontUnit` | standalone: `override_font_unit` / `inherit_font_unit`; panel labels inherit the panel font unit | standalone spawn seeds the global default, then typed overrides self-heal the target entity |
| text content / layout (panel) | `DiegeticPanel` tree | `set_tree` → reconcile (Phase 3 gated) | coarse: rebuilds the tree; reconcile reuses unchanged runs |

### The ergonomic gaps

1. **No granular setters.** `TextStyle` exposes consuming `with_*` builders
   and a single mutable `set_color` (`text_props.rs:343`). Changing any other
   field in place means reconstructing the whole component.
2. **Cascade attributes have no public per-entity setter.** `Override<A>` /
   `Resolved<A>` / `TextAlpha` / `FontUnit` are `pub(crate)`, so user code cannot
   change one standalone entity's blend mode or font unit at runtime — only the
   global default, or (for panels) the panel-level override the labels inherit.
3. **Spawn-only fields silently no-op on mutation.** `TextStyle.alpha_mode`
   and `.unit` are consumed once by the `On<Add>` seed; mutating them at runtime
   triggers a full rebuild that re-reads the *same* resolved value — wasted work
   with no visible change. A footgun until the seed re-runs on change or the
   fields leave `TextStyle`.

### Sketch of a uniform API (to design later)

- A mutable setter per `TextStyle` field (or a `&mut`-editing closure), so
  in-place tweaks don't reconstruct the component.
- A public per-entity way to set the cascade attributes — a typed setter that
  writes `Override<A>` and lets the propagation pass re-resolve — giving
  standalone text the same runtime blend-mode / font-unit control panels already
  get through the tree.
- Decide the fate of the spawn-only `TextStyle.alpha_mode` / `.unit`
  fields: either re-seed on `Changed<TextStyle>`, or drop them from the
  component now that the cascade owns those values, removing the gap-3 footgun.

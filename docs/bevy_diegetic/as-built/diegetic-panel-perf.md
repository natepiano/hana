# Panel Text Change Detection & Flash-Fix Scheduling

Panel text and images feed the batched-records render pipeline
([`material-table-batching.md`](material-table-batching.md)) through a reconcile pass that runs
every time a panel's tree changes. A single value change on a panel (one chip
flipping color, one label's text edited) must touch only the run(s) that
actually changed — every other run keeps its GPU records, material-table row,
and batch membership untouched. Two mechanisms deliver that:

1. **Scheduler ordering** in the `PostUpdate` batching pass, so a run that *does*
   rebuild acquires its transform, `Aabb`, and visibility the same frame — no
   one-frame blank flash.
2. **Change detection** in reconcile and shaping: content-stable reuse keys,
   per-component conditional writes, and bit-equality comparators (`gating_eq`)
   that keep an unchanged run un-`Changed`, so nothing downstream reprocesses it.

All code is in `crates/bevy_diegetic/src/render/panel_text/`. Systems are
registered in `TextRenderPlugin::build` (`mod.rs`).

## Flash-fix scheduler ordering

The batching pass has four systems whose relative order to Bevy's `PostUpdate`
transform/visibility propagation is load-bearing (`mod.rs`, and the top-of-file
schedule note in `batching.rs`):

```rust
update_panel_text_batches
    .after(shape_panel_text_children)
    .after(MaterialTableAppendReady)
    .before(TransformSystems::Propagate)
    .before(BatchResourcesReady),
write_batch_run_transforms
    .after(TransformSystems::Propagate),
update_batch_bounds
    .after(write_batch_run_transforms)
    .after(VisibilitySystems::CalculateBounds)
    .before(VisibilitySystems::CheckVisibility),
commit_batch_buffers
    .after(update_panel_text_batches)
    .after(write_batch_run_transforms),
```

- `update_panel_text_batches` writes each run's `PathRenderRecord` **before**
  `TransformSystems::Propagate`. The record carries a pre-propagation transform
  snapshot (`run_record_for`, `batching.rs`); freshly-spawned batch entities
  therefore exist before the propagation pass and get their `GlobalTransform`
  the same frame.
- `write_batch_run_transforms` runs **after** `Propagate` and overwrites each
  run record's transform with the now-propagated label `GlobalTransform`.
- `update_batch_bounds` hand-writes each batch entity's `Aabb` and sort
  translation **between** `CalculateBounds` and `CheckVisibility`, so the batch
  entity is visibility-classified the same frame it changes.
- `commit_batch_buffers` uploads dirty record buffers **last**, so render-world
  extraction sees this frame's data.

**Why the edge is explicit.** Without `.before(TransformSystems::Propagate)` the
scheduler is free to pick a topological order that runs the record write *after*
propagation (Bevy `0.19`'s scheduler does exactly this where `0.18`'s happened
not to). A batch entity spawned that frame would then miss the transform and
visibility pass and render blank for one frame — the whole-panel flash the
original per-panel-rebuild design exhibited. The batched-records path never
despawns per-run meshes on a value change, but new panels and new runs still
spawn batch entities that need same-frame transform + visibility, so the
ordering edge stays required.

## Reconcile: content-stable reuse + conditional writes

`reconcile_panel_text_children` (`reconcile.rs`) runs on every
`Changed<ComputedDiegeticPanel>`. It reuses existing run entities keyed by the
content-stable `(PanelElementId, line_index)` pair — `id` is the panel-local run
id (named or `Auto`), `line_index` the `0`-based line ordinal within a wrapped
run. This replaced the former positional `(element_idx, command_index)` key so a
named run survives a sibling reorder without respawning.

For a reused run, `update_reused_panel_text_child` writes each component **only
when it differs** (`reconcile.rs`), so an unchanged run stays un-`Changed`:

- `TextContent` — compared by `.text()`.
- `TextStyle` — compared by its derived/manual `PartialEq` (`reusable.style != &style`).
- `PanelTextLayout` — compared by `gating_eq` (below).
- `PanelTextDrawZIndex` / `PanelTextDrawZIndexRank` — compared by `==`.
- Cascade overrides (alpha, material, lighting, sidedness, shadow-casting, glyph
  shadow mode, HDR text coverage bias) — each gated in `sync_cascade_override`.

**Why cascade overrides are gated individually.** Each override drives a
`Changed<Resolved<A>>` signal that the batching/alpha path consumes to update a
run record in place. Writing one unconditionally re-fires `Changed<Resolved<A>>`
on every run every rebuild and defeats the per-run short-circuit. `Override<A>`
derives no `PartialEq`, so `sync_cascade_override` compares the inner value and
either applies the override, removes it (on `Inherit` when one is present), or
no-ops.

`reconcile_panel_image_children` is ordered `.after(reconcile_panel_text_children)`
so the two passes' shared `DiegeticPerfStats::reconcile_ms` reset-then-accumulate
is deterministic (text reconcile resets it, image reconcile accumulates).

## `gating_eq` comparators

Two bit-equality comparators decide "did this actually change." Both use
`to_bits` rather than `==` on floats — `+0.0`/`-0.0` are distinct bit patterns
and NaN compares equal to itself — matching the layout layer's own
`layout_eq_excluding_visuals`.

**`TextStyle::gating_eq`** (`layout/text_props.rs`, `pub(crate)`). Compares only
the *measurement* fields (`font_id`, `size`, `weight`, `slant`, `line_height`,
letter/word spacing, `align`, `anchor`, `font_features`) via `to_bits`. Excludes:

- render/material fields (`color`, `render_mode`, `shadow_mode`,
  `shadow_casting`, `sidedness`, `lighting`, `material`, `hdr_text_coverage_bias`)
  — `PreparedPanelText`, cascade overrides, and the frame material table own
  those without touching glyph geometry;
- `unit` — measurement *context*, not a mesh input;
- `alpha_mode` — gated separately through `Override<TextAlpha>`.

This comparator is *not* used by reconcile's `TextStyle` write (that uses
`PartialEq`); it is used by shaping (below) to distinguish a geometry change from
a render-only change.

**`PanelTextLayout::gating_eq`** (`layout.rs`, `pub(super)`). Bit-equality over
the fields a glyph mesh depends on: `bounds`, `draw_ordinal`, `depth_bias`,
`oit_depth_offset`, `scale_x`, `scale_y`, `anchor_x`, `anchor_y`, `clip_rect`.
Excludes the reuse-identity fields (`id`, `line_index`, `element_idx`) — those
are the key, not content. Used by reconcile at the `PanelTextLayout` write.

## Geometry vs render-only split in shaping

`shape_panel_text_children` (`shaping.rs`) processes each `Changed<TextStyle>`
run and decides whether the change needs a full reshape or only a record update.
`text_render_only_refresh` short-circuits when:

- neither `TextContent` nor `PanelTextLayout` changed, **and**
- `TextStyle` changed but `prepared.style_gate.gating_eq(config)` holds — i.e.
  only render fields moved.

In that case it updates `PreparedPanelText.render_mode` / `.shadow_mode` /
`.fill_color`, refreshes `style_gate`, and sets `PreparedPanelText.render_only =
true`. The batching pass reads `render_only` to update the existing run record in
place instead of re-deriving identical glyph quads; it is `false` on every full
reshape. `style_gate` is the `TextStyle` snapshot used for this comparison next
frame.

## Image gating + tint split

`reconcile_panel_image_children` (`reconcile.rs`) caches each image child's
inputs on its `PanelImageChild` component: `element_idx` (the reuse key),
`handle`, `tint`, `bounds`, `draw_depth` (`DrawCommandDepth`), `shadow_casting`.
`reconcile_existing_image` compares incoming against cached and branches:

- **`handle` / `bounds` / `draw_depth` moved** → rebuild the rectangle mesh +
  `StandardMaterial` (`build_image_visuals`). `depth_bias` is set from
  `draw_depth.screen_depth_bias()`, so a draw-order shift under sibling
  insert/remove rebuilds the material and keeps overlapping images from
  z-fighting.
- **tint-only** → mutate `base_color` on the existing material in place, guarded
  by `material.base_color != incoming.tint`.
- **nothing changed** (still refreshing layer + shadow-casting) → no material
  touch.

**Why the guard is at the comparison, not just the write.** Image tint has no
cascade layer suppressing no-ops (unlike text alpha, which `propagate_cascade`
value-guards upstream). `materials.get_mut` marks the asset modified on *access*,
so the tint branch must be *reached* only when the cached tint differs — the
input comparison in `reconcile_existing_image` is the sole no-op suppressor.
Images carry their material on the same entity (no `ChildOf` hop), reuse by
`element_idx`, and despawn orphans synchronously, so they need no reparenting and
no remove-observer.

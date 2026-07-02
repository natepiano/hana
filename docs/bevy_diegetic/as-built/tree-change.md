# Tree Change Classification

Explicit panel-tree replacement chooses the cheapest correct update path. When
user code replaces a panel's layout tree, the setter classifies old-vs-new and
records the result so a visual-only replacement (colors, backgrounds, border
color) skips the layout solve and only re-emits render commands, while a
structural or sizing change runs the full layout path.

This runs only when a caller explicitly replaces a tree through the optimized
API. It adds no per-frame comparison work to unchanged panels.

## Public API

```rust
commands.set_tree(panel_entity, next_tree);
```

`set_tree` is a `DiegeticPanelCommands` trait method (`panel/diegetic_panel.rs`)
implemented on `Commands`. It queues `set_tree_command` via
`run_system_cached_with`, so the replacement is **deferred**. Systems that
respond to the change (layout, screen placement) must run after the deferred
setter applies — the panel plugin provides that ordering (see Schedule).

`set_tree_command` classifies `panel.tree().classify_change(&next_tree)`, records
the result on the sibling `DiegeticPanelChangeClassification` component, then
replaces the tree via `replace_tree_full_rebuild`.

A `bench_support`-gated `DiegeticPanel::set_tree_full_rebuild` component method
forces the conservative full-layout path for benchmark comparisons. It is not
normal public API — a direct component method cannot update the sibling
classification component, which is why the optimized path goes through
`Commands`.

## Classification: `LayoutTreeChange`

`layout/element.rs`, re-exported as `crate::LayoutTreeChange`.

```rust
#[repr(u8)]
pub enum LayoutTreeChange {
    Identical       = 0,
    VisualOnly      = 1,
    LayoutAffecting = 2,
}
```

Ordered as a lattice; `combine` is `max`. Repeated queued replacements in one
frame compose to the strongest change, never last-wins.

- `LayoutAffecting`: geometry, measurement, wrapping, content bounds, or command
  placement may change. Full solve required.
- `VisualOnly`: geometry is still valid, but render commands must be re-emitted
  because colors/materials/backgrounds/borders/images may differ.
- `Identical`: nothing the classifier inspects changed.

`LayoutTree::classify_change` returns `LayoutAffecting` immediately on a root or
element-count mismatch, otherwise zips the two element vectors and `combine`s
per-element results, short-circuiting on the first `LayoutAffecting`.

Per-element classification (`classify_element_change`) **exhaustively
destructures** `Element` on both sides. Adding a field to `Element` (or the
content/border/child-layout helpers) fails compilation until the new field is
classified as layout-affecting or visual-only. Helpers: `classify_content_change`
(text/image/children/empty), `classify_border_change`,
`classify_child_layout_change`, `classify_child_divider_change`.

Current classification:

- **LayoutAffecting**: width, height, padding, overflow, scroll offset/anchors,
  editable field metadata, child-layout variant or gap/alignment, child order or
  references, border side widths, divider width, add/remove of border or
  divider, text sizing, text config fields that affect measurement
  (`config.layout_eq_excluding_visuals`), and visible-text content changes when
  `sizing.visible_text_affects_layout()`.
- **VisualOnly**: element id, background (including add/remove), corner radius,
  `draw` / `z_index`, anti-alias / hairline-fade / shadow-casting authoring,
  precompose mode, material handle, border color, divider color, text color and
  measurement-neutral config, and image handle or tint.

Image handle changes are `VisualOnly` because images are not intrinsically
measured by layout today. If intrinsic image sizing is ever added, image-handle
changes must move to `LayoutAffecting`.

## Panel-side state

`DiegeticPanelChangeClassification` is a required component of `DiegeticPanel`.
Its `pending: Option<LayoutTreeChange>` is transient — `None` on most frames,
`take`n by `compute_panel_layouts` after a replacement applies. It also tracks
`tree_visual_geometry_stable`, set true only when accumulated changes are
`VisualOnly` and each contributing change kept geometry stable.

- `record_tree_change` composes `pending` with `combine` and updates the
  geometry-stable flag.
- `note_text_edit` records a per-frame run-text edit as `VisualOnly` but clears
  the geometry-stable flag (the leaf must be re-measured to confirm its box did
  not move).
- `take_with_tree_visual_geometry_stable` drains both for the layout system.

`PanelTree` wraps the source `LayoutTree` with a `TreeRevision`; every
replacement or text/style edit bumps the revision. The revision means "source
tree content changed"; `LayoutTreeChange` decides whether layout geometry must
be recomputed. `ScaledLayoutTreeCache` (the point-scaled derived tree) is keyed
on `source_revision` plus the layout/font scale bits, so it takes the
revision-owned `PanelTree` and callers cannot pass mismatched cache identity.

## Geometry reuse: `LayoutResult`

`LayoutResult` (`layout/engine/layout_engine.rs`) caches enough to regenerate
render commands without re-running the solve: `computed` element bounds,
per-element `wrapped` text lines, `viewport_width`/`viewport_height`,
`font_scale`, and a `structure_hash`.

- `regenerate_commands(&mut self, tree)` rebuilds `commands` from cached
  positions and wrapped text via `render_commands_from_geometry`. Debug-asserts
  the structural invariant (same element count, same `structure_hash`) — the new
  tree must have the same structure as the tree that produced the geometry. Text
  and visual-only config are re-read from `tree`, so those flow through without a
  solve.
- `can_reuse_geometry(...)` is the correctness gate: returns `false` (forcing a
  full solve) on any structural change, viewport or `font_scale` change, a
  word-wrapped text leaf (cached line breaks belong to the old string), a newline
  in the new text, or a changed measured leaf width.

`ComputedDiegeticPanel::regenerate_commands` wraps the `LayoutResult` method and
rebuilds `DrawOrder` from the new commands. It returns `false` when no computed
result exists yet.

## System flow

`compute_panel_layouts` (`panel/compute_layout.rs`) processes each panel that is
`Changed` or has pending classification:

1. Skip entirely if neither changed nor pending.
2. `take` the pending change and geometry-stable flag.
3. `Identical` with an existing result → continue (no work).
4. `VisualOnly` and geometry is reusable (`tree_visual_geometry_stable`, or
   `can_reuse_geometry` re-measures and accepts) → `regenerate_commands`, fire a
   `PanelChangeKind::VisualOnly` panel-changed event, continue. Mutating
   `ComputedDiegeticPanel` marks it `Changed`, so render reconciliation
   (`render/panel_text/`) re-emits.
5. Otherwise run the full `LayoutEngine::compute` solve and commit.

## Schedule (invariant)

`panel/mod.rs`, `HeadlessLayoutPlugin`:

```
PanelSystems::ApplyTreeChanges  (ApplyDeferred)
  → ApplyConversions            (before ComputeLayout)
  → ComputeLayout               (compute_panel_layouts)
```

The `ApplyTreeChanges` `ApplyDeferred` boundary guarantees the deferred
`set_tree` command has applied — and recorded pending classification — before
`compute_panel_layouts` consumes it. Layout systems must stay ordered after this
boundary.

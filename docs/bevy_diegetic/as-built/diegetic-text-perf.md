# Diegetic text performance — main-thread CPU

This doc covers the **layout / main-thread** cost of panel text: how per-frame
text edits stay cheap. The **render-thread** side (glyph geometry, batching,
material rows, the shared glyph-outline atlas) lives in
[`material-table-batching.md`](material-table-batching.md) and
[`slug.md`](slug.md); this doc does not
repeat it.

The reference workload is `examples/diegetic_text_stress.rs`: 100 world labels,
each restrung every frame with a fixed-width `"NN MMM"` string. Every lever below
is what keeps that per-frame restringing off the layout solve.

## The three levers

### 1. The tree is the single source of truth for text

Panel text has one authoritative home: `El.text` in the panel's layout tree,
reached through `DiegeticPanel::sync_run_text_cache`. The per-run child's
`TextContent` component is **derived output only** — `reconcile_panel_text_children`
writes it tree→child, `shape_panel_text_children` reads it. Nothing writes
`TextContent` back into the tree.

Writes go through the `TextEdit` cursor
(`render/panel_text/access.rs`), handed to the `DiegeticTextMut::for_each_mut`
closure and used internally by `PanelText`:

```rust
pub struct TextEdit<'a> {
    panel:          Mut<'a, DiegeticPanel>,
    classification: Mut<'a, DiegeticPanelChangeClassification>,
    element_idx:    usize,
}
```

`TextEdit::set_text` writes `El.text` and records the edit for the skip below.
`text()` reads back from the tree cache.

Why single-source: the previous model kept text in two homes synced over one
`Changed<TextContent>` flag, which needs a one-frame `ReconcileOwned` marker to
stop reconcile's own tree→child write from looping back as a user edit. That
marker had a side effect — layout ran every *other* frame (a fragile accidental
gate) and a reflow edit landing on a marked frame lagged one frame. Removing the
two-way sync (`ReconcileOwned`, `sync_run_text_to_cache`, `clear_reconcile_owned`
are all gone) removed the lag and the accidental gate; the geometry-stable skip
(lever 3) replaces the gate with a designed one.

**Gotcha — no-op guard is load-bearing.** `TextEdit::set_text` read-compares
against the current tree string through `Deref` *before* taking the `&mut DiegeticPanel`
borrow, so an unchanged write dirties nothing: no relayout, no measure, no change
record. Restringing a label to the same value must stay free. Keep the equality
check before the mutable access.

### 2. `ShapedTextCache` is shared, not cloned per frame

`ShapedTextCache` (`layout/shape_cache.rs`) holds its two maps (glyph runs +
measurements) behind one `Arc<Mutex<ShapedTextCacheMaps>>`:

```rust
#[derive(Resource, Clone, Default)]
pub struct ShapedTextCache {
    inner: Arc<Mutex<ShapedTextCacheMaps>>,
}
```

All methods take `&self` and lock internally. `compute_panel_layouts` calls
`build_cached_measure`, which `cache.clone()`s the handle (a refcount bump, not a
map copy) into the `'static` `MeasureTextFn` closure the layout engine needs.
Two consequences that are the whole point:

- Cloning the cache into the closure each frame is free — no full-map copy.
- Cache misses the measure closure computes during layout are inserted back into
  the shared cache, so they persist for the renderer's shaper instead of being
  discarded at end of system.

**Gotcha:** if you add a code path that clones `ShapedTextCache` expecting an
independent copy, you get a shared handle instead — every write is visible through
every clone. That is intentional. Do not "fix" it into an owned copy.

### 3. Geometry-stable skip

`compute_panel_layouts` (`panel/compute_layout.rs`) runs the full layout solve
only when geometry can actually move. A text-only edit is classified `VisualOnly`
(via `TextEdit::set_text` → `DiegeticPanelChangeClassification::note_text_edit`).
For a `VisualOnly` change the system re-measures the edited leaves (cache-backed,
cheap) and, if every leaf's box is unchanged, takes the cheap path:
`regenerate_commands` from the cached positions and skip `LayoutEngine::compute`
entirely. The guard is `LayoutResult::can_reuse_geometry`
(`layout/engine/layout_engine.rs`): it rejects the reuse unless structure,
viewport, and every leaf's measured width are bit-identical and no leaf wraps —
so a genuine reflow (a width-changing edit) always falls through to the full
solve and never renders stale geometry.

```rust
let can_reuse_geometry = tree_visual_geometry_stable
    || computed.result().is_some_and(|result| {
        result.can_reuse_geometry(scaled_tree, &cached_measure,
            viewport_width, viewport_height, 1.0)
    });
if matches!(pending_change, Some(LayoutTreeChange::VisualOnly))
    && can_reuse_geometry
    && computed.regenerate_commands(scaled_tree)
{
    // cheap path: regenerate render commands, no engine solve
    continue;
}
```

Every `"NN MMM"` stress label measures identical frame to frame, so all 100 take
the cheap path each frame — the layout solve does not run for a same-width text
edit. This is content-agnostic: it works whether width is fixed by declaration or
merely stable by content.

## Change-detection gating (upstream of all three)

`compute_panel_layouts` only touches a panel whose `DiegeticPanel` is
`Ref::is_changed()` or has a pending tree change; an unchanged panel is skipped
before any measure. A `LayoutTreeChange::Identical` change with an existing result
also skips. So an idle frame does no layout work at all, and a text-only frame
does the lever-3 cheap path.

## Invariants

1. **Tree is authoritative for text; `TextContent` is derived.** Only reconcile
   writes `TextContent` (tree→child); only shaping reads it. Route every writer
   through `TextEdit` / `sync_run_text_cache`, never back into `TextContent`. Do
   not reintroduce a child→tree sync or a `ReconcileOwned`-style marker.
2. **No-op-no-work.** A `set_text` to the current value drives zero relayout and
   zero measure. The `Deref` equality check must stay before the `&mut` borrow.
3. **`ShapedTextCache` is a shared handle.** Clone is a refcount bump; measure
   closures write through it. Keep methods `&self`.
4. **Geometry-stable skip never renders stale geometry.** `can_reuse_geometry`'s
   three guards (same measured width, no newline/wrap, same structure+viewport)
   gate the cheap regenerate path; a width-changing edit must fall through to the
   full solve.

## Observability

`DiegeticPerfStats` (`panel/perf.rs`) publishes the main-thread layout cost:
`compute_ms` (the `compute_panel_layouts` wall-clock, zeroed on a no-panel frame)
and `compute_panels` (how many panels relaid out this frame). The
`diegetic_text_stress` overlay reads these plus the render-thread rows. For the
render-thread breakdown and per-pass timing constraints on macOS, see
`material-table-batching.md`.

# Tree Change Optimization Plan

Goal: make explicit panel-tree replacement choose the cheapest correct update
path.

This should only run when a caller explicitly replaces a panel tree with
the optimized tree setter. It should not add per-frame comparison work.

## Target Workflow

When user code replaces a panel tree through the optimized API:

```rust
commands.set_diegetic_panel_tree(panel_entity, next_tree);
```

the queued setter should classify the change:

```rust
match old_tree.classify_change(&next_tree) {
    TreeChange::Identical => skip layout and skip render command regeneration,
    TreeChange::VisualOnly => reuse existing layout geometry and regenerate render commands,
    TreeChange::LayoutAffecting => run the full layout path,
}
```

The important split is:

- `LayoutAffecting`: geometry, measurement, wrapping, content bounds, or command
  placement may change.
- `VisualOnly`: geometry is still valid, but render commands may need to be
  re-emitted because colors/materials/backgrounds/borders/images may differ.

## Why This Shape

Today `LayoutEngine::compute` does two jobs:

1. Compute layout geometry.
2. Emit render commands from that geometry.

That coupling makes visual-only changes expensive, because skipping layout also
skips the only place render commands are currently generated.

The general solution is to separate those phases:

1. Compute and store layout geometry.
2. Generate render commands from the stored geometry plus the current tree.

Then a visual-only tree replacement can skip measurement, sizing, wrapping, and
positioning while still producing a fresh render command list.

## Change Classification

Classification should be exhaustive over stored tree fields. Adding a new field
to `Element`, `ElementContent`, `Border`, or `LayoutTextStyle` should force us
to decide whether that field is layout-affecting or visual-only. Implement this
with exhaustive destructuring rather than field-by-field `if` chains so new
fields fail compilation until they are classified.

Layout-affecting examples:

- tree root, element count, child order, child references
- width, height, padding, child gap, direction, alignment
- text content
- text measurement fields: font id, size, weight, slant, line height, letter
  spacing, word spacing, wrap mode, font features
- border widths and between-children border width
- clip behavior

Visual-only examples:

- text color
- background color, including adding or removing a background
- border color, including adding or removing a zero-width visual border only if
  command generation can represent it safely
- corner radius
- material changes only after a dedicated comparator confirms the changed
  material data is visual-only; otherwise treat material changes as
  `LayoutAffecting`
- image tint
- image handle under current behavior, because images are not intrinsically
  measured by layout today
- text render/compositing fields that do not affect measurement

If image intrinsic sizing is added later, image handle changes must move to
`LayoutAffecting`. The classifier should make that dependency explicit, for
example with a shared intrinsic-image-sizing predicate or feature gate.

## Data Model Changes

Add a sibling required component that stores internal pending-change state:

```rust
#[derive(Component, Default)]
pub(super) struct DiegeticPanelChangeClassification {
    pending: Option<PendingTreeChange>,
}
```

The component lives for the lifetime of the panel entity. Its `pending` value is
transient: most frames it is `None`, and `compute_panel_layouts` consumes it with
`take()` after a tree replacement has been applied.

```rust
enum PendingTreeChange {
    Identical,
    VisualOnly,
    LayoutAffecting,
}
```

Absence of pending work should be modeled separately:

```rust
pending: Option<PendingTreeChange>
```

Repeated queued tree replacements in one frame must compose by taking the strongest
change, not by last-wins assignment:

```rust
pending = Some(match pending.take() {
    None => change,
    Some(prior) => prior.combine(change),
});
```

`PendingTreeChange` should be ordered as a small lattice:

```rust
#[repr(u8)]
enum PendingTreeChange {
    Identical = 0,
    VisualOnly = 1,
    LayoutAffecting = 2,
}
```

`combine` can then be implemented with `max`.

The optimized public mutation path should be a `Commands` extension that queues
a cached setter system:

```rust
commands.set_diegetic_panel_tree(entity, next_tree);
```

Internally that extension can use `run_system_cached_with` so the system has
access to both components:

```rust
fn set_diegetic_panel_tree(
    In((entity, next_tree)): In<(Entity, LayoutTree)>,
    mut panels: Query<(
        &mut DiegeticPanel,
        &mut DiegeticPanelChangeClassification,
    )>,
) {
    // classify old tree vs next_tree, compose pending, then replace panel.tree
}
```

This setter is deferred. Systems that respond to tree changes, including
`compute_panel_layouts`, must run after the deferred setter has been applied.
The plugin schedule must provide an `ApplyDeferred` boundary between user code
that queues `set_diegetic_panel_tree` and layout systems that consume the
pending classification.

Full-rebuild benchmark comparisons should use a `bench_support`-only helper
that replaces the tree and forces the conservative full-layout path. Normal
callers that want optimized change classification should use
`commands.set_diegetic_panel_tree(entity, next_tree)`. Any remaining direct
`DiegeticPanel::set_tree` call sites are compatibility surface to migrate
toward the command API rather than the long-term preferred path.

## Layout Result Refactor

Refactor `LayoutResult` so render command generation can run independently from
layout geometry computation.

Current shape:

```rust
LayoutResult {
    computed: Vec<ComputedLayout>,
    commands: Vec<RenderCommand>,
}
```

Target shape can stay source-compatible at first, but internally it needs enough
cached layout data to regenerate commands without recomputing geometry:

- computed element bounds
- wrapped text line data
- content bounds
- layout-time scale inputs used for geometry and text wrapping
- a structural guard, such as an element-count check plus a layout-time
  structure hash

Then expose an internal method:

```rust
impl LayoutResult {
    fn regenerate_commands(&mut self, tree: &LayoutTree);
}
```

This method should use existing element bounds and wrapped text lines, not call
text measurement or layout sizing. It should also assert the structural
invariant that the new tree has the same element order and count as the tree
that produced the cached geometry.

`regenerate_commands` should use the scale data captured at layout time. If a
caller supplies live scale inputs, debug assertions must verify they match the
layout-time values; otherwise a font-unit change could reuse wrapped text with
the wrong scale.

## System Flow

`compute_panel_layouts` should become:

```rust
// Runs after ApplyDeferred for set_diegetic_panel_tree.
for changed panel {
    match pending_tree_change.take() {
        None => {
            run full layout;
        }
        Some(PendingTreeChange::Identical) => {
            continue;
        }
        Some(PendingTreeChange::VisualOnly) => {
            computed.regenerate_commands(panel.tree());
            handle_scaled_tree_cache_visual_change();
            continue;
        }
        Some(PendingTreeChange::LayoutAffecting) => {
            run full layout;
        }
    }
}
```

The scaled-tree cache contract must account for visual-only changes.
Visual-only changes and layout-affecting changes both bump the existing tree
revision. `tree_revision` means "source tree content changed", while
`PendingTreeChange` decides whether layout geometry must be recomputed. The
cache should not rely on an out-of-band `clear` whose invalidation behavior is
not visible in the key.

Cache key fields should use narrow newtypes such as `TreeRevision` and
`F32Bits` instead of raw `u64` / `u32` values once the cache surface is touched
for this work.

## Benchmark Plan

Keep benchmarks in the existing matrix:

1. `layout_engine_raw`
   - keep `layout_tree_diff_*` classifier benches
   - add `regenerate_commands_only`
   - compare against `scale_tree_only` and `raw_compute_prebuilt_tree`

2. `panel_perf`
   - add `visual_only_rebuild`
   - compare against current `color_change_rebuild`
   - use a fixture shaped like the typography font list: stable labels, stable
     fonts, active color changes

Success criteria:

- `VisualOnly` public path is materially faster than full `color_change_rebuild`.
- `LayoutAffecting` path is not slower in common cases.
- No work is added to unchanged frames.

## Test Plan

Unit tests:

- identical tree classifies as `Identical`
- text color-only classifies as `VisualOnly`
- background add/remove classifies as `VisualOnly`
- text content change classifies as `LayoutAffecting`
- font size/id change classifies as `LayoutAffecting`
- border color-only classifies as `VisualOnly`
- border width change classifies as `LayoutAffecting`
- combined visual and layout changes classify as `LayoutAffecting`
- empty tree to populated tree, and populated tree to empty tree, classify as
  `LayoutAffecting`
- material changes are classified by an explicit material comparator, not by
  `Option::is_some`
- image handle changes follow the documented intrinsic-sizing guard
- layout-text `unit` / `world_scale` behavior is covered so the classifier does
  not report layout-affecting changes for fields that layout text truly ignores

System tests:

- queued tree setter with visual-only change updates render commands without
  changing content bounds
- repeated `VisualOnly` then `LayoutAffecting` changes in the same frame run
  the full layout path
- repeated `VisualOnly` -> `LayoutAffecting` -> `VisualOnly` sequences bump
  tree revision for each source-tree replacement and do not return stale scaled
  trees from cache
- queued tree setter applies before `compute_panel_layouts` consumes the pending
  classification
- queued tree setter with visual-only change bumps `tree_revision` but does not
  recompute layout geometry
- queued tree setter with layout-affecting change bumps `tree_revision` and
  recomputes layout geometry
- visual-only update marks `ComputedDiegeticPanel` changed so render
  reconciliation runs

## Implementation Order

1. Move `LayoutTree::classify_change` into crate-private production code before
   the optimized path uses it. Keep only benchmark harness access behind
   `bench_support`.
2. Implement classifier methods with exhaustive destructuring and add the
   classification unit tests.
3. Add the required `DiegeticPanelChangeClassification` component and the
   command-backed `set_diegetic_panel_tree` API.
4. Add a `bench_support`-only full-rebuild tree setter for benchmark
   comparisons.
5. Schedule layout response after the deferred tree setter has applied.
6. Update tree revision semantics so any source-tree replacement bumps
   `tree_revision`.
7. Extract render command generation from layout positioning into a reusable
   internal pass.
8. Store wrapped text data, content bounds, scale inputs, and structural guards
   in `LayoutResult` so commands can be regenerated without measurement.
9. Add pending tree-change recording to the queued tree setter.
10. Add the `VisualOnly` branch in `compute_panel_layouts`.
11. Add public-path benchmark coverage for visual-only tree replacement.
12. Migrate crate examples from direct `DiegeticPanel::set_tree` calls to
    `commands.set_diegetic_panel_tree(entity, next_tree)` once the optimized
    command API is tested and benchmarked.
13. After examples and internal callers use the command API, decide whether
    direct `DiegeticPanel::set_tree` should stay as a documented conservative
    escape hatch or be deprecated.

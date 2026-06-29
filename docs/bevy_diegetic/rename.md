# Draw-order rename and concept split work order

Temporary planning doc. Delete this file after the new names and conceptual
model have been swept into the permanent `docs/bevy_diegetic/` docs.

## Goal

The current draw-order vocabulary mixes authoring, layout commands, batch
identity, sort placement, and depth transport. We will fix this in order:

1. Mechanical renames only.
2. Behavior-neutral code simplification.
3. Concept split and warning changes.
4. Docs sweep through code comments and `docs/bevy_diegetic`.

Nate will do the rename-only pass in the editor. Codex will not make code
changes until Nate explicitly says to.

## Current Problem

The code currently couples several different ideas through one integer model:

- authored panel-local stacking: `DrawZIndex`;
- render projection range: `z_level * DRAW_LEVEL_STRIDE`;
- reserved renderer-family positions: SDF surface, panel shape, and text
  "sublanes";
- per-command sorted rank inside one z-index;
- per-record render-depth nudging;
- OIT per-fragment ordering.

Names like `DrawStep`, `Fill`, `Shapes`, `z_level`, `level_ordinal`,
`sublane`, `geometry`, and `lane` make the reader carry hidden type
relationships in their head.

## Rename-only Pass

Rules for this pass:

- Mechanical renames only.
- Do not change numeric values.
- Do not change sort behavior.
- Do not add or remove enum variants.
- Do not remove `PanelDrawKind` yet.
- Do not introduce `DrawBatchFamily` yet.
- Do not change the warning behavior yet.

### 1. Sort-tier Names

These renames make the existing ordering type say what it already does.

| Order | Current name | New name | Scope |
| --- | --- | --- | --- |
| 1 | `DrawStep` | `DrawSortTier` | Type rename only. |
| 2 | `DrawStep::Fill` | `DrawSortTier::Surface` | Variant rename only. |
| 3 | `DrawStep::Shapes` | `DrawSortTier::PanelShape` | Variant rename only. |
| 4 | `DrawStep::Text` | keep | Already clear. |
| 5 | `DrawStep::ordinal()` | `DrawSortTier::sort_order()` | Method rename only. |
| 6 | `RenderCommandKind::draw_step()` | `draw_sort_tier()` | Method rename only. |
| 7 | `HierarchicalDrawKey.step` | `sort_tier` | Field rename only. |
| 8 | `HierarchicalDrawKey.tree_order` | `command_index` | Field rename only; keep the current type during the rename-only pass. |

Meaning after the rename:

```text
RenderCommandKind -> DrawSortTier
```

`RenderCommandKind` remains the concrete command kind. `DrawSortTier` is only
the coarse ordering bucket used by the sort key.

Important invariant:

```text
sort key = (DrawZIndex, DrawSortTier, command_index)
```

Do not sort by the full `RenderCommandKind`. Surface commands must keep their
natural emitted order through `command_index`; otherwise rectangle, image, and
border commands could be regrouped incorrectly.

### 2. Panel Shape Names

These renames remove the generic `Shapes` wording where it means panel-shape
renderer data.

| Order | Current name | New name | Scope |
| --- | --- | --- | --- |
| 9 | `RenderCommandKind::Shapes` | `RenderCommandKind::PanelShapes` | Variant rename only. |
| 10 | local helper names like `lines()` when they return `RenderCommandKind::Shapes` | `panel_shapes()` | Test/helper rename only. |

Do not rename the public `PanelDraw::lines()` constructor in this pass. It is an
authoring convenience that specifically accepts lines.

### 3. Command-rank Names

These renames separate panel-wide rank from z-index-local rank.

| Order | Current name | New name | Scope |
| --- | --- | --- | --- |
| 11 | `DrawOrdinal` | `PanelDrawCommandRank` | Type rename only. |
| 12 | `DrawCommandDepth::ordinal` field | `panel_draw_command_rank` | Field rename only. |
| 13 | `RankedDrawCommand::ordinal` field | `panel_draw_command_rank` | Field rename only. |
| 14 | `ordinal_index()` | `panel_draw_command_rank_index()` | Method rename only. |
| 15 | `DrawCommandDepth::ordinal()` | `panel_draw_command_rank()` | Test-only accessor rename. |
| 16 | local/test helper `ordinal_at` | `panel_draw_command_rank_at` | Helper rename only. |
| 17 | `level_ordinal` | `command_rank_in_z_index` | Field/local rename only. |
| 18 | `enumerate_ordinals()` | `rank_draw_commands_for_test()` | Test helper rename only. |

Meaning after the rename:

```text
PanelDrawCommandRank
```

Panel-wide sorted rank after `(DrawZIndex, DrawSortTier, command_index)`.

```text
command_rank_in_z_index
```

Sorted rank within one `DrawZIndex`. This resets when `DrawZIndex` changes.

`PanelDrawCommandRank` is not the same thing as `CommandIndex`:

```text
CommandIndex
```

Original slot in `LayoutResult::commands`. It is the emitted command-stream
index, before draw sorting, and scissor commands can have one.

```text
PanelDrawCommandRank
```

Dense rank after sorting draw-participating commands by
`(DrawZIndex, DrawSortTier, command_index)`. It excludes scissor commands and
is the value used to derive depth/OIT ordering.

### 4. Z-index Projection Names

These renames remove generic "level" language from values that are really
derived from `DrawZIndex`.

| Order | Current name | New name | Scope |
| --- | --- | --- | --- |
| 19 | `z_level` | `z_index` | Field/local rename only. |
| 20 | `z_level()` | `z_index()` | Method rename only. |
| 21 | `current_z_level` | `current_z_index` | Local rename only. |
| 22 | `level_occupancy()` | `command_counts_by_z_index()` | Method rename only. |
| 23 | `warn_panel_draw_order_limit_occupancy` | `warn_panel_draw_order_limit_counts` | Function rename only. |

This pass does not change the data type. If a later code change introduces a
typed `DrawZIndexBand`, that is not part of the rename-only pass.

### 5. Band and Sort-offset Constants

These renames remove "sublane" and "geometry lanes" from constants while the
current math is still in place. Do not treat a generic band offset as a durable
domain concept; the later concept split should separate command ranks from
batch sort anchors instead of preserving one shared offset namespace.

| Order | Current name | New name | Scope |
| --- | --- | --- | --- |
| 24 | `DRAW_LEVEL_STRIDE` | `DRAW_Z_INDEX_BAND_WIDTH` | Constant rename only. |
| 25 | `DRAW_LEVEL_FILL_SUBLANE` | `SDF_SURFACE_BATCH_SORT_ANCHOR` | Constant rename only. |
| 26 | `DRAW_LEVEL_TEXT_SUBLANE` | `TEXT_BATCH_SORT_ANCHOR` | Constant rename only. |
| 27 | `DRAW_LEVEL_GEOMETRY_START_SUBLANE` | `FIRST_COMMAND_SORT_OFFSET` | Constant rename only. |
| 28 | `DRAW_LEVEL_GEOMETRY_LANES` | `COMMAND_SORT_OFFSET_CAPACITY` | Constant rename only. |

Do not add `PANEL_SHAPE_BATCH_SORT_ANCHOR` yet unless it is needed as a pure
alias. Turning the existing expression into a new constant can wait for the code
change pass.

### 6. Depth-bias Helper Names

These renames make helper names describe what they calculate.

| Order | Current name | New name | Scope |
| --- | --- | --- | --- |
| 29 | `level_sublane_depth_bias` | `z_index_band_offset_depth_bias` | Function rename only. |
| 30 | `geometry_depth_bias` | `command_depth_bias` | Function rename only. |
| 31 | `fill_batch_depth_bias` | `sdf_surface_batch_depth_bias` | Function rename only. |
| 32 | `line_batch_depth_bias` | `panel_shape_batch_depth_bias` | Function rename only. |
| 33 | `text_batch_depth_bias` | keep | Already clear enough. |

`z_index_band_offset_depth_bias` is a temporary compatibility name for the
existing helper. It should not force a long-lived `DrawZIndexBandOffset` type
into the model.

### 7. Renderer Transport Names

These renames are allowed, but only after the core sort/rank/band names are
settled.

| Order | Current name | New name | Scope |
| --- | --- | --- | --- |
| 34 | `depth_nudge` | `clip_depth_nudge` | Host and WGSL field rename only. |
| 35 | `ScreenDepthBias` | keep | Already names Bevy sort/depth-bias transport. |
| 36 | `OitDepthOffset` | keep | Already names OIT transport. |

## Behavior-neutral Code Cleanup

Codex will do this only after Nate says to proceed.

### Remove Duplicate Panel-shape Test Helper

After `RenderCommandKind::Shapes` has been renamed to
`RenderCommandKind::PanelShapes`, remove the duplicate test-only
`panel_shapes()` helper from one of the two draw-order test modules.

Current duplicated helpers:

- `render/draw_order.rs` test module;
- `render/draw_order_limits.rs` test module.

Preferred cleanup:

- keep the helper only where it has several call sites;
- inline `RenderCommandKind::PanelShapes { shapes: Vec::new() }` in the smaller
  test module, or move to a shared test helper only if more modules need it.

### Remove `PanelDrawKind`

`PanelDrawKind` currently has one private variant:

```rust
pub(super) enum PanelDrawKind {
    Shapes(Vec<PanelShape>),
}
```

It is not a draw-order concept and does not add type safety. Replace it with:

```rust
pub struct PanelDraw {
    shapes:   Vec<PanelShape>,
    overflow: DrawOverflow,
}
```

Keep `PanelDraw` as the public authored API object.

### Simplify `PanelDraw::lines()`

`PanelLine` is already a shape:

```rust
pub enum PanelShape {
    Line(PanelLine),
    Circle(PanelCircle),
}
```

`PanelDraw::lines()` is currently convenience sugar over `PanelDraw::shapes()`:

```rust
pub fn lines(lines: impl IntoIterator<Item = PanelLine>) -> Self {
    Self::shapes(lines.into_iter().map(PanelShape::Line))
}
```

After the rename-only pass, consider changing `PanelDraw::shapes()` so callers
can pass either `PanelLine` or `PanelShape` directly:

```rust
pub fn shapes(shapes: impl IntoIterator<Item = impl Into<PanelShape>>) -> Self
```

Then either:

- keep `PanelDraw::lines()` only as public compatibility sugar; or
- remove/deprecate it in a planned API cleanup if the crate is allowed to make
  that breaking change.

Do not remove `PanelDraw::lines()` during the mechanical rename pass.

### Type `HierarchicalDrawKey::command_index`

After `tree_order` is renamed to `command_index`, change the field type from
`u32` to the existing `CommandIndex`.

Current shape:

```rust
command_index: u32,
```

Target shape:

```rust
command_index: CommandIndex,
```

`CommandIndex` already means "slot in `LayoutResult::commands`" and derives the
ordering traits needed by `HierarchicalDrawKey`. This removes the extra
`u32::try_from(index)` conversion from the sort key path.

After this change, update test helpers that index rank vectors by command slot
to take `CommandIndex` too, for example:

```rust
fn panel_draw_command_rank_at(
    ranks_by_command_index: &[Option<PanelDrawCommandRank>],
    command_index: CommandIndex,
) -> PanelDrawCommandRank
```

### Type `command_rank_in_z_index`

After the rename-only pass, consider replacing the raw `i32` with a newtype:

```rust
struct CommandRankInZIndex(i32);
```

This is the real domain value: the rank assigned to one draw command among
commands with the same `DrawZIndex`.

Do not add a matching `DrawZIndexBandOffset` domain newtype unless the old
shared offset namespace survives the concept split. The intended direction is
to remove that shared namespace and keep batch sort anchors separate from
command ranks.

### Type Z-index Projection Values

After the rename-only pass, change draw-order projection fields that carry the
authored z-index from raw `i8` to `DrawZIndex`.

Likely targets:

- `DrawCommandDepth::z_index`;
- `RankedDrawCommand::z_index`.

Those values are not a separate renderer concept. They come directly from
`RenderCommand::z_index`, so the typed value should survive through the
draw-order projection. Convert to raw `i8` only at boundaries that still need
the numeric payload, such as current batch keys, material depth-bias helpers,
or debug/BRP summaries.

Do not convert every `z_level: i8` mechanically. Use this rule:

- fields/locals that identify the authored draw-order bucket become
  `z_index: DrawZIndex`;
- shader/material payloads and arithmetic internals may remain raw integers
  after being renamed;
- BRP/debug summary fields are not limited by BRP. They may also use
  `DrawZIndex` when the diagnostic schema should expose the authored domain
  value. Keep a primitive there only when the summary intentionally presents a
  numeric/export-friendly value.

Likely follow-up: derive `Hash` for `DrawZIndex` before storing it directly in
hash-map batch keys.

Conversion rule: implement `From` on `DrawZIndex`, not handwritten `Into`.
Non-const authoring helpers may accept `impl Into<DrawZIndex>` for ergonomics.
Keep `const fn` APIs accepting `DrawZIndex` directly unless we intentionally
drop their constness.

### Add Explicit Batch-family Vocabulary

Introduce this only after the rename pass:

```rust
enum DrawBatchFamily {
    SdfSurface,
    PanelShape,
    Text,
}
```

Then derive it from `RenderCommandKind`:

```text
RenderCommandKind -> DrawBatchFamily
```

This is not the same thing as `DrawSortTier`. `Image` and `PrecomposeLdr` can
sort in `DrawSortTier::Surface` without belonging to
`DrawBatchFamily::SdfSurface`.

Batch keys should read conceptually as:

```text
DrawZIndex + DrawBatchFamily + compatibility = GPU batch
```

### Add Panel-shape Batch Anchor

After constants are renamed, replace the panel-shape batch expression with an
explicit constant if it improves readability:

```text
PANEL_SHAPE_BATCH_SORT_ANCHOR
```

That is a batch sort anchor, not a command rank.

## Concept Changes

Codex will do these only after the rename-only pass and behavior-neutral cleanup
are complete.

### Separate Diagnostics From Fixed Command-count Capacity

Status: done.

Deleted the warning that fires on raw
`COMMAND_SORT_OFFSET_CAPACITY`.

A high command count in one `DrawZIndex` is not itself a renderer failure.
Future diagnostics should describe actual risk:

- normalized per-record spacing is too small to preserve overlapping records;
- OIT offset span approaches the precision budget;
- a batch sort anchor causes a visible cross-panel ordering issue;
- overlapping records fail an explicit overlap or precision check.

### Decide Depth Projection Semantics

Decide separately whether to normalize per-record ordering inside each
`DrawZIndex`, for example:

```text
rank / (rank_count - 1)
```

This cannot rely only on fractional `StandardMaterial::depth_bias`, because
Bevy integer-casts material depth bias for the pipeline depth-bias state. Any
semantic change must specify which renderer transport carries fine ordering:

- transparent sort via float `depth_bias`;
- non-OIT per-record `clip_depth_nudge`;
- OIT per-record `OitDepthOffset`;
- hardware depth bias via integer material value.

## Verification

After code changes, run:

```text
cargo +nightly fmt --all -- --check
cargo nextest run -p bevy_diegetic
```

## Documentation Sweep

After implementation, sweep both code comments and docs.

Update:

- rustdoc and comments near renamed symbols;
- tests whose names still imply old concepts;
- comments, test names, and panic messages saying "line batch" when they mean
  the panel-shape batch;
- `docs/bevy_diegetic/as-built/panel-draw-order.md`;
- `docs/bevy_diegetic/sdf-material-table-batching.md`;
- any batching or performance docs that mention "sub-lanes", "geometry lanes",
  `DrawStep`, `z_level`, `level_ordinal`, or the 64-command warning.

Then delete `docs/bevy_diegetic/rename.md`.

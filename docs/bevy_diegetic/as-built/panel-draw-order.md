# Panel draw order

## What it is

`DrawOrder` is the per-panel ordering projection for draw-participating layout
commands. It starts from the authored `DrawZIndex` on each element, combines it
with a fixed per-command draw tier and the original layout command index, then
stores the derived values needed by the screen-depth and OIT paths.

The central rule is:

```text
(DrawZIndex, DrawSortTier, CommandIndex)
```

That one sorted command stream feeds three related values:

- `DrawOrderIndex`: panel-wide rank in the sorted draw stream.
- `CommandSortOffset`: rank within the command's authored `DrawZIndex`.
- `DrawCommandDepth`: the packaged depth values used by rendering code.

Scissor commands remain in the layout command stream, but they do not receive a
draw depth because they do not emit visible fragments.

## Sort Inputs

`DrawZIndex` is the authored signed z bucket. `DrawZIndex(0)` is the default. A
negative value sorts behind default content inside the same panel; a positive
value sorts in front of it. The value is panel-local and must not be interpreted
as a cross-panel ordering key.

`DrawSortTier` is the fixed command kind ordering used inside one `DrawZIndex`:

```text
Surface < PanelShape < Text
```

`CommandIndex` is the command's original index in `LayoutResult::commands`. It is
the stable "later wins" tie breaker after `DrawZIndex` and `DrawSortTier`. It is
not recomputed by batching.

## Example

The layout engine emits commands in command-index order:

| `CommandIndex` | `RenderCommandKind` | `DrawSortTier` | `DrawZIndex` | Draws? |
| ---: | --- | --- | ---: | --- |
| 0 | `Rectangle` | `Surface` | 0 | yes |
| 1 | `Text` | `Text` | 0 | yes |
| 2 | `Rectangle` | `Surface` | 0 | yes |
| 3 | `Text` | `Text` | -1 | yes |
| 4 | `ScissorStart` | none | 0 | no |
| 5 | `PanelShapes` | `PanelShape` | 1 | yes |
| 6 | `Text` | `Text` | 1 | yes |

`DrawOrder::from_commands` sorts only the draw-participating commands:

| `DrawOrderIndex` | `CommandIndex` | Sort key `(DrawZIndex, DrawSortTier, CommandIndex)` | `CommandSortOffset` | Screen-depth position | OIT position |
| ---: | ---: | --- | ---: | --- | --- |
| 0 | 3 | `(-1, Text, 3)` | 0 | `-1 * DRAW_Z_INDEX_BAND_WIDTH + FIRST_COMMAND_SORT_OFFSET + 0 = -65` | `(0 - text_anchor) * OIT_DEPTH_STEP` |
| 1 | 0 | `(0, Surface, 0)` | 0 | `0 * DRAW_Z_INDEX_BAND_WIDTH + FIRST_COMMAND_SORT_OFFSET + 0 = 1` | `(1 - text_anchor) * OIT_DEPTH_STEP` |
| 2 | 2 | `(0, Surface, 2)` | 1 | `0 * DRAW_Z_INDEX_BAND_WIDTH + FIRST_COMMAND_SORT_OFFSET + 1 = 2` | `(2 - text_anchor) * OIT_DEPTH_STEP` |
| 3 | 1 | `(0, Text, 1)` | 2 | `0 * DRAW_Z_INDEX_BAND_WIDTH + FIRST_COMMAND_SORT_OFFSET + 2 = 3` | `(3 - text_anchor) * OIT_DEPTH_STEP` |
| 4 | 5 | `(1, PanelShape, 5)` | 0 | `1 * DRAW_Z_INDEX_BAND_WIDTH + FIRST_COMMAND_SORT_OFFSET + 0 = 67` | `(4 - text_anchor) * OIT_DEPTH_STEP` |
| 5 | 6 | `(1, Text, 6)` | 1 | `1 * DRAW_Z_INDEX_BAND_WIDTH + FIRST_COMMAND_SORT_OFFSET + 1 = 68` | `(5 - text_anchor) * OIT_DEPTH_STEP` |

Command `1` was emitted before command `2`, but it sorts after command `2`
because `Text` sorts after `Surface` inside `DrawZIndex(0)`. `CommandIndex`
breaks ties only after `DrawZIndex` and `DrawSortTier` match.

In this example, `text_anchor` is `0` because the first sorted command is also
the first text command. If there is no text, the anchor falls back to `0`.

## Derived Values

`DrawOrderIndex` is the dense panel-wide sorted rank. It is used for
`OitDepthOffset`:

```text
oit_depth_offset = (draw_order_index - text_anchor) * OIT_DEPTH_STEP
```

`CommandSortOffset` is the dense rank within the current authored `DrawZIndex`.
It resets when the sorted stream enters a new z index. It is used for
`ScreenDepthBias`:

```text
screen_depth_bias =
    command_depth_bias(z_index, command_sort_offset)
```

`DrawCommandDepth` stores the command's `DrawOrderIndex`, `DrawZIndex`,
`ScreenDepthBias`, and `OitDepthOffset`. `clip_depth_nudge()` returns the
screen-depth value in the floating-point form used by vertex-pulled shader
records. It changes the rendered depth value for sorting; it does not change the
authored world-space position.

## Screen Sort Positions

Each authored `DrawZIndex` owns a fixed-width screen-depth band:

| Constant | Value | Purpose |
| --- | ---: | --- |
| `SDF_SURFACE_BATCH_SORT_ANCHOR` | 0 | Batch material anchor for SDF surfaces. |
| `FIRST_COMMAND_SORT_OFFSET` | 1 | First per-command screen sort position. |
| `COMMAND_SORT_OFFSET_CAPACITY` | 64 | Number of reserved per-command sort positions before batch anchors. |
| `PANEL_SHAPE_BATCH_SORT_ANCHOR` | 64 | Batch material anchor for panel-shape records. |
| `TEXT_BATCH_SORT_ANCHOR` | 65 | Batch material anchor for text records. |
| `DRAW_Z_INDEX_BAND_WIDTH` | 66 | Distance from one z-index band to the next. |

The fixed batch anchors exist because SDF surfaces, panel shapes, and text are
submitted as batches. Their material `depth_bias` needs a stable z-index-derived
anchor even when individual records inside the batch carry their own shader
record depth values.

The screen-depth helpers are:

- `sdf_surface_batch_depth_bias(z_index)` for the SDF surface batch anchor.
- `panel_shape_batch_depth_bias(z_index)` for the panel-shape batch anchor.
- `text_batch_depth_bias(z_index)` for the text batch anchor.
- `command_depth_bias(z_index, command_sort_offset)` for per-command positions.

`COMMAND_SORT_OFFSET_CAPACITY` is a screen-sort layout constant. It is not a
diagnostic threshold by itself.

## Batching

Batch keys still include `DrawZIndex` where the batch must submit separate
materials for different z indices. Records inside those batches carry
`DrawCommandDepth`, which gives the shader path the command-specific
`clip_depth_nudge()` and `oit_depth_offset()`.

This separates two jobs:

- The batch key says which records can share a GPU draw call and a material.
- `DrawCommandDepth` says where each command lands in the panel's sorted draw
  order.

That distinction matters most for text and panel shapes: many records can share
the same batch, while each record still keeps the command depth that came from
the panel's `DrawOrder`.

## Diagnostics

`warn_panel_draw_order_limits` no longer warns when one `DrawZIndex` contains
more than `COMMAND_SORT_OFFSET_CAPACITY` commands. The old warning treated the
fixed screen-sort layout as a likely visual problem, but it counted commands,
not overlapping fragments, and it ignored the fact that dense non-overlapping
panel content is common.

The remaining warning is panel-wide OIT budget pressure. It sums
`DrawOrder::command_counts_by_z_index()` and compares the total draw-command
count with `oit_depth_budget()`. That budget is derived from
`OIT_FOCUS_DEPTH / OIT_DEPTH_STEP`; if the panel exceeds it, the OIT depth offset
range can reach the focus-depth budget used by the weighted OIT shaders.

## Invariants

- The sorted command stream and OIT offsets use the same `DrawOrderIndex`.
- `DrawZIndex` is panel-local; it must not reorder one panel's content against
  another panel's content.
- `CommandIndex` is the layout command index. Batching must not replace it with
  glyph order, path order, entity order, or submission order.
- Scissor commands do not receive `DrawCommandDepth`.
- A z-index or tier change affects ordering only. It does not respawn the owning
  panel entity or change authored world-space geometry.

## Open Semantics

The current implementation still has fixed screen-depth batch anchors and a
fixed `COMMAND_SORT_OFFSET_CAPACITY`. That is separate from the removed warning.
Whether screen-depth bias, clip-depth nudge, and OIT offsets should be normalized
from the full sorted command stream is a semantic design decision, not part of
this docs cleanup.

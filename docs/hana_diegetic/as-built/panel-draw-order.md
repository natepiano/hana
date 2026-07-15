# Panel draw order

## What it is

`DrawOrder` is the per-panel ordering projection for draw-participating layout
commands. It starts from the authored `DrawZIndex` on each element, combines it
with a fixed per-command draw tier and the original layout command index, then
caches the values the render paths need.

The central rule is:

```text
DrawOrderKey = (DrawZIndex, DrawSortTier, CommandIndex)
```

`DrawOrder::from_commands` sorts those keys and assigns:

```text
DrawZIndexRank = dense rank of each distinct DrawZIndex in the sorted z-index set
DrawOrderIndex = dense rank in the sorted draw-command stream
```

That sorted stream feeds the render values:

- `DrawCommandDepth`: cached per-command values derived from `DrawZIndexRank`
  and `DrawOrderIndex`.
- Batch material `depth_bias`: derived from the batch key's `DrawZIndexRank`.
- Uploaded record `clip_depth_nudge`: the command's absolute
  `ClipDepthNudge`, made relative to the batch material base.
- Uploaded record `oit_depth_offset`: the command's text-anchored
  `OitDepthOffset`, kept absolute to the panel.

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

`DrawOrder::from_commands` sorts only the draw-participating commands. The
screen depth value changes once per distinct authored `DrawZIndex`, while the
clip and OIT values still change once per sorted draw command:

| `DrawOrderIndex` | `CommandIndex` | `DrawOrderKey` | `DrawZIndexRank` | `ScreenDepthBias` | `ClipDepthNudge` | `OitDepthOffset` |
| ---: | ---: | --- | ---: | --- | --- | --- |
| 0 | 3 | `(-1, Text, 3)` | 0 | `0 * LAYER_DEPTH_BIAS` | `0` | `(0 - text_anchor) * OIT_DEPTH_STEP` |
| 1 | 0 | `(0, Surface, 0)` | 1 | `1 * LAYER_DEPTH_BIAS` | `1` | `(1 - text_anchor) * OIT_DEPTH_STEP` |
| 2 | 2 | `(0, Surface, 2)` | 1 | `1 * LAYER_DEPTH_BIAS` | `2` | `(2 - text_anchor) * OIT_DEPTH_STEP` |
| 3 | 1 | `(0, Text, 1)` | 1 | `1 * LAYER_DEPTH_BIAS` | `3` | `(3 - text_anchor) * OIT_DEPTH_STEP` |
| 4 | 5 | `(1, PanelShape, 5)` | 2 | `2 * LAYER_DEPTH_BIAS` | `4` | `(4 - text_anchor) * OIT_DEPTH_STEP` |
| 5 | 6 | `(1, Text, 6)` | 2 | `2 * LAYER_DEPTH_BIAS` | `5` | `(5 - text_anchor) * OIT_DEPTH_STEP` |

Command `1` was emitted before command `2`, but it sorts after command `2`
because `Text` sorts after `Surface` inside `DrawZIndex(0)`. `CommandIndex`
breaks ties only after `DrawZIndex` and `DrawSortTier` match.

In this example, `text_anchor` is `0` because the first sorted command is also
the first text command. If there is no text, the anchor falls back to `0`.

## Cached Values

`DrawOrder` stores an index-aligned cache:

```rust
pub(crate) struct DrawOrder {
    depths: Vec<Option<DrawCommandDepth>>,
}
```

The vector is aligned with the panel's `RenderCommand` list:

```text
commands[0] -> depths[0]
commands[1] -> depths[1]
commands[2] -> depths[2]
```

`DrawOrder::depth_for(command_index)` directly indexes that vector. It does not
scan the command list. The value is `None` for scissor commands and out-of-range
indices.

`DrawCommandDepth` is the cached per-command projection:

```rust
pub(crate) struct DrawCommandDepth {
    draw_order_index: DrawOrderIndex,
    z_index_rank: DrawZIndexRank,
    z_index: DrawZIndex,
    screen_depth_bias: ScreenDepthBias,
    clip_depth_nudge: ClipDepthNudge,
    oit_depth_offset: OitDepthOffset,
}
```

The projections are:

```text
screen_depth_bias = z_index_rank * LAYER_DEPTH_BIAS
clip_depth_nudge = draw_order_index
oit_depth_offset = (draw_order_index - text_anchor) * OIT_DEPTH_STEP
```

These are separate projections even when the numeric values are close. The
screen value is used for Bevy's `StandardMaterial::depth_bias`. The clip value
is written into vertex-pulled records and applied in the vertex shader for
non-OIT. The OIT value is written into records and added to `position.z` before
OIT depth packing.

## Batch Depth Bias

Batch keys keep both `DrawZIndex` and `DrawZIndexRank` as splitters. Commands
in different authored z-index values do not share one retained batch, and
commands whose panel-local rank differs do not share a batch material.

Each batch stores:

```text
first_draw_order_index = minimum DrawOrderIndex in that batch
batch material depth_bias = DrawZIndexRank * LAYER_DEPTH_BIAS
```

Each uploaded record stores its non-OIT clip value relative to that batch base:

```text
uploaded clip_depth_nudge =
    command clip_depth_nudge - first_draw_order_index clip_depth_nudge
```

For the example above:

| Batch | Source commands | Batch `DrawZIndex` | `DrawZIndexRank` | `first_draw_order_index` | Material `depth_bias` | Uploaded `clip_depth_nudge` values |
| --- | --- | ---: | ---: | ---: | --- | --- |
| SDF surface batch | `CommandIndex` 0, 2 | 0 | 1 | 1 | `1 * LAYER_DEPTH_BIAS` | `0`, `1` |
| text batch | `CommandIndex` 1 | 0 | 1 | 3 | `1 * LAYER_DEPTH_BIAS` | `0` |
| panel-shape batch | `CommandIndex` 5 | 1 | 2 | 4 | `2 * LAYER_DEPTH_BIAS` | `0` |
| raised text batch | `CommandIndex` 6 | 1 | 2 | 5 | `2 * LAYER_DEPTH_BIAS` | `0` |

This is why there is no fixed "text = 65" or "shape = 64" position anymore.
The batch material gets the rank of the authored z-index band, and the records
keep the rest of the command order relative to the batch base.

## Batching

Batch keys decide which records can share a GPU draw call and material. They use
coarse compatibility facts such as `DrawZIndex`, render layers, shadow mode,
pipeline compatibility, and resource compatibility.

Records inside those batches carry per-command values:

- `clip_depth_nudge` for non-OIT vertex depth adjustment.
- `oit_depth_offset` for OIT fragment ordering.

That separation is intentional:

- The batch key says which records can share a draw call.
- The batch material says which authored z-index band the draw call belongs to.
- The record says where one command lands inside the panel's sorted draw order.

## Diagnostics

There is no command-count warning for a dense `DrawZIndex` or for a large panel
command stream. Command count alone does not say whether fragments overlap on
screen, so the old warning produced noise for complex but ordinary panels.

The current depth-order diagnostic is visual: `depth_bias_stress.rs` places a
large rear panel behind a smaller front panel and varies the rear panel's command
count. That example protects the hardware `StandardMaterial::depth_bias` rule:
material depth changes once per `DrawZIndexRank`, while per-command ordering
stays in `clip_depth_nudge` and `oit_depth_offset`.

## Invariants

- `DrawOrderKey` is the single ordering key for commands.
- `DrawZIndexRank` is the dense rank of distinct authored z-index values in one
  panel.
- `DrawOrderIndex` is the dense per-command rank after sorting.
- Batch material `depth_bias` uses the batch key's `DrawZIndexRank`.
- Uploaded non-OIT `clip_depth_nudge` values are relative to the batch's
  `first_draw_order_index`.
- OIT `oit_depth_offset` values stay absolute to the panel's text anchor.
- `DrawZIndex` is panel-local; it must not reorder one panel's content against
  another panel's content.
- `CommandIndex` is the layout command index. Batching must not replace it with
  glyph order, path order, entity order, or submission order.
- Scissor commands do not receive `DrawCommandDepth`.
- A z-index or tier change affects ordering only. It does not respawn the owning
  panel entity or change authored world-space geometry.

# Child Layout Typestate and Overlay Layout

## What It Is

`hana_diegetic` has a type-safe child-layout API that separates row, column, and
overlay layout at the public builder boundary while keeping the runtime layout
engine non-generic. Authors use `El::row()` or `El::column()` for sequential
child flow with gaps and child dividers, and `El::overlay()` for children that
share the same content rectangle. This removes negative-gap overlap workarounds
and makes invalid overlay states, such as gaps or child dividers on overlays,
unrepresentable at compile time.

## How It Works

The public declaration type is `El<L = Row>` in
`crates/hana_diegetic/src/layout/builder.rs`. Its layout state is one of the
public marker types `Row`, `Column`, or `Overlay`, all accepted by the public
`ChildLayoutState` trait. That trait is sealed by a private supertrait so callers
can use the marker types but cannot add their own layout states. These names are
re-exported from both `layout/mod.rs` and `lib.rs`.

`El<Row>` exposes row construction, spacing, and dividers:

```rust
pub fn new() -> Self
pub fn row() -> Self
pub fn gap(self, gap: impl Into<Dimension>) -> Self
pub const fn child_divider(self, divider: ChildDivider) -> Self
```

`El<Column>` exposes column construction, spacing, and dividers:

```rust
pub fn column() -> Self
pub fn gap(self, gap: impl Into<Dimension>) -> Self
pub const fn child_divider(self, divider: ChildDivider) -> Self
```

`El<Overlay>` exposes only overlay construction:

```rust
pub fn overlay() -> Self
```

Common element fields live in private `CommonEl`: sizing, padding, alignment,
background, outer border, clipping, scroll offsets, independent scroll anchors,
material, editable field state, draw primitives, z-index, and anti-aliasing
overrides. Common setters are generic over `L`, so size, padding, alignment,
background, border, clipping, scrolling, draw, and z-index methods work for all
layout states.

`LayoutBuilder` accepts any child-layout state through generic entry points:

```rust
pub fn with_root<L>(el: El<L>) -> Self
where
    L: ChildLayoutState;

pub fn with<L>(&mut self, el: El<L>, children: impl FnOnce(&mut Self)) -> &mut Self
where
    L: ChildLayoutState;

pub fn text_element<L>(
    &mut self,
    el: El<L>,
    text: impl Into<String>,
    config: TextStyle,
) -> &mut Self
where
    L: ChildLayoutState;

pub fn text_id_element<L>(
    &mut self,
    id: impl Into<PanelElementId>,
    el: El<L>,
    text: impl Into<String>,
    config: TextStyle,
) -> &mut Self
where
    L: ChildLayoutState;

pub fn image<L>(&mut self, el: El<L>, handle: Handle<Image>, tint: Color) -> &mut Self
where
    L: ChildLayoutState;
```

`El::into_element(...)` lowers the public typestate into the private runtime
layout representation. Container nodes preserve their authored layout state;
text and image leaves normalize their child layout to the default inert row
layout because leaves cannot have children.

The runtime engine stores a plain internal `ChildLayout` enum in
`crates/hana_diegetic/src/layout/child_layout.rs`:

```rust
Row { gap, align_x, align_y, divider }
Column { gap, align_x, align_y, divider }
Overlay { align_x, align_y }
```

`ChildLayout` provides helpers for alignment, optional row/column dividers,
row/column main gaps, unit scaling, and axis classification.
`AxisRole::{RowMain, ColumnMain, Cross, Overlay}` drives sizing and positioning
so overlay is not treated as a fake row or column.

The shared `content_box(...)` helper in `engine/sizing.rs` subtracts padding and
border and is used for overlay sizing, positioning, scrolling, and text wrapping.
Row and column main-axis sizing still sums children and gaps, applies
compression/expansion, and preserves existing percent/grow behavior. Overlay
sizing uses each child independently against the parent content box: `Grow`
fills the content box, `Percent` resolves against the content box, `Fit` keeps
propagated natural size, and no sibling distribution or gap accumulation runs.

Positioning uses `ChildFlow::Overlay` in `engine/positioning.rs`. Overlay
children are all placed at the content-box origin minus the scroll offset, then
offset by both `AlignX` and `AlignY`. Overlay content extents are the max child
width and max child height, producing independent horizontal and vertical scroll
ranges. `scroll_x(...)`, `scroll_y(...)`, and `scroll_y_from_end(...)` mutate
only their own axis anchor and offset.

`Border` in `geometry.rs` is only the outer element border. Row/column
separators use `ChildDivider`, stored inside row/column `ChildLayout` and
emitted as `RectangleSource::ChildDivider`. Overlay cannot hold or emit child
dividers.

Opt-in compile-time guarantees live in `crates/hana_diegetic/tests/trybuild/**`:
row/column/overlay helper signatures compile, while `El::overlay().gap(...)`,
`.child_gap(...)`, `.direction(...)`, and `.child_divider(...)` intentionally
fail. The `typestate_helper_signatures_compile` trybuild test is ignored by
default because it is slow; run it when changing the public layout typestate
helpers.

## Invariants

Invalid overlay states must remain unrepresentable in the public API. Overlay
must not expose gap, child-gap, direction, or child-divider methods.

The layout engine must stay non-generic. Public `El<L>` lowers into the internal
`ChildLayout` enum before layout computation.

`Row`, `Column`, `Overlay`, and `ChildLayoutState` must be exported wherever
`El` is exported. The internal `ChildLayout` and private lowering trait must stay
hidden.

Row and column behavior must remain compatible with the previous layout model:
main-axis gaps, child alignment, percent/grow sizing, clipping, wrapping, scroll
behavior, and Clay parity expectations are not overlay semantics.

Overlay children must share the parent content rectangle. Overlay fit size is
max child extent plus padding and border, overlay percent/grow sizing resolves
against the content box, overlay positioning uses both alignment axes, overlay
scroll extents are independent per axis, and overlay never emits child dividers.

Text and image leaves must normalize child layout state to the default inert row
layout.

`DrawZIndex` behavior is unchanged. Overlay only makes overlap expressible; it
does not introduce a separate draw-order system.

Negative row/column gaps are no longer the overlap mechanism. Intentional
sibling overlap should use `El::overlay()`.

## Calibration / Gotchas

The content box subtracts both padding and border. This matters for bordered
overlay text wrapping, overlay percent/grow sizing, scroll extents, and child
positioning.

Row/column cross-axis percent sizing deliberately preserves the existing
parent-size basis. Overlay percent sizing instead uses the content-box size.

Overlay `Fit` children keep their propagated natural size when present;
otherwise they fall back to their minimum. Overlay `Grow` children fill the
content box independently, not as a distributed share.

`ChildDivider` width changes are layout-affecting; divider color-only changes
are visual-only. Outer border width is layout-affecting; outer border color-only
changes are visual-only.

Searches for old Clay-style method names still find intentional matches in Clay
reference declarations, side-by-side Clay examples, migration docs, and trybuild
fail fixtures. Do not treat those as Diegetic API regressions without checking
context.

`panel_draw_order` is an overlay-based `DrawZIndex` example: the story text and
sweep are siblings inside one overlay, and the controls change only the sweep
element's `DrawZIndex`.

`diegetic_text_stress` uses overlay tracks for GPU pipeline lanes. Segment
blocks and labels share the same lane rectangle without negative row/column
gaps.

## Why

Typestate puts layout validity at the API boundary. Overlay cannot accidentally
inherit row/column-only spacing or divider affordances, so invalid combinations
fail before they reach layout computation.

The internal enum keeps the retained layout tree and engine simple. The engine
does not need to become generic over public marker types, and change
classification can compare concrete runtime variants.

Keeping alignment in common element state lets all layout states use the same
fluent setters while still lowering alignment into the runtime child-layout
variant that needs it.

Splitting `ChildDivider` from `Border` prevents the common border API from
carrying row/column-only separator behavior into overlay. Keeping the public name
`Border` for outer borders minimizes churn while making divider ownership
explicit.

Overlay is an explicit axis role because it has different sizing, positioning,
and scrolling semantics from both main-axis and cross-axis layout. Modeling it
directly avoids hidden row/column assumptions in percent sizing, gap math, scroll
ranges, and divider emission.

Overlay replaces negative-gap overlap because overlap is a layout mode, not
spacing. That makes examples, diagnostics, and future panels easier to reason
about: row and column arrange siblings, overlay layers siblings.

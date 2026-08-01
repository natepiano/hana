# Constrained screen sizing

Status: **proposal**. This is the span-driven screen-space sizing feature:
derive a screen panel's width or height from two external anchor references. It
is intentionally separate from point placement
([`as-built/panel-anchoring.md`](as-built/panel-anchoring.md)).

## Goal

Allow a screen panel to say:

```text
my left edge  = window bottom-left x
my right edge = camera panel bottom-left x
```

That determines the panel's width. The cross-axis position and height can still
come from normal screen placement and normal sizing.

The GPU meter example:

- bottom-left position pinned to the window bottom-left
- width spans to the camera panel's bottom-left
- height stays `Fit`

## Why this is not a panel attachment

A point attachment pins one point:

```text
my BottomRight = camera panel BottomLeft
```

That determines position, not size. Width only becomes determined when another
constraint also fixes the opposite edge:

```text
my BottomLeft  = window BottomLeft
my BottomRight = camera panel BottomLeft
```

That is a span. It needs explicit sizing semantics so one relationship does not
silently change panel dimensions.

## Public model

Keep the panel builder's required size slot, but add a constrained sizing mode:

```rust
DiegeticPanel::screen()
    .size(Constrained, Fit)
```

The span details live in a component:

```rust
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ScreenSizeConstraints {
    pub width: Option<ScreenAxisSpan>,
    pub height: Option<ScreenAxisSpan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenAxisSpan {
    pub start: ScreenAnchorRef,
    pub end: ScreenAnchorRef,
    pub start_margin: f32,
    pub end_margin: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenAnchorRef {
    Window(Anchor),
    Panel { entity: Entity, anchor: Anchor },
}
```

For a width span, only the x component of each anchor reference is used. For a
height span, only the y component is used. This avoids accidental rotation or
skew if the two referenced anchor points do not line up on the cross axis.

## GPU meter shape

Conceptually:

```rust
ScreenSizeConstraints {
    width: Some(ScreenAxisSpan {
        start: ScreenAnchorRef::Window(Anchor::BottomLeft),
        end: ScreenAnchorRef::Panel {
            entity: camera_panel,
            anchor: Anchor::BottomLeft,
        },
        start_margin: 0.0,
        end_margin: 1.0,
        min: 120.0,
        max: f32::INFINITY,
    }),
    height: None,
}
```

The panel can still use normal placement for its bottom-left point:

```rust
DiegeticPanel::screen()
    .size(Constrained, Fit)
    .anchor(Anchor::BottomLeft)
```

With the default screen position, `Anchor::BottomLeft` pins the panel's
bottom-left point to the window's bottom-left point.

The exact builder syntax can improve later; the important split is that
placement pins a point, while `ScreenSizeConstraints` computes the width.

## Resolver timing

Constrained screen sizing is harder than simple attachment because width can
affect text layout. If a panel wraps text, the width must be known before
`compute_panel_layouts` produces content height.

The first implementation should resolve constrained axes before layout when the
axis is used as a layout input:

```text
resolve constrained screen widths
compute panel layouts
resolve Fit heights
resolve screen-space attachments
position screen-space panels
```

If a constrained width depends on a target panel whose own size is `Fit`, the
target must be resolved first. That implies a dependency graph. The minimal
version can support spans whose endpoint panels already have fixed, percent, or
previously resolved dimensions, then broaden to full graph ordering.

## Bounds API

This feature needs resolved screen bounds, not just dimensions:

```rust
pub struct PanelScreenBounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl PanelScreenBounds {
    pub fn point(&self, anchor: Anchor) -> Vec2;
}
```

`PanelDimensionsChanged` is useful for invalidation, but it does not include
location. A span resolver needs both size and screen position.

## First implementation

1. Add a `Constrained` sizing marker for screen axes.
2. Add `ScreenSizeConstraints`, `ScreenAxisSpan`, and `ScreenAnchorRef`.
3. Implement width-only screen spans.
4. Restrict first-pass span endpoints to window anchors and already-resolved
   screen panel bounds.
5. Use the GPU meter as the example: bottom-left placement, constrained width,
   `Fit` height.
6. Add tests for window resize, target panel movement, min/max clamping, and a
   constrained width feeding a `Fit` height.

## Open questions

- Should constrained axes be a new `Sizing` variant or a separate component that
  overrides a placeholder size?
- Should endpoint panel bounds be current-frame required, or can first pass use
  last-frame bounds for dependency-breaking?
- Should margins be scalar per axis, or `Vec2` so the same type can later drive
  point spans?
- Should a span endpoint reference a panel anchor, a panel edge, or a future
  public `PanelAnchorPoint` entity?

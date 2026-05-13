# Shared Width Layout Groups

## Status

Idea note - not an implementation plan yet. Captures a recurring layout need:
make a set of elements share the widest intrinsic width in the group.

## Problem

Several panel layouts want table-like columns without hand-measuring text or
hard-coding a width. A typical case is:

```text
MMB drag
Trackpad          -> Orbit
--------------------------
Shift+MMB drag
Shift+trackpad    -> Pan
--------------------------
Wheel
Ctrl+trackpad     -> Zoom
Pinch
```

The desired rule is simple:

```text
action column width = max(width("Orbit"), width("Pan"), width("Zoom"))
```

Today this has to be approximated with something like:

```rust
El::new().width(Sizing::fit_min(ACTION_COLUMN_MIN_WIDTH))
```

That works when the widest label is known, but it is not declarative. It also
makes layout code carry knowledge that belongs in the layout engine.

## What it might look like

Add a shared intrinsic width group. Elements in the same group keep their normal
sizing behavior, but their minimum width becomes the maximum intrinsic width of
all elements in that group.

Possible API:

```rust
El::new()
    .width(Sizing::FIT)
    .shared_width("camera-actions")
```

Or, if it belongs on sizing:

```rust
El::new().width(Sizing::fit_shared("camera-actions"))
```

The first version is probably cleaner: sizing still says how the element wants
to size itself, and `shared_width` adds a cross-element constraint.

## Expected Behavior

For three elements:

```rust
text("Orbit").shared_width("actions")
text("Pan").shared_width("actions")
text("Zoom").shared_width("actions")
```

the resolved widths should be:

```text
Orbit: width("Orbit")
Pan:   width("Orbit")
Zoom:  width("Orbit")
```

If a group member's own content is wider than the current group maximum, the
group maximum updates and all members receive the wider minimum.

This is most useful when paired with growable neighboring cells:

```text
[binding stack: Grow] [arrow: Fit] [action: Fit + shared_width("actions")]
```

The action column stays stable, so the growable binding column absorbs the
remaining space consistently and the arrows align without manual measurement.

## Layout Engine Implications

This probably needs an additional intrinsic-size pass before final top-down
sizing:

1. Measure leaf content and propagate normal `Fit` sizes.
2. Collect shared-width group maxima from participating elements.
3. Apply the group maximum as a minimum width to each participating element.
4. Run normal axis sizing and positioning.

The same pattern may eventually apply to shared heights, but width is the
recurring need today.

## Open Questions

- Should shared groups use strings, typed IDs, or a generated handle?
- Are groups scoped to a single `LayoutTree`, a panel, or a subtree?
- Should the constraint be `min-width` only, or should it force exact equality?
- How does it interact with wrapping text, where intrinsic width can change
  depending on available width?
- Should shared size participate in tree-change classification as
  layout-affecting metadata?
- Does this need both `shared_width` and `shared_height`, or only width for now?

## When to Pursue

Revisit when a second or third panel repeats the same workaround: a hand-picked
`fit_min(...)` value used only to align columns. At that point a shared-width
primitive would remove layout trial and error and make the intended relationship
visible in the tree.

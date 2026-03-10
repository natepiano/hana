# bevy_diegetic

Diegetic UI layout engine for [Bevy](https://bevyengine.org/), implemented in pure Rust.

Diegetic UI lives inside the game world — panels on surfaces, status displays on objects, HUDs that exist as physical things characters can see. This crate provides the layout engine and Bevy integration to build those interfaces.

## Inspiration

The layout algorithm is inspired by [Clay](https://github.com/nicbarker/clay), a C layout library designed for immediate-mode UI. Clay is excellent, but its C implementation uses global state that is fundamentally incompatible with Bevy's parallel ECS scheduler. Even with `Clay_SetCurrentContext`, concurrent FFI calls from different threads corrupt shared state, causing crashes ("out of bounds array access") when multiple systems run Clay layout passes in the same frame.

`bevy_diegetic` reimplements Clay's layout algorithm in pure Rust with none of these limitations:

- **No global state** — each `LayoutEngine` instance is fully self-contained.
- **No `unsafe`** — the entire crate is safe Rust.
- **Thread-safe** — multiple engines can run concurrently on different threads.
- **No FFI** — no C dependencies, no linking issues, no build complexity.

## Architecture

```
bevy_diegetic
├── layout/          Pure layout engine (no Bevy dependency in logic)
│   ├── types.rs     Core types: Sizing, Direction, Padding, BoundingBox, etc.
│   ├── element.rs   Arena-based element tree (LayoutTree, Element)
│   ├── builder.rs   Closure-based builder API (El, LayoutBuilder)
│   ├── engine.rs    Two-pass layout algorithm
│   └── render.rs    Render commands (Rectangle, Text, Border, Scissor)
│
└── plugin/          Bevy integration
    ├── components.rs  DiegeticPanel, ComputedDiegeticPanel, DiegeticTextMeasurer
    └── systems.rs     Layout computation + gizmo debug rendering
```

### Layout algorithm

The engine uses a two-pass approach:

1. **Sizing (BFS, top-down)** — Resolves element dimensions along each axis. Grow elements expand to fill available space using a smallest-first heuristic. Overflow is compressed using a largest-first heuristic.
2. **Positioning (DFS)** — Computes final bounding boxes and emits a flat list of render commands in draw order.

Before the BFS pass, a bottom-up propagation step initializes Fit containers from their children's accumulated sizes. Text leaves are measured via a pluggable `MeasureTextFn` callback.

Sizing rules: `Fixed`, `Grow`, `Fit`, `Percent` — each with optional min/max constraints.

## API

Build a layout tree with `El` and `LayoutBuilder`:

```rust
let mut builder = LayoutBuilder::new(160.0, 120.0);
builder.with(
    El::new()
        .width(Sizing::GROW)
        .height(Sizing::GROW)
        .padding(Padding::all(4.0))
        .direction(Direction::TopToBottom)
        .child_gap(4.0)
        .background(BackgroundColor::rgb(40, 44, 52)),
    |b| {
        b.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::fixed(20.0))
                .background(BackgroundColor::rgb(60, 130, 180)),
            |b| { b.text("STATUS", TextConfig::new(12)); },
        );
        b.with(
            El::new().width(Sizing::GROW).height(Sizing::GROW),
            |b| { b.text("Hello, world", TextConfig::new(10)); },
        );
    },
);
let tree = builder.build();
```

Use it as a Bevy component:

```rust
commands.spawn((
    DiegeticPanel {
        tree,
        layout_width: 160.0,
        layout_height: 120.0,
        world_width: 1.0,
        world_height: 0.75,
    },
    Transform::from_xyz(0.0, 1.0, -2.0),
));
```

The plugin handles layout computation automatically. Gizmo wireframes visualize the layout in 3D space for debugging.

## Completeness

### Implemented

- Full layout algorithm: `Fixed`, `Grow`, `Fit`, `Percent` sizing with min/max constraints
- Directional layout: `LeftToRight`, `TopToBottom`
- Padding, child gap, alignment (`AlignX`, `AlignY`)
- Overflow compression (largest-first) and growth expansion (smallest-first)
- Clipping regions (scissor start/end commands)
- Borders with between-children separators
- Background color on elements
- Pluggable text measurement callback (`MeasureTextFn`)
- Closure-based builder API (`El`, `LayoutBuilder`)
- Bevy plugin with automatic layout computation on change
- Gizmo debug renderer (layout-to-world coordinate transformation)
- Default monospace text measurement fallback
- 31 integration tests covering all layout primitives

### Placeholder / Minimal

- **Text measurement** — ships with a monospace approximation. Real usage requires injecting a `DiegeticTextMeasurer` resource backed by an actual text renderer (e.g. `bevy_rich_text3d`).
- **Rendering** — only gizmo wireframes for now. No mesh-based rectangles or rendered text.

## Future directions

- **Text rendering bridge** — integrate with `bevy_rich_text3d` for real text measurement and 3D text rendering on panels.
- **Mesh-based rendering** — generate actual meshes for rectangles, with materials and proper depth sorting, replacing gizmo wireframes for production use.
- **Interaction / picking** — hit-testing against layout bounding boxes for pointer events on panel elements.
- **Scroll containers** — overflow scrolling with scroll state and inertia.
- **Image elements** — display textures within layout elements.
- **Animation** — smooth transitions when layout properties change.
- **Retained layout diffing** — skip recomputation when the tree hasn't changed, diff render commands to minimize entity updates.
- **Builder ergonomics** — macro sugar or proc-macro for more concise tree construction if the closure-based API proves too verbose at scale.

## License

MIT OR Apache-2.0

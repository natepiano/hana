# Unit System DX — Document-Scale Design with World Scaling

## Context

The unit system (Phase 1) is functionally complete: `Unit` enum, `UnitConfig` with
`layout`/`font`/`world_font` defaults, pre-scale to points for parley, and per-panel
overrides. But the developer experience is poor. Working in meters for layout means
tiny floating-point numbers for padding (0.03), gaps (0.02), borders (0.002), and
either meter-scale font sizes (0.04) or a separate mm font unit with large numbers
(552mm). Neither feels like designing a document.

The goal: **work in a natural document coordinate space (points, mm, or inches) with
intuitive font sizes and layout dimensions, then scale the result to world size with
a single number.** Like SVG's `viewBox` → `width`/`height`, or CSS `@page` for print.

## Design

### Newtypes for dimensional values

```rust
pub struct Pt(pub f32);    // typographic points (1/72 inch)
pub struct Mm(pub f32);    // millimeters
pub struct In(pub f32);    // inches

impl From<Pt> for f32 { fn from(v: Pt) -> f32 { v.0 * Unit::Points.meters_per_unit() } }
impl From<Mm> for f32 { fn from(v: Mm) -> f32 { v.0 * Unit::Millimeters.meters_per_unit() } }
impl From<In> for f32 { fn from(v: In) -> f32 { v.0 * Unit::Inches.meters_per_unit() } }
```

Used internally by the builder and `PaperSize` — developers rarely need these directly.

### Standard paper sizes

```rust
pub enum PaperSize {
    A3,
    A4,
    A5,
    USLetter,
    USLegal,
    BusinessCard,
}
```

Implements `PanelSize` trait (see builder below) so it can be passed directly to `.size()`.

### Panel builder

Replaces direct struct construction. Eliminates the separate `LayoutBuilder::new(w, h)`
step — the builder knows the dimensions from `.size()` and passes them to the layout
closure automatically. No more specifying dimensions twice.

```rust
// Before (current API — dimensions specified twice, easy to mismatch):
let mut builder = LayoutBuilder::new(612.0, 792.0);
builder.with(El::new()..., |b| { ... });
let tree = builder.build();
commands.spawn(DiegeticPanel {
    tree,
    width: 612.0,
    height: 792.0,
    font_unit: Some(Unit::Points),
    ..default()
});

// After (builder API — dimensions specified once):
commands.spawn(
    DiegeticPanel::builder()
        .size(PaperSize::USLetter)
        .world_height(3.1)
        .layout(|b| {
            b.with(
                El::new()
                    .padding(Padding::all(24.0))
                    .direction(Direction::TopToBottom),
                |b| {
                    b.text("Hello", LayoutTextStyle::new(48.0));
                },
            );
        })
        .build()
);
```

The `.size()` method accepts anything implementing `PanelSize`:

```rust
pub trait PanelSize {
    fn dimensions(self) -> (f32, f32);
}

impl PanelSize for PaperSize { ... }
impl<W: Into<f32>, H: Into<f32>> PanelSize for (W, H) { ... }
```

Usage with different size sources:
```rust
.size(PaperSize::USLetter)              // standard paper size
.size((Pt(612.0), Pt(792.0)))           // explicit points
.size((Mm(210.0), Mm(297.0)))           // explicit mm
.size((In(3.5), In(2.0)))              // explicit inches
```

The `.layout(|b| { ... })` closure receives a `LayoutBuilder` pre-configured
with the panel dimensions. The tree is built internally — no separate build step.

### World scaling

Optional methods on the builder:

```rust
.world_height(3.1)   // uniform scale so height = 3.1m, width follows aspect ratio
.world_width(5.0)    // uniform scale so width = 5.0m, height follows aspect ratio
// both → non-uniform (explicit distortion)
// neither → physical size (points/mm/inches → meters at real scale)
```

The scale is computed once and applied at the rendering boundary — the layout engine
still works in layout units internally, pre-scaled to points for parley.

### Other builder methods

```rust
.font_unit(Unit::Points)     // override font unit (default inherits from UnitConfig)
.layout_unit(Unit::Points)   // override layout unit (default inherits from UnitConfig)
.anchor(Anchor::Center)      // override anchor (default TopLeft)
```

### How font_unit interacts

When using `.size(PaperSize::USLetter)` with `layout_unit: Points`, font sizes in
`LayoutTextStyle::new()` are in points too (default `font_unit: Points`). Both in
points means `font_scale = 1.0` — no conversion. The developer works entirely in
one unit: 24pt font, 12pt padding, 2pt border, 612×792pt page.

When layout is meters (via newtypes) and font is points (default), the pre-scale
to points handles the conversion transparently.

### Example: font_features showcase

```rust
commands.spawn(
    DiegeticPanel::builder()
        .size(PaperSize::USLetter)
        .layout_unit(Unit::Points)
        .world_height(3.1)
        .layout(|b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .padding(Padding::all(18.0))        // 18pt
                    .direction(Direction::TopToBottom)
                    .child_gap(12.0)                     // 12pt
                    .border(Border::all(2.0, border_color)),  // 2pt
                |b| {
                    b.text("Font Features",
                        LayoutTextStyle::new(36.0)       // 36pt
                            .with_color(section_color));
                    build_feature_grid(b, ...);
                },
            );
        })
        .build()
);
```

All values in points. `world_height(3.1)` scales the 792pt page to 3.1 meters.

### Example: A4 info panel in mm

```rust
commands.spawn(
    DiegeticPanel::builder()
        .size(PaperSize::A4)
        .layout_unit(Unit::Millimeters)
        .world_height(0.5)
        .layout(|b| {
            b.with(El::new()..., |b| { ... });
        })
        .build()
);
```

### Example: business card at real size (no scaling)

```rust
commands.spawn(
    DiegeticPanel::builder()
        .size((In(3.5), In(2.0)))
        .layout_unit(Unit::Inches)
        .layout(|b| { ... })
        .build()
);
```

No `world_height` — renders at physical size (89mm × 51mm).

## Implementation

### Step 1: Add newtypes (`Pt`, `Mm`, `In`)

**File:** `src/plugin/config.rs` (alongside `Unit`)

Simple tuple structs with `From<T> for f32` impls that convert to meters.
Export from `src/lib.rs`.

### Step 2: Add `PanelSize` trait and `PaperSize` enum

**File:** `src/plugin/config.rs` (or new `src/plugin/paper.rs`)

`PanelSize` trait with `fn dimensions(self) -> (f32, f32)`.
Implement for `PaperSize`, `(W, H) where W: Into<f32>, H: Into<f32>`.

### Step 3: Add `DiegeticPanelBuilder`

**File:** `src/plugin/components.rs`

Builder struct with:
- `.size(impl PanelSize)`
- `.layout(impl FnOnce(&mut LayoutBuilder))`
- `.world_width(f32)` / `.world_height(f32)`
- `.layout_unit(Unit)` / `.font_unit(Unit)` / `.anchor(Anchor)`
- `.build() -> DiegeticPanel`

The `.layout()` closure receives a `LayoutBuilder` pre-sized from `.size()`.
The builder calls `LayoutBuilder::new(w, h)` internally and stores the tree.

`DiegeticPanel::builder()` returns the builder.

### Step 4: Add `world_width` / `world_height` to `DiegeticPanel`

**File:** `src/plugin/components.rs`

Both `Option<f32>` defaulting to `None`. Update `world_width()` and
`world_height()` methods to use them when set, computing uniform scale
from whichever is provided.

### Step 5: Update rendering boundary

**Files:** `src/plugin/systems.rs`, `src/render/text_renderer.rs`

Where `anchor_offsets` / scale factors are computed from panel dimensions,
use the new `world_width()` / `world_height()` methods which incorporate
the scaling.

### Step 6: Update examples

Convert `font_features.rs` first as the proof-of-concept: design in
points at US Letter size, scale to 3.1m. Then migrate other examples
that benefit from document-scale design.

### Step 7: Update `units.rs` example

Add a third panel demonstrating the world_scale workflow: design a
poster in points, scale to scene size.

## Verification

1. `cargo build && cargo +nightly fmt`
2. `cargo nextest run`
3. `cargo run --example font_features` — panel renders at 3.1m with readable
   text, font sizes are standard point values (24, 48, etc.)
4. `cargo run --example units` — existing mm/inches panels still work
5. Spot-check other examples

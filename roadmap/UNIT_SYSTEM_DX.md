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

Usage at panel construction:
```rust
DiegeticPanel {
    width: Pt(612.0).into(),   // US Letter width in points
    height: Pt(792.0).into(),  // US Letter height in points
    ..default()
}
```

Or millimeters:
```rust
DiegeticPanel {
    width: Mm(210.0).into(),   // A4 width
    height: Mm(297.0).into(),  // A4 height
    ..default()
}
```

The newtypes convert to meters (the default layout unit) at construction time.
No `layout_unit` override needed — the conversion is explicit in the code.

### Standard paper sizes

```rust
pub enum PaperSize {
    A3,
    A4,
    A5,
    USLetter,
    USLegal,
    BusinessCard,
    // ...
}

impl PaperSize {
    /// Width in meters (shorter dimension).
    pub const fn width(self) -> f32;
    /// Height in meters (longer dimension).
    pub const fn height(self) -> f32;
    /// Width in the given unit.
    pub fn width_in(self, unit: Unit) -> f32;
    /// Height in the given unit.
    pub fn height_in(self, unit: Unit) -> f32;
}
```

Usage:
```rust
DiegeticPanel {
    width: PaperSize::A4.width(),
    height: PaperSize::A4.height(),
    ..default()
}
```

### World scaling

Two optional fields on `DiegeticPanel`:

```rust
pub struct DiegeticPanel {
    pub tree: LayoutTree,
    pub width: f32,
    pub height: f32,
    pub layout_unit: Option<Unit>,
    pub font_unit: Option<Unit>,
    pub anchor: Anchor,
    pub world_width: Option<f32>,   // NEW — target world width in meters
    pub world_height: Option<f32>,  // NEW — target world height in meters
}
```

Behavior:
- **Neither set**: physical size (`width × layout_unit.meters_per_unit()`)
- **`world_height` only**: uniform scale so height matches, width follows aspect ratio
- **`world_width` only**: uniform scale so width matches, height follows aspect ratio
- **Both set**: non-uniform scale (explicit distortion, user's choice)

The scale is computed once and applied at the rendering boundary — the layout engine
still works in layout units internally, pre-scaled to points for parley.

### How font_unit interacts

With `layout_unit: Points` (or default Meters + Pt newtypes), font sizes in
`LayoutTextStyle::new()` are in whatever `font_unit` says (default: Points).
When layout and font are both in points, `font_scale = 1.0` — no conversion.
The developer works entirely in one unit.

When layout is meters (via newtypes) and font is points (default), the pre-scale
to points handles the conversion transparently.

### Example: font_features showcase

```rust
// Design as a US Letter document in points
let mut builder = LayoutBuilder::new(
    PaperSize::USLetter.width_in(Unit::Points),  // 612
    PaperSize::USLetter.height_in(Unit::Points),  // 792
);

DiegeticPanel {
    tree: builder.build(),
    width: PaperSize::USLetter.width_in(Unit::Points),
    height: PaperSize::USLetter.height_in(Unit::Points),
    layout_unit: Some(Unit::Points),
    // font_unit inherits Points — same unit, no conversion
    world_height: Some(3.1),  // scale to 3.1m in the scene
    ..default()
}
```

Font sizes: `LayoutTextStyle::new(48.0)` = 48pt title.
Padding: `Padding::all(24.0)` = 24pt = 1/3 inch.
Border: `Border::all(2.0, color)` = 2pt border.
All natural numbers in a natural coordinate space.

### Example: A4 info panel in mm

```rust
DiegeticPanel {
    width: Mm(210.0).into(),
    height: Mm(297.0).into(),
    layout_unit: Some(Unit::Millimeters),
    world_height: Some(0.5),  // scale to 50cm in the scene
    ..default()
}
```

### Example: business card at real size (no scaling)

```rust
DiegeticPanel {
    width: In(3.5).into(),
    height: In(2.0).into(),
    layout_unit: Some(Unit::Inches),
    // No world_width/world_height — renders at physical size (89mm × 51mm)
    ..default()
}
```

## Implementation

### Step 1: Add newtypes (`Pt`, `Mm`, `In`)

**File:** `src/plugin/config.rs` (alongside `Unit`)

Simple tuple structs with `From<T> for f32` impls that convert to meters.
Export from `src/lib.rs`.

### Step 2: Add `PaperSize` enum

**File:** `src/plugin/config.rs` (or new `src/plugin/paper.rs`)

Standard sizes with `width()`/`height()` returning meters, and
`width_in(unit)`/`height_in(unit)` for other units.

### Step 3: Add `world_width` / `world_height` to `DiegeticPanel`

**File:** `src/plugin/components.rs`

Both `Option<f32>` defaulting to `None`. Update `world_width()` and
`world_height()` methods to use them when set, computing uniform scale
from whichever is provided.

### Step 4: Update rendering boundary

**Files:** `src/plugin/systems.rs`, `src/render/text_renderer.rs`

Where `half_w` / `half_h` are computed from panel dimensions, use the
new `world_width()` / `world_height()` methods which incorporate the
scaling.

### Step 5: Update examples

Convert `font_features.rs` first as the proof-of-concept: design in
points at US Letter size, scale to 3.1m. Then migrate other examples
that benefit from document-scale design.

### Step 6: Update `units.rs` example

Add a third panel demonstrating the world_scale workflow: design a
poster in points, scale to scene size.

## Verification

1. `cargo build && cargo +nightly fmt`
2. `cargo nextest run`
3. `cargo run --example font_features` — panel renders at 3.1m with readable
   text, font sizes are standard point values (24, 48, etc.)
4. `cargo run --example units` — existing mm/inches panels still work
5. Spot-check other examples

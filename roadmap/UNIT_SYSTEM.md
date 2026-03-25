# Panel Unit System

## Context

`DiegeticPanel` currently requires manual specification of both layout dimensions and world
dimensions (`layout_width`, `layout_height`, `world_width`, `world_height`), and font sizes
must be in the same units as the layout. This makes it impossible to say "24pt font on a
210×297mm panel" — the user must manually convert points to millimeters.

This replaces the earlier `TextScale` / `TextScaleOverride` / `METERS_PER_POINT` approach
(see [PHYSICAL_FONT_SIZING.md](PHYSICAL_FONT_SIZING.md)) with a proper unit system.

**Goal:** Let users specify panel dimensions in whatever spatial unit they prefer (meters, mm,
inches) and font sizes in points (the universal standard), with automatic conversion. Defaults
are physically correct: 1 world unit = 1 meter, fonts in points.

## Design

### `Unit` enum (`src/plugin/config.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum Unit {
    Meters,          // 1.0 m/unit (Bevy default)
    Millimeters,     // 0.001 m/unit
    Points,          // 0.0254 / 72.0 m/unit ≈ 0.000353
    Inches,          // 0.0254 m/unit
    Custom(f32),     // arbitrary meters-per-unit
}

impl Unit {
    pub const fn meters_per_unit(self) -> f32;
}
```

Single enum for both layout and font units. `Custom(f32)` is the escape hatch for
centimeters, picas, or legacy `TextScale(0.01)` equivalents.

### `UnitConfig` resource (`src/plugin/config.rs`)

```rust
#[derive(Resource, Clone, Copy, Debug, Reflect)]
pub struct UnitConfig {
    pub layout: Unit,  // default: Meters
    pub font: Unit,    // default: Points
}
```

Global defaults. Inserted by the plugin. Replaces `TextScale`.

### `DiegeticPanel` refactor (`src/plugin/components.rs`)

```rust
pub struct DiegeticPanel {
    pub tree: LayoutTree,
    pub width: f32,           // in layout units
    pub height: f32,          // in layout units
    pub layout_unit: Option<Unit>,  // None → inherit from UnitConfig
    pub font_unit: Option<Unit>,    // None → inherit from UnitConfig
}
```

Replaces `layout_width`/`layout_height`/`world_width`/`world_height`. World dimensions
become computed:
- `world_width = width × resolved_layout_unit.meters_per_unit()`
- `world_height = height × resolved_layout_unit.meters_per_unit()`

### Font scale conversion

The layout engine works in layout units. Font sizes specified in font units get converted:
```
font_scale = font_unit.meters_per_unit() / layout_unit.meters_per_unit()
```

Example — A4 panel with mm layout, pt fonts:
- `font_scale = 0.000353 / 0.001 = 0.353`
- 24pt → `24 × 0.353 = 8.47mm` in layout units
- Layout engine wraps 8.47mm text in a 210mm-wide panel

When `font_unit == layout_unit`, `font_scale = 1.0` (no conversion).

## Implementation

### Step 1: Add `Unit` and `UnitConfig` types

**File:** `src/plugin/config.rs`

- Add `Unit` enum with `meters_per_unit()` method
- Add `UnitConfig` resource with `Default` impl (layout: Meters, font: Points)
- Add `UnitConfig::font_scale(&self) -> f32` convenience method
- Remove `TextScale` resource

### Step 2: Refactor `DiegeticPanel`

**File:** `src/plugin/components.rs`

- Replace `layout_width`/`layout_height`/`world_width`/`world_height` with
  `width`/`height`/`layout_unit`/`font_unit`
- Add `DiegeticPanel::resolved_layout_unit(&self, config: &UnitConfig) -> Unit`
- Add `DiegeticPanel::resolved_font_unit(&self, config: &UnitConfig) -> Unit`
- Add `DiegeticPanel::world_width(&self, config: &UnitConfig) -> f32` (computed)
- Add `DiegeticPanel::world_height(&self, config: &UnitConfig) -> f32` (computed)
- Add `DiegeticPanel::font_scale(&self, config: &UnitConfig) -> f32` (computed)
- Remove `TextScaleOverride` component

### Step 3: Add font scaling to layout engine

**File:** `src/layout/types.rs`
- Add `TextMeasure::scaled(self, factor: f32) -> Self` — multiplies `size`, `line_height`,
  `letter_spacing`, `word_spacing`
- Add `LayoutTextStyle::scaled(&self, factor: f32) -> Self` — same fields, returns new
  instance

**File:** `src/layout/engine.rs`
- Add `font_scale: f32` parameter to `compute()`
- In `initialize_leaf_sizes`: use `config.as_measure().scaled(font_scale)` for measurement
- In `rewrap_text_elements` / `wrap_text_words` / `wrap_text_newlines`: pass `font_scale`
  through, scale the `TextMeasure` before measuring
- In render command emission: use `config.scaled(font_scale)` so render commands have font
  sizes in layout units

### Step 4: Update panel systems

**File:** `src/plugin/systems.rs`
- `compute_panel_layouts`: resolve units from `UnitConfig` + panel overrides, compute
  `font_scale`, pass to `engine.compute()`, compute world dimensions for
  `computed.set_content_size()`
- `render_panel_gizmos`: compute scale from `UnitConfig` instead of
  `world_width / layout_width`

### Step 5: Update panel text renderer

**File:** `src/render/text_renderer.rs`
- `reconcile_panel_text_children`: compute scale from resolved units instead of
  `world_width / layout_width`
- `shape_panel_text_children`: same — `scale_x`/`scale_y` from `meters_per_unit`
- `build_panel_batched_meshes`: same
- Remove `TextScaleOverride` from all panel queries

### Step 6: Update standalone `WorldText` rendering

**File:** `src/render/world_text.rs`
- Replace `TextScale` resource with `UnitConfig` in queries
- Scale computation: `unit_config.font.meters_per_unit()` replaces `text_scale.0`
- Remove `TextScaleOverride` from queries

### Step 7: Update typography overlay

**File:** `src/debug/typography_overlay.rs`
- Replace `TextScale` + `TextScaleOverride` with `UnitConfig`
- Scale = `unit_config.font.meters_per_unit()` for standalone text
- For panel text children: scale from panel's resolved layout unit

### Step 8: Update plugin configuration

**File:** `src/plugin/mod.rs`
- Replace `text_scale` field/method with `unit_config` on `DiegeticUiPluginConfigured`
- Insert `UnitConfig` resource instead of `TextScale`
- Register `Unit` and `UnitConfig` for reflection

### Step 9: Remove obsolete types

- `src/plugin/config.rs`: remove `TextScale`
- `src/plugin/components.rs`: remove `TextScaleOverride`
- `src/render/constants.rs`: remove `METERS_PER_POINT` (encoded in `Unit::Points`)
- `src/render/mod.rs`: remove `METERS_PER_POINT` export
- `src/lib.rs`: remove `TextScale`, `TextScaleOverride`, `METERS_PER_POINT` exports; add
  `Unit`, `UnitConfig`

### Step 10: Update `text_scale` example

**File:** `examples/text_scale.rs`

Rewrite to showcase the unit system:
- **A4 page** (left): `width: 210.0, height: 297.0, layout_unit: Some(Unit::Millimeters)`,
  font sizes in points (24pt title, 12pt body, 9pt caption)
- **US business card** (right): `width: 3.5, height: 2.0, layout_unit: Some(Unit::Inches)`,
  font sizes in points (12pt name, 9pt details)
- Both inherit `font_unit: Points` from global `UnitConfig`
- Display values showing the unit configuration and computed world dimensions
- Real physical sizes — no artificial scaling

### Step 11: Update all other examples

Each example that used `TextScale(0.01)` migrates to
`UnitConfig { font: Unit::Custom(0.01), layout: Unit::Meters }` for behavioral equivalence.
Examples with explicit `world_width`/`world_height` migrate to `width`/`height` with
appropriate `layout_unit`.

**WorldText-only examples** (use custom font scale):
- `atlas_pages.rs`, `font_loading.rs`, `world_text.rs`, `shadows.rs`, `preload_text.rs`

**Panel examples** (migrate fields):
- `text_panel.rs`, `typography.rs`, `font_features.rs`, `minimal_eb.rs`, `hue_offset.rs`,
  `side_by_side.rs`, `text_stress.rs`

### Step 12: Update tests and benchmarks

- `src/layout/layout_tests.rs`: pass `font_scale: 1.0` to `engine.compute()`
- `src/layout/clay_parity_tests.rs`: same
- `benches/layout_comparison.rs`, `benches/panel_perf.rs`: same

## Files to modify

1. `src/plugin/config.rs` — add `Unit`, `UnitConfig`; remove `TextScale`
2. `src/plugin/components.rs` — refactor `DiegeticPanel`; remove `TextScaleOverride`
3. `src/layout/types.rs` — add `TextMeasure::scaled()`, `LayoutTextStyle::scaled()`
4. `src/layout/engine.rs` — add `font_scale` to `compute()` and internal functions
5. `src/plugin/systems.rs` — unit-aware scale computation
6. `src/render/text_renderer.rs` — unit-aware panel rendering
7. `src/render/world_text.rs` — `UnitConfig` replaces `TextScale`
8. `src/debug/typography_overlay.rs` — `UnitConfig` replaces `TextScale`/`TextScaleOverride`
9. `src/plugin/mod.rs` — `UnitConfig` plugin configuration
10. `src/render/constants.rs` — remove `METERS_PER_POINT`
11. `src/render/mod.rs` — update exports
12. `src/lib.rs` — update public API exports
13. `examples/text_scale.rs` — showcase example rewrite
14. All other examples (11 files) — field migration
15. `src/layout/layout_tests.rs` — pass `font_scale: 1.0`
16. `src/layout/clay_parity_tests.rs` — pass `font_scale: 1.0`
17. `benches/layout_comparison.rs` — pass `font_scale: 1.0`
18. `benches/panel_perf.rs` — pass `font_scale: 1.0`

## Verification

1. `cargo build && cargo +nightly fmt`
2. `cargo nextest run` — all tests pass
3. `cargo run --example text_scale` — A4 page in mm with pt fonts, business card in inches
   with pt fonts, correct physical sizes
4. `cargo run --example typography` — backward compatible rendering (Custom(0.01) font scale)
5. `cargo run --example text_panel` — panels render correctly with migrated fields
6. Spot-check 2-3 other examples

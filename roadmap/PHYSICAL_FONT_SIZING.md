# Physical Font Sizing

## Context

Font "size" in the system is in abstract layout units with a hardcoded `0.01` scale
factor converting to world units (`world_text.rs:325`). Size=12 produces text 0.12m
tall — about 28x larger than physical 12pt type (4.23mm). The goal is to make the
default physically accurate (1 point = 1/72 inch), while allowing users to override
the scale globally and per-entity for "worlds within worlds" scenarios.

The guiding principle: if you specify a 12pt font on a `WorldText`, it should be the
same size as 12pt text on a printed page. A 72pt letter placed on a mesh of a 3x5
index card should look the same as it would on that card in the real world.

## The Conversion Constant

```
METERS_PER_POINT = 0.0254 / 72.0 ≈ 0.000_352_778
```

- 72pt text → 1 inch (0.0254m) em-square
- 12pt text → 4.23mm em-square
- Assumes Bevy convention: 1 world unit = 1 meter

## Design

### New types

| Type | Kind | Location | Purpose |
|------|------|----------|---------|
| `METERS_PER_POINT` | Constant | `src/render/constants.rs` (new file) | Physical conversion factor |
| `TextScale` | Resource | `src/plugin/config.rs` | Global scale for `WorldText` entities |
| `TextScaleOverride` | Component | `src/plugin/components.rs` | Per-entity multiplier |

### `TextScale` resource

Global scale factor for `WorldText` entities. Does NOT apply to `DiegeticPanel`
(panels already have `world_width / layout_width`).

```rust
#[derive(Resource, Clone, Copy, Debug, Reflect)]
pub struct TextScale(pub f32);
// Default: METERS_PER_POINT (physically accurate)
// TextScale(0.01) restores old behavior
```

### `TextScaleOverride` component

Per-entity multiplier applied on top of the effective scale. Works on both
`WorldText` and `DiegeticPanel` entities.

```rust
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct TextScaleOverride(pub f32);
// Default: 1.0 (neutral multiplier)
```

**Final scale computation:**
- `WorldText`: `TextScale.0 * TextScaleOverride.0`
- `DiegeticPanel`: `(world_width / layout_width) * TextScaleOverride.0`

Entities without `TextScaleOverride` behave as if it were `1.0`.

### Plugin builder API

```rust
DiegeticUiPlugin::with_atlas()
    .quality(RasterQuality::High)
    .text_scale(TextScale(0.01))  // <-- new
```

Add `text_scale: Option<TextScale>` to `DiegeticUiPluginConfigured`.
In `build_plugin()`: insert `TextScale` resource (defaults to physical if unset).

## Implementation steps

### 1. Add `METERS_PER_POINT` constant

**New file**: `src/render/constants.rs`

Wire up in `src/render/mod.rs`: `mod constants;` + `pub use constants::METERS_PER_POINT;`

### 2. Add `TextScale` resource

**File**: `src/plugin/config.rs` (alongside `AtlasConfig`, `RasterQuality`)

### 3. Add `TextScaleOverride` component

**File**: `src/plugin/components.rs` (alongside `DiegeticPanel`, `HueOffset`)

### 4. Plugin builder + registration

**File**: `src/plugin/mod.rs`

- Add `text_scale` field to `DiegeticUiPluginConfigured`
- Add `.text_scale()` builder method
- In `build_plugin()`: insert resource, register both types with Bevy reflection

### 5. Update `WorldText` rendering

**File**: `src/render/world_text.rs`

- `render_world_text` system: add `Res<TextScale>` param, add `Option<&TextScaleOverride>`
  to both queries, add `Changed<TextScaleOverride>` to the `Or` filter
- Pass resolved scale to `shape_world_text`
- `shape_world_text`: add `scale: f32` parameter, remove hardcoded `let scale = 0.01_f32;`
  on line 325

### 6. Update `DiegeticPanel` rendering

**File**: `src/render/text_renderer.rs`

- `extract_text_meshes` system: add `Option<&TextScaleOverride>` to both panel queries
- Multiply existing panel scale by the override:
  ```rust
  let override_mult = text_scale_override.map_or(1.0, |o| o.0);
  let scale_x = panel.world_width / panel.layout_width * override_mult;
  let scale_y = panel.world_height / panel.layout_height * override_mult;
  ```

### 7. Update typography overlay

**File**: `src/debug/typography_overlay.rs`

- Remove `const LAYOUT_TO_WORLD: f32 = 0.01;`
- Add `Res<TextScale>`, query `Option<&TextScaleOverride>` on entities
- Compute dynamic scale, thread through helper functions
- Scale hardcoded world-unit nudge offsets proportionally

### 8. Public exports

**`src/plugin/mod.rs`**: `pub use config::TextScale;` + `pub use components::TextScaleOverride;`

**`src/lib.rs`**: Add exports for `TextScale`, `TextScaleOverride`, `METERS_PER_POINT`

### 9. Update examples

All `WorldText` examples render ~28x smaller with the new default.

| Example | Change |
|---------|--------|
| `world_text.rs` | Demonstrate physical sizing (adjust camera/sizes) |
| `typography.rs` | `TextScale(0.01)` for compatibility |
| `shadows.rs` | `TextScale(0.01)` for compatibility |
| `atlas_pages.rs` | `TextScale(0.01)` for compatibility |
| `font_loading.rs` | `TextScale(0.01)` for compatibility |
| `preload_text.rs` | `TextScale(0.01)` for compatibility |
| `side_by_side.rs` | Replace `WORLD_TEXT_SCALE` const with `METERS_PER_POINT` or resource |
| `text_stress.rs` | No change (panel scale, not WorldText) |
| `text_panel.rs` | No change (panel-only) |
| `hue_offset.rs` | No change (panel-only) |

## Files touched

| File | Action |
|------|--------|
| `src/render/constants.rs` | **New** — `METERS_PER_POINT` |
| `src/render/mod.rs` | Add module + re-export |
| `src/plugin/config.rs` | Add `TextScale` resource |
| `src/plugin/components.rs` | Add `TextScaleOverride` component |
| `src/plugin/mod.rs` | Exports, builder, `build_plugin` |
| `src/lib.rs` | Public exports |
| `src/render/world_text.rs` | Replace hardcoded scale with resource + override |
| `src/render/text_renderer.rs` | Add optional `TextScaleOverride` to panel rendering |
| `src/debug/typography_overlay.rs` | Replace `LAYOUT_TO_WORLD` with dynamic scale |
| 6+ examples | Scale compatibility or physical demo |

## Verification

1. `cargo build && cargo +nightly fmt`
2. `cargo nextest run`
3. Run `world_text` example — 72pt text should match a 0.0254m reference cube
4. Run examples with `TextScale(0.01)` — no visual regression
5. Spawn two `WorldText` entities, one with `TextScaleOverride(2.0)` — verify 2x size
6. Run `typography` example — metric lines align with rendered text

# Typography Example — Implementation Plan

## Context

Build an interactive typography metrics visualizer that renders a large word using `WorldText`, draws annotation lines showing parley's typography metrics (ascent, descent, baseline, leading, line height, top, bottom), and pairs it with an inspector `DiegeticPanel` showing the live metric values. The whole scene floats above the ground plane.

## Architecture

Three visual elements floating in space:

1. **Display Word** — Large `WorldText` showing "Typography" (anchored `CenterLeft` so we know the origin)
2. **Annotation Lines** — Bevy gizmos drawn each frame showing the metric lines over the word
3. **Inspector Panel** — `DiegeticPanel` beside the word showing metric names, values, and color-coded labels matching the annotation lines

## Parley Metrics Available

From `parley::layout::LineMetrics` (accessed via `layout.lines().metrics()`):

| Field | Type | Description |
|---|---|---|
| `ascent` | f32 | Typographic ascent (baseline to top of tallest glyph) |
| `descent` | f32 | Typographic descent (baseline to bottom of descenders) |
| `leading` | f32 | Typographic leading (font-defined line gap) |
| `line_height` | f32 | Absolute line height (the full line box) |
| `baseline` | f32 | Offset to the baseline |
| `min_coord` | f32 | Top of line box (baseline - ascent - half-leading) |
| `max_coord` | f32 | Bottom of line box (baseline + descent + half-leading) |
| `advance` | f32 | Full horizontal advance including trailing whitespace |

## Implementation Steps

### Step 1: Expose Parley LineMetrics from the Library

Currently `shape_text_cached` in `src/render/text_renderer.rs` iterates `layout.lines()` but only reads glyph positions. We need to also extract and return `LineMetrics`.

- Create a new struct `LineMetricsSnapshot` that captures the parley `LineMetrics` fields we care about (ascent, descent, leading, line_height, baseline, min_coord, max_coord, advance).
- Add a new public function (e.g., `query_text_metrics`) that takes text + `TextConfig` + font registry access and returns `Vec<LineMetricsSnapshot>`. This keeps the shaping cache path clean while giving the example access to parley metrics.
- Re-export from `src/lib.rs`.

### Step 2: Build the Example Scene (`examples/typography.rs`)

**Setup:**
- Add `DiegeticUiPlugin` to plugins
- Keep ground plane, light, camera
- Position camera to frame the text nicely

**Display Word:**
- Spawn a `WorldText::new("Typography")` with large size (e.g., 40.0-60.0 layout units)
- Use `TextStyle::new().with_size(DISPLAY_SIZE).with_color(WHITE).with_anchor(TextAnchor::BottomLeft)`
- Position floating above ground plane (~1.5 Y)

**Metric Query System:**
- On startup (or when config changes), call the new metrics query function with the display text + config
- Store the resulting `LineMetricsSnapshot` in a resource (`TypographyMetrics`)

**Annotation Lines (Gizmos):**
- System runs each frame, reads `TypographyMetrics` resource
- For each metric, draw a colored horizontal line at the appropriate Y position across the text width
- Scale from layout units to world units using the same `1 layout unit = 0.01 world units` factor from `WorldText`
- Lines extend slightly beyond text bounds for readability
- Color scheme (distinct, readable):
  - **Top (min_coord)** — red
  - **Ascent** — blue
  - **Baseline** — white (thicker/prominent)
  - **Descent** — green
  - **Bottom (max_coord)** — orange
  - **Line height** — bracket/brace on the side

**Inspector Panel:**
- `DiegeticPanel` positioned to the right of the display word
- Dark background, no child gaps or padding — use `Border::between_children` with thin (0.5) lines as separators
- Each row: `[metric name] [value]` with color matching the annotation line
- Rows for: Top, Ascent, Baseline, Descent, Bottom, Line Height, Leading, Advance

### Step 3: Interactive Controls

- Keyboard controls to adjust parley-configurable values:
  - **Up/Down arrows** — adjust `line_height`
  - **Left/Right arrows** — adjust `letter_spacing`
  - **+/-** — adjust `font_size`
- When values change, update the `WorldText`'s `TextStyle`, re-query metrics, rebuild inspector panel
- A small controls hint panel (bottom corner) showing available keys

### Step 4: Grid Background

- Spawn a subtle grid mesh or use gizmo grid behind the display word
- High alpha (transparent) so it doesn't dominate but gives visual reference for spacing

## Key Files to Modify

| File | Change |
|---|---|
| `examples/typography.rs` | New example (main implementation) |
| `src/render/text_renderer.rs` | Expose line metrics query function |
| `src/lib.rs` | Re-export the new metrics types |

## Verification

1. `cargo build --example typography` compiles
2. `cargo run --example typography` shows:
   - Large "Typography" word floating above ground
   - Colored horizontal lines at each metric position
   - Inspector panel to the right with labeled values
   - Arrow keys adjust spacing/size and annotations update
3. `cargo +nightly fmt` passes
4. `cargo clippy` clean

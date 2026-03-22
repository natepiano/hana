# Typography — Implementation Plan

## Branch Info

This work was branched from `feature/text-rendering`. When complete, merge back into `feature/text-rendering`.

## Context

Build a feature-gated typography debug overlay into the library that can be attached to any `WorldText` entity. The overlay renders font-level metric lines (ascent, descent, cap height, x-height, baseline, line gap) and optionally per-glyph metrics (advance width, bounding boxes) as gizmos. A typography example demonstrates the overlay.

## Terminology

| Concept | Apple Diagram | Medium Diagram | Our Name | Level | Source |
|---|---|---|---|---|---|
| Top of line box | *(not labeled)* | **Top** | `top` | Layout | `baseline - ascent - half_leading` (parley `min_coord`) |
| Tallest glyph reach | **Ascent** | **Ascent** | `ascent` | Font | OS/2 or hhea table |
| Lowercase letter height | **X-height** | **Mean line** | `x_height` | Font | OS/2 table; fallback: measure 'x' glyph bbox |
| Capital letter height | **Cap height** | *(not shown)* | `cap_height` | Font | OS/2 table; fallback: measure 'H' glyph bbox |
| Where letters sit | **Baseline** | **Baseline** | `baseline` | Layout | Offset computed by parley per line |
| Below-baseline depth | **Descent** | **Descent** | `descent` | Font | OS/2 or hhea table (positive = below baseline) |
| Bottom of line box | *(not labeled)* | **Bottom** | `bottom` | Layout | `baseline + descent + half_leading` (parley `max_coord`) |
| Inter-line spacing | **Line gap (leading)** | **Leading** | `line_gap` | Font | OS/2 or hhea table. Also called "leading" in typography |
| Full line measure | **Line height** | **Line height** | `line_height` | Font | `ascent + descent + line_gap` |
| Horizontal glyph width | **Advancement** | *(not shown)* | `advance_width` | Glyph | Per-glyph horizontal advance |
| Glyph extents box | **Bounding rectangle** | *(not shown)* | `bounds` | Glyph | Per-glyph bounding rect (xmin, ymin, xmax, ymax) |
| Glyph reference point | **Origin** | *(not shown)* | `bearing_x` / `bearing_y` | Glyph | Per-glyph bearing offsets |
| Slant angle | **Italic angle** | *(not shown)* | `slant_angle` | Font | post table; default 0.0 for upright |

**References:**
- https://developer.apple.com/library/mac/documentation/TextFonts/Conceptual/CocoaTextArchitecture/Art/glyph_metrics_2x.png
- https://miro.medium.com/v2/resize:fit:1100/format:webp/1*v1FDlH-vFEnhXDFDxp6OXg.png

**Notes:**
- All `FontMetrics` fields are non-optional. When the font's OS/2 table lacks `x_height` or `cap_height`, we derive them by measuring the bounding box of the 'x' or 'H' glyph respectively. `slant_angle` defaults to 0.0 if missing.
- `line_gap` is the canonical name in our API. Doc comments note it is also called "leading" in traditional typography.
- `ascent` vs `cap_height`: ascent is the font's full ascender line (includes room for accented characters like Â, É). Cap height is just the top of unadorned uppercase letters (H, E, T). Ascent >= cap height always.
- `line_height = ascent + descent + line_gap` — both Apple and Medium diagrams agree on this formula. Medium distributes half the line_gap above and half below (shown as top/bottom), while Apple shows it as a single measurement.

## Three Levels of Metrics

### Font-level metrics (`FontMetrics`)

Pre-parsed once at font registration from OS/2, hhea, and post tables. Scaled to any size via `Font::metrics(size)` — pure arithmetic.

| Field | Description |
|---|---|
| `ascent` | Baseline → ascender line |
| `descent` | Baseline → descender line (positive = below baseline) |
| `line_gap` | Font-recommended inter-line spacing (also called "leading") |
| `line_height` | `ascent + descent + line_gap` |
| `x_height` | Baseline → mean line (height of lowercase 'x') |
| `cap_height` | Baseline → cap line (height of uppercase 'H') |
| `slant_angle` | Degrees from vertical (0 for upright) |
| `underline_position` | Distance below baseline |
| `underline_thickness` | Stroke thickness |
| `strikeout_position` | Distance above baseline |
| `strikeout_thickness` | Stroke thickness |
| `font_size` | Size these metrics were computed for |
| `units_per_em` | Raw design units per em |

### Layout-level metrics (`LineMetricsSnapshot`)

Computed by parley for a specific text run via `layout.lines().metrics()`:

| Field | Description |
|---|---|
| `top` | Top of line box (parley `min_coord`) |
| `ascent` | Typographic ascent for this line |
| `baseline` | Offset to the baseline |
| `descent` | Typographic descent for this line |
| `bottom` | Bottom of line box (parley `max_coord`) |
| `leading` | Line gap as computed for this layout |
| `line_height` | Absolute line height |
| `advance` | Full horizontal advance including trailing whitespace |

### Per-glyph metrics (`GlyphTypographyMetrics`)

Computed on the fly from font bytes — only when `TypographyOverlay` is active with `show_glyph_metrics: true`. Never stored persistently.

| Field | Description |
|---|---|
| `advance_width` | Horizontal advance (Apple's "Advancement") |
| `bounds` | Bounding rectangle (xmin, ymin, xmax, ymax) |
| `bearing_x` | Left side bearing (origin to left edge) |
| `bearing_y` | Top side bearing (baseline to top edge) |

## Implementation Steps

### Step 1: Create `Font` and `FontMetrics` structs

**New file:** `src/text/font.rs`

`Font` pre-parses raw design-unit metrics from the font file at creation time using `ttf_parser::Face`. Also stores font data bytes (`Arc<[u8]>`) for on-demand per-glyph metric queries.

```rust
pub struct Font {
    name: String,
    data: Arc<[u8]>,           // retained for per-glyph queries
    units_per_em: u16,
    raw_ascent: i16,
    raw_descent: i16,
    raw_line_gap: i16,
    raw_cap_height: i16,       // derived from 'H' bbox if OS/2 lacks it
    raw_x_height: i16,         // derived from 'x' bbox if OS/2 lacks it
    raw_slant_angle: f32,      // default 0.0 if missing
    raw_underline_position: Option<i16>,
    raw_underline_thickness: Option<i16>,
    raw_strikeout_position: Option<i16>,
    raw_strikeout_thickness: Option<i16>,
}
```

- `Font::from_bytes(name, data)` — parses once, derives any missing metrics from glyph bboxes
- `Font::metrics(size) -> FontMetrics` — scales by `size / units_per_em`, pure arithmetic
- `Font::glyph_metrics(char, size) -> Option<GlyphTypographyMetrics>` — parses glyph on demand from stored `data`, behind `#[cfg(feature = "typography_debug")]`
- `Font::name() -> &str`

### Step 2: Update `FontRegistry` to hold `Font` structs

**File:** `src/text/font_registry.rs`

- Replace `families: Vec<String>` with `fonts: Vec<Font>`
- `Font` is created during registration by calling `Font::from_bytes(name, data)`
- Add `pub fn font(&self, id: impl Into<FontId>) -> Option<&Font>`
- `family_name()` delegates to `Font::name()`
- Make `FontRegistry` public (re-export from `lib.rs`)

### Step 3: Wire up module visibility

- `src/text/mod.rs`: add `mod font;` with `pub use font::Font;` and `pub use font::FontMetrics;`
- `src/lib.rs`: add `pub use text::Font;`, `pub use text::FontMetrics;`, promote `FontRegistry` to `pub use text::FontRegistry;`
- Behind `#[cfg(feature = "typography_debug")]`: export `GlyphTypographyMetrics`, `TypographyOverlay`

### Step 4: Expose Parley LineMetrics from the Library

Currently `shape_text_cached` in `src/render/text_renderer.rs` iterates `layout.lines()` but only reads glyph positions. We need to also extract and return `LineMetrics`.

- Create `LineMetricsSnapshot` capturing parley `LineMetrics` fields using our canonical names (`top`, `bottom`, `baseline`, etc.)
- Add `query_text_metrics` function that takes text + `TextConfig` + font registry access and returns `Vec<LineMetricsSnapshot>`
- Re-export from `src/lib.rs`

### Step 5: Create `TypographyOverlay` component and system

**New file:** `src/render/typography_overlay.rs` — behind `#[cfg(feature = "typography_debug")]`

**Component:**
```rust
/// Add to a `WorldText` entity to render typography metric annotations as gizmos.
/// Built into the library as a debug tool — not example code.
#[derive(Component)]
pub struct TypographyOverlay {
    /// Show font-level metric lines (ascent, descent, cap height, x-height, baseline, line gap).
    pub show_font_metrics: bool,
    /// Show per-glyph bounding boxes and advance widths. Computed on the fly, never stored.
    pub show_glyph_metrics: bool,
    /// Show text labels on the metric lines.
    pub show_labels: bool,
}
```

**System:** Queries `Query<(&WorldText, &TextStyle, &GlobalTransform, &TypographyOverlay)>`:
- Calls `Font::metrics(size)` for font-level lines
- Draws B&W gizmo lines with different dash patterns and weights
- If `show_glyph_metrics`: calls `Font::glyph_metrics(char, size)` per character, draws bounding boxes and advance markers
- If `show_labels`: spawns/positions small `WorldText` labels at line ends

### Step 6: Register overlay in plugin

**File:** `src/plugin.rs`

- Behind `#[cfg(feature = "typography_debug")]`: register the overlay system
- Add `typography_debug` feature to `Cargo.toml`

### Step 7: Build the typography example

**File:** `examples/typography.rs`

The example is now minimal — it demonstrates the library's debug overlay:

1. Spawn a `WorldText::new("Typography")` with large size, black text
2. Insert `TypographyOverlay { show_font_metrics: true, show_glyph_metrics: true, show_labels: true }`
3. White/light background, front-facing camera
4. Interactive controls: arrow keys to adjust font size/spacing, toggle overlay options
5. Optional `DiegeticPanel` inspector showing numeric metric values

## Key Files to Modify

| File | Change |
|---|---|
| `src/text/font.rs` | **New** — `Font`, `FontMetrics`, `GlyphTypographyMetrics` |
| `src/text/font_registry.rs` | Replace `families` with `fonts: Vec<Font>`, add `.font()`, make public |
| `src/text/mod.rs` | Add `mod font` + `pub use` for `Font`, `FontMetrics` |
| `src/render/typography_overlay.rs` | **New** — `TypographyOverlay` component + gizmo drawing system |
| `src/render/mod.rs` | Wire up overlay module (feature-gated) |
| `src/plugin.rs` | Register overlay system (feature-gated) |
| `src/lib.rs` | Re-export `Font`, `FontMetrics`, `FontRegistry`, `TypographyOverlay` |
| `src/render/text_renderer.rs` | Expose line metrics query function |
| `Cargo.toml` | Add `typography_debug` feature |
| `examples/typography.rs` | Minimal: spawn text + overlay + interactive controls |

## Verification

1. `cargo build` — library compiles with new `Font`/`FontMetrics` types (no feature flag needed)
2. `cargo build --features typography_debug` — overlay code compiles
3. `cargo build --example typography --features typography_debug` — example compiles
4. `cargo run --example typography --features typography_debug` — shows:
   - Large "Typography" word on clean background
   - B&W metric annotation lines with labels
   - Per-glyph bounding boxes and advance markers
   - Interactive controls to adjust size/spacing and toggle overlay options
5. `cargo +nightly fmt` passes
6. `cargo clippy` clean

## Follow-up Items (before merging back to `feature/text-rendering`)

These are out of scope for this branch but must be addressed before or shortly after the merge:

1. **`FontRegistry::register_font(name, bytes) -> FontId`** — Allow users to load custom TTF/OTF fonts at runtime. The `Font::from_bytes()` infrastructure is already in place; this just needs a public registration method that inserts into both parley's `FontContext` and `fonts: Vec<Font>`.

2. **System font discovery** — Enumerate and load system-installed fonts. Would integrate with fontique's system font discovery. Each discovered font gets its own `FontId` via `register_font()`.

3. **Font selection ergonomics** — Consider whether `FontId` should support string-based lookup (e.g., `registry.font_by_name("Helvetica")`) alongside the numeric ID pattern.

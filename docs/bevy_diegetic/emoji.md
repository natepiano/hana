# Analytic color emoji (COLR) for the slug text renderer

Status: planned / not started. Related: [slug_fx.md](slug_fx.md),
[diegetic-text-perf.md](diegetic-text-perf.md),
[../fairy_dust/canonical_example.md](../fairy_dust/canonical_example.md).

## Goal

Render full-color emoji (and color icon fonts) through the existing analytic
glyph renderer, so they stay crisp at any zoom and gain real color — the one
thing bitmap emoji (Apple's sbix) and atlas-based color emoji cannot do. A COLR
glyph is a stack of vector outlines, each with a color or gradient; those
outlines are the same kind the renderer already fills analytically, so the
geometry path is reuse, not a second renderer.

## Non-goals

- Bitmap emoji (sbix / CBDT+CBLC) and SVG-table glyphs. Those are raster or
  full-SVG and do not fit the curve/band coverage model. A COLR font (Noto Color
  Emoji, Twemoji) is required.
- COLRv1 variable-font deltas (gradient/transform deltas at a variation
  instance). Pin to the font's default instance.

## Background: what COLR is

OpenType color via the `COLR` + `CPAL` tables. A color glyph is a back-to-front
list of layers (COLRv0) or a paint graph (COLRv1):

- **COLRv0** — a flat list of `(glyph id, palette index)` layers, each a solid
  fill. Covers older Microsoft emoji and many flat-color icon fonts.
- **COLRv1** — a paint graph: `PaintColrLayers`, `PaintGlyph` (clip to an
  outline, then paint a child), `PaintColrGlyph` (reference another color
  glyph), solid fills, linear / radial / sweep gradients with color-stop ramps
  and extend modes, per-subtree affine transforms, and `PaintComposite`
  (Porter-Duff / blend modes). Noto Color Emoji and Twemoji are COLRv1.
- **CPAL** — the palette table holding the actual colors that solid fills and
  gradient stops index into. Palette 0 is the default. The special foreground
  index (0xFFFF) means "use the run's text color".

`ttf-parser` 0.25 (already in the tree, the same crate `outline.rs` uses)
exposes this through `Face::paint_color_glyph(glyph_id, palette, foreground,
&mut painter)` where `painter` implements `ttf_parser::colr::Painter`. The
walker drives the paint graph and calls the painter back; outlines arrive
through the same `OutlineBuilder` interface the monochrome path already
implements. No new font dependency.

## The pipeline this plugs into

Today's monochrome path (all under `crates/bevy_diegetic/src/text/slug/`):

- `glyph/outline.rs` — `ttf_parser::Face` + `OutlineBuilder`, cubic→quadratic
  conversion, producing a quadratic-only `Glyph` (`Contour` / `QuadraticSegment`
  / `Bounds`).
- `glyph/packing.rs` — packs a glyph's quadratics into `CurveRecord` + a
  horizontal/vertical `BandRecord` acceleration structure, plus a `GlyphRecord`
  (`bounds_min_size`, `band_range`).
- `render/run_data.rs` — `RunRenderData { mesh, curves, bands, glyphs }`: one
  quad per glyph instance, combined curve/band/glyph buffers for the run.
- `render/material.rs` — `TextExtension` binds `uniforms` (100, a single
  `fill_color` + `render_mode` + AA flags), `curves` (101), `bands` (102),
  `glyphs` (103). `RenderMode` is an enum (`Text` / `PunchOut`).
- `shaders/slug_text.wgsl` — each quad reads its glyph index from `UV_1.x`,
  evaluates non-zero winding `render_coverage(uv, glyph)` from the curves via the
  bands, then `final_alpha = coverage * fill_color.a`, `base_color =
  fill_color.rgb`, runs PBR lighting, and writes through OIT (`oit_draw`). A
  prepass fragment uses a fixed 0.5 coverage cutoff and reads only `glyphs`.
- `runtime/glyph_cache.rs` — `GlyphCache` owns `GlyphOutlineCache` (per-glyph
  packed curves/bands, keyed by `GlyphKey`) and per-run `RunStorage`;
  `build_run_render_data` + `commit_run_storage` build then upload a run.
- `render/text_shaping.rs` — parley shaping; reads glyph ids/positions and
  builds `GlyphInstance`s.

One run = one mesh + curve/band/glyph buffers + one material with one
`fill_color`.

## The insight: a color glyph is N monochrome layers

Each COLR layer is one outline + one brush + a draw order (and, in v1, a baked
transform). An outline is exactly what `packing.rs` already turns into
curves/bands, and coverage is exactly what the shader already evaluates. So a
color glyph expands into N layer-quads in the run mesh; the only genuinely new
pieces are (a) a per-layer **brush** (solid color or gradient) replacing the
single per-run `fill_color`, and (b) gradient evaluation in the shader.

```
monochrome:  glyph  -> 1 quad -> coverage * one run fill_color
color glyph: glyph  -> N quads (one per layer)
                       each quad: coverage * its own brush (solid or gradient)
```

## Data model (decided)

These are the types and the GPU layout the phases below build toward.

- **`CachedGlyph` enum** — the glyph cache returns
  `Monochrome(GlyphOutline) | Color(ColorGlyph)`. `ColorGlyph { layers:
  Vec<ColorLayer> }`; `ColorLayer { outline: Glyph, transform: Affine2, brush:
  Brush }`. Color status is marked on `PositionedGlyph` at the shaping→build
  boundary so run build never re-queries the font.
- **`Brush`** — `Solid(PaletteEntry) | LinearGradient {…} | RadialGradient {…}
  | SweepGradient {…}`. `PaletteEntry = Static(LinearRgba) | Foreground`. A
  gradient cannot be `Foreground`, so that state is unrepresentable. Extraction
  never bakes a color for the 0xFFFF foreground index — it stores `Foreground`,
  resolved in-shader from `uniforms.fill_color` (both color and alpha).
- **`GlyphKey`** — gains a `Palette` newtype field so a color glyph's identity
  includes its palette (palette 0 only until a multi-palette / dark-mode need
  appears). The palette, brush, and layer indices are newtypes mirroring the
  existing `FontKey` / `GlyphKey` pattern, and their ranges are validated at pack
  time so an out-of-range index cannot reach the GPU.
- **`QuadRecord { glyph_outline_index, brush_index, layer_index }`** — one
  storage entry per layer-quad, a `ShaderType` struct at binding 106. The mesh
  keeps a single per-quad index in `UV_1.x` that addresses it;
  `glyph_outline_index` points into the deduped `glyphs` buffer. The main
  fragment takes one indirection
  (`glyphs[quad_records[i].glyph_outline_index]`); the prepass does not — color
  runs are `AlphaMode::Blend` (below) and so are excluded from the opaque depth
  prepass, which keeps its direct `glyph_index(uv)` coverage path. Pure-mono runs
  never address a `QuadRecord`. `GlyphRecord` stays monochrome-only — per-layer
  data lives in `QuadRecord`, not a widened glyph record.
- **`BrushRecord` / `StopRecord`** — `Brush` → `BrushRecord` via an explicit
  `impl From<&Brush>` with `#[repr(u32)]` tags (`BrushTag`, `ExtendMode`,
  `CompositeMode`). A `ShaderType` layout test asserts size+offsets match the
  WGSL structs, mirroring the `packing.rs` record tests.
- **GPU bindings** — `brushes` (104), `gradient_stops` (105), `quad_records`
  (106), all per-run buffers like curves/bands/glyphs. Confirm 104–106 are free
  against StandardMaterial + OIT bindings.
- **`RenderMode::ColorLayer`** — a new variant on the existing `RenderMode` enum
  (not a separate `has_color` bool). One discriminant routes the shader per-run;
  keep the WGSL render-mode constant in sync with the Rust discriminant through
  the existing `From<u32>` mapping, and assert the pairing in a test.
- **Layer depth** — the main fragment adds `f32(layer_index) * stride` to
  `oit_pos.z` before `oit_draw`, layer 0 nearest. There is no separate
  `base_offset`: the per-run `command_index` depth bias already rides in
  `uniforms.oit_depth_offset`, and the per-layer term composes with it rather
  than overwriting it, so emoji layering and element layering stack correctly.
  Define the stride as a named constant with a max-layer cap and documented
  precision headroom, and verify ordering with a 40+-layer emoji.
- **Blend mode** — a color run sets `AlphaMode::Blend` at material build so it
  routes through OIT (where prepass depth is not authoritative). `text_material`
  currently leaves `base.alpha_mode` defaulting to `Opaque`; color runs must set
  Blend explicitly. Color glyphs are never opaque-prepass.
- **Mixed runs** — a run with ≥1 color glyph routes every quad through a brush:
  monochrome letters in the run get a `Solid(Foreground)` brush at layer index
  0. A pure-monochrome run (the common case, every stress-test label) stays on
  the single-`fill_color` fast path with zero extra per-fragment work, and its
  mesh+buffers stay byte-identical to today.
- **Color space** — CPAL colors and gradient stops are sRGB; convert to linear
  at extraction (reuse the `LinearRgba` conversion in `text_material`), store and
  interpolate stops in linear, consistent with the PBR `base_color` path.
- **Transforms** — the painter keeps a transform stack; when a layer is emitted
  it composes the stack into one `bevy::math::Affine2` (left-multiply), applies
  it to the outline control points, and transforms any gradient endpoints by the
  same affine so the shader evaluates the gradient parameter in post-transform
  design space. A layer whose transformed bounds have zero width or height is
  skipped (`packing.rs` divides by extent → NaN otherwise).
- **Lighting** — emoji layers render PBR-lit like the rest of the text, since
  diegetic panels are physically lit by design. A per-run opt-out to emissive is
  available if a flat self-lit look is wanted.

## Plan of record

Sequential phases. Each builds on the previous; the example in Phase 7 is the
first user-visible result.

### Phase 1 — Bump parley 0.10 / harfrust 0.8

Update `parley` in the workspace `Cargo.toml` from 0.9.0 to 0.10.0 and let
`harfrust` follow to 0.8. Build, run the existing text examples, confirm no
visual regression in monochrome text. This is the one external prerequisite:
color-emoji matching with variation selectors (U+FE0F) is broken on 0.9 (#617),
so "❤️" would miss or fall back to monochrome until this lands. Guard the bump
with a shaping-output regression check — assert glyph ids and advances are
unchanged across 0.9→0.10 on a diverse string set (ASCII, CJK, RTL Arabic,
ligatures, a U+FE0F sequence), not just a visual pass, since a parley minor bump
can move glyph selection or line breaking.

### Phase 2 — Verify the ttf-parser COLR API and add a cycle guard

Confirm `Face::paint_color_glyph`'s signature (including the foreground
argument), the `colr::Painter` callbacks the data model assumes, and whether
ttf-parser bounds `PaintColrGlyph` recursion. If it does not, the painter adds a
depth cap (≈16) that logs and skips on overflow, so a cyclic or adversarial COLR
font cannot hang extraction. Verify against a real COLR font (Noto Color Emoji
or a fixture) with a minimal probe painter that records the callback sequence,
the foreground index value, and whether a transform stack is pushed. Track
recursion depth as a field on the painter, increment/decrement on push/pop, and
on overflow log a warning naming font + glyph and return the partially
accumulated layers (best-effort, never a hang).

### Phase 3 — Color extraction (`glyph/color.rs`)

Implement `ttf_parser::colr::Painter`, accumulating a flat layer list into
`ColorGlyph`. Callbacks map directly:

- `outline_glyph` feeds the existing `OutlineBuilder`.
- `push_transform` / `pop_transform` maintain the transform stack baked into
  each layer's outline at emit time (per the transform rule above).
- A solid paint's palette index `0xFFFF` → `PaletteEntry::Foreground`; any other
  index → CPAL lookup → sRGB→linear `PaletteEntry::Static`.
- `paint` resolves a `Paint` into a `Brush`; gradients capture endpoints, stops,
  and `ExtendMode`.
- `push_layer` / `pop_layer` carry the composite mode (alpha-over only at
  first).

Detect a color glyph by `paint_color_glyph(...)` returning `Some`; otherwise
fall through to the monochrome `load_glyph_by_id_from_face`. Unsupported `Paint`
variants log and skip rather than panicking. An out-of-range palette index falls
back to palette 0 (or skips the layer with a warning); a `PaintColrGlyph`
reference to a missing glyph skips that subtree. Every fallback logs once and
never panics.

### Phase 4 — Glyph cache (`runtime/glyph_cache.rs`)

Cache the flattened, packed color layers the same way `GlyphOutlineCache` caches
monochrome packed glyphs, keyed off `GlyphKey` plus the new `Palette` field.
Return the `CachedGlyph` enum. Adding the `Palette` field to `GlyphKey` and
widening the cache accessor from `&GlyphOutline` to `&CachedGlyph` migrates the
monochrome call sites in the same change (an `outline_ref()` helper keeps the
mono path terse). Color-glyph layers are static per `(font, glyph id, palette)`,
so a moving emoji re-packs nothing and the per-frame in-place update stays cheap
(see diegetic-text-perf.md).

### Phase 5 — Run build (`render/run_data.rs`)

Expand a color glyph instance into one quad per layer. `RunPacker` adds one
glyph record per layer to the run's deduped `glyphs` buffer, emits N
layer-quads, and returns a per-layer index range (not a single index). Extend
`RunRenderData` with the per-run brush/quad/stop buffers:

```text
RunRenderData {
    mesh, curves, bands, glyphs,        // glyphs now include layer outlines
    brushes: Vec<BrushRecord>,          // one per distinct brush in the run
    quad_records: Vec<QuadRecord>,      // one per layer-quad
    gradient_stops: Vec<StopRecord>,    // referenced by gradient brushes (Phase 8+)
}
```

curves/bands/glyphs keep the stable-handle in-place re-upload; the brush / quad
/ stop buffers are sized to the run's layer-quad count and may realloc on
rebuild. The quad↔brush pairing is modeled structurally (one `QuadRecord` per
layer-quad), not as length-correlated parallel `Vec`s.

When a layer's baked transform collapses its bounds to zero width or height, the
quad is dropped with a warning before packing (`packing.rs` divides by extent).
In a color or mixed run every quad carries a `QuadRecord`; a monochrome glyph in
such a run emits one quad with a `Solid(Foreground)` brush at layer 0.

### Phase 6 — Material + shader (`render/material.rs`, `shaders/slug_text.wgsl`)

Add the `brushes` (104), `gradient_stops` (105), and `quad_records` (106)
storage bindings and the `RenderMode::ColorLayer` discriminant. Color runs set
`AlphaMode::Blend`. In the fragment shader, after `render_coverage`:

- Read the quad's `QuadRecord` for its glyph outline, brush, and layer index.
- **Solid brush** — `rgb = brush.color.rgb`; a `Foreground` brush reads
  `uniforms.fill_color` (color and alpha). Identical cost to today for the
  constant case.
- Apply the per-layer depth term `oit_pos.z += f32(layer_index) * stride` before
  `oit_draw` (the per-run bias already rides in `uniforms.oit_depth_offset`).

`final_alpha = coverage * brush_alpha`, feed `base_color`, PBR + OIT tail
unchanged. Phase 6 lands solid brushes only; the gradient branch is Phase 8.
Pure-monochrome runs stay on the `RenderMode::Text` single-`fill_color` path.

Add a test that bindings 104–106 do not collide with StandardMaterial + OIT, and
one that a color run's material is built with `AlphaMode::Blend`. Color runs
(Blend) and monochrome runs (Opaque) land in different render phases, so they do
not batch together — measure the bind-group / draw-call impact in Phase 7.

### Phase 7 — Flat color (COLRv0) + the `color_emoji` example

First end-to-end color: layered solid fills, no gradients or transforms beyond
the baked affine. Proves painter → multi-layer expand → palette → per-layer
solid brush on the existing coverage shader. Ships the `color_emoji` example
(below) and the acceptance gates (below). Validate with a COLRv0 font or a
COLRv1 font's v0 fallback.

### Phase 8 — Gradients (COLRv1 solid + linear/radial)

Covers the bulk of Noto Color Emoji / Twemoji. Adds the gradient `BrushRecord`
fields, the `StopRecord` buffer, and shader evaluation: compute the gradient
parameter `t` at the design-space point (`design_position(uv, glyph)` exists),
apply the extend mode, sample the stop ramp. Linear and radial first. Decide
stop-ramp storage here: a buffer of stops with in-shader interpolation (simpler,
more ALU) versus a baked 1D ramp texture (less ALU, one more texture binding);
`StopRecord` and the `(stop_start, stop_count)` range are defined either way.
Phase 7 extracts `ExtendMode` but treats it as Pad and warns on Repeat/Reflect;
full extend handling lands here. A layer with a non-uniform or shear affine
breaks the assumption that the gradient direction stays perpendicular to the
outline — handle it by evaluating the gradient in pre-transform space or storing
the inverse affine in the brush record. Survey Noto / Twemoji for non-uniform
layer transforms first; defer the handling if no target font uses them.

### Phase 9 — Conformance tail

Sweep gradients, nested `PaintColrGlyph`, and `PaintComposite` / blend modes
beyond alpha-over. OIT alpha-over already handles the common overlap case, so
non-alpha-over composite modes are last and optional. Survey Noto Color Emoji
first to confirm which composite modes any target font actually uses before
implementing them.

## Example: `color_emoji`

`crates/bevy_diegetic/examples/color_emoji.rs`, following
[canonical_example.md](../fairy_dust/canonical_example.md): a slowly spinning
cube whose six faces demonstrate every combination of {WorldText, panel} ×
{emoji-only, mixed with text}, plus a single-large hero and an any-zoom
demonstration. It is the Phase 7 deliverable and the standing visual regression
for the feature.

### Face matrix

| Face   | Surface           | Content                                                            |
|--------|-------------------|--------------------------------------------------------------------|
| Front  | WorldText         | One large emoji (e.g. 🚀) — hero shot, the any-zoom claim up close   |
| Back   | WorldText         | A row of several emoji (e.g. 😀 🎉 🌍 🔥) — emoji-only in world space  |
| Right  | WorldText         | Inline text + emoji ("Ship it 🚀") — mixed run in world space        |
| Left   | DiegeticPanel     | Emoji picker grid (rows × columns of emoji) — emoji-only in a panel |
| Top    | DiegeticPanel     | Mixed text+emoji rows ("Build ✅", "Tests 🧪", "Deploy 🚀")            |
| Bottom | WorldText         | One emoji repeated small → large — any-zoom crispness within a face |

This covers: emoji as WorldText alone (Front, Back) and mixed (Right); emoji in
a panel alone (Left) and mixed (Top); a single large emoji (Front); an emoji
picker panel (Left); and the resolution-independence claim made visible without
orbiting (Bottom).

### Conventions

Standard canonical builder chain — `fairy_dust::sprinkle_example()` with
`.with_brp_extras()`, `.with_save_window_position()`, `.with_studio_lighting()`,
`.with_ground_plane().size(fairy_dust::EXAMPLE_GROUND_SIZE)`, the cube via
`.with_cube()` carrying `CameraHomeTarget`, `.with_orbit_cam_preset(|_| {},
OrbitCamPreset::BlenderLike)`, `.with_camera_home().pitch(..).yaw(..)`, a
`TitleBar`, and `.with_camera_control_panel()`. The lead `fn main` comment notes
that `DiegeticUiPlugin` is registered automatically by `sprinkle_example`.

- **Rotation** — `.with_cube_spin::<…>(CubeSpinConfig::new())`, which registers
  the `P Pause` chip and starts spinning. The slow spin is what shows every face
  and lets the orbit camera demonstrate crispness across distances.
- **Panels** (Left, Top) — `CubeFacePanelStyle::for_cube(cube_size)` with
  `cube_face_panel_tree(...)`; the emoji picker is a grid of emoji cells, the Top
  panel is title + text/emoji rows.
- **WorldText faces** (Front, Back, Right, Bottom) — `fairy_dust::cube_face_text`
  (or a WorldText child bundle) sized per face.
- **`.with_stable_transparency()`** after the OrbitCam helper, since the faces
  carry coplanar translucent color-glyph layers under OIT.
- Add `.with_title("Color Emoji")` and a chip listing any example-specific
  control; `H Home` and `P Pause` are auto-added by their capabilities.
- Use both simple (few-layer, e.g. 🎉) and complex (many-layer, e.g. 😀) emoji so
  the example exercises the light and heavy fill-rate cases; note it raises the
  render floor on an already render-bound path.

## Acceptance gates (Phase 7)

- The painter flattens a named emoji into the expected layer count and per-layer
  palette colors (unit test against a known COLR font fixture).
- The `color_emoji` example shows no pixelation or color bleed at 1× / 5× / 10×
  zoom.
- A `Foreground`-index glyph resolves to two different run fill colors (proves
  the in-shader cascade, color and alpha).
- Monochrome text mesh + buffers stay byte-identical (the color path is
  additive; the single-`fill_color` route must not change).
- An N-layer emoji composites top-to-bottom under OIT, not shuffled. Verify OIT
  linked-list ordering with a 40+-layer emoji.
- The Phase 1 shaping-output regression (glyph ids + advances across the parley
  bump) stays green.
- Extracting a cyclic or adversarial COLR font terminates without hanging and
  logs the skipped subtree.
- The example's frame cost is measured against the monochrome render floor; the
  added fill-rate from coplanar layers is recorded, not treated as a regression.

# Analytic color emoji (COLR) for the slug text renderer

Status: design / not started. Owner: TBD. Related: [slug_fx.md](slug_fx.md),
[diegetic-text-perf.md](diegetic-text-perf.md).

## Goal

Render full-color emoji (and color icon fonts) through the existing analytic
glyph renderer, so they stay crisp at any zoom and gain real color — the one
thing bitmap emoji (Apple's sbix) and atlas-based color emoji cannot do. A COLR
glyph is a stack of vector outlines, each with a color or gradient; those
outlines are the same kind the renderer already fills analytically, so the
geometry path is reuse, not a second renderer.

Non-goal: bitmap emoji (sbix / CBDT+CBLC) and SVG-table glyphs. Those are raster
or full-SVG and do not fit the curve/band coverage model. A COLR font (Noto
Color Emoji, Twemoji) is required.

## Background: what COLR is

OpenType color via the `COLR` + `CPAL` tables. A color glyph is a back-to-front
list of layers (COLRv0) or a paint graph (COLRv1):

- **COLRv0** — a flat list of `(glyph id, palette index)` layers, each a solid
  fill. Simple. Covers older Microsoft emoji and many flat-color icon fonts.
- **COLRv1** — a paint graph: `PaintColrLayers`, `PaintGlyph` (clip to an
  outline, then paint a child), `PaintColrGlyph` (reference another color
  glyph), solid fills, linear / radial / sweep gradients with color-stop ramps
  and extend modes, per-subtree affine transforms, and `PaintComposite`
  (Porter-Duff / blend modes). Noto Color Emoji and Twemoji are COLRv1.
- **CPAL** — the palette table holding the actual colors that solid fills and
  gradient stops index into. Palette 0 is the default.

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
  `glyphs` (103).
- `shaders/slug_text.wgsl` — each quad reads its glyph index from `UV_1.x`,
  evaluates non-zero winding `render_coverage(uv, glyph)` from the curves via the
  bands, then `final_alpha = coverage * fill_color.a`, `base_color =
  fill_color.rgb`, runs PBR lighting, and writes through OIT (`oit_draw`).
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

## Architecture changes

### 1. Color extraction (`glyph/` — new `color.rs`)

Implement `ttf_parser::colr::Painter`, accumulating a flat layer list:

```text
ColorGlyph { layers: Vec<ColorLayer> }
ColorLayer { outline: Glyph,            // quadratics, via the existing OutlineBuilder
             transform: Affine2,        // baked from push/pop_transform
             brush: Brush }
Brush      = Solid { rgba_linear }
           | LinearGradient { p0, p1, stops, extend }
           | RadialGradient { c0, r0, c1, r1, stops, extend }
           | SweepGradient  { center, start_angle, end_angle, stops, extend }
```

Painter callbacks map directly: `outline_glyph` feeds the existing
`OutlineBuilder`; `push_transform` / `pop_transform` maintain a transform stack
baked into each layer's outline at emit time; `paint` resolves a `Paint` into a
`Brush`; `push_layer` / `pop_layer` carry the composite mode (subset: alpha-over
only at first). Detect a color glyph by `paint_color_glyph(...)` returning
`Some`; otherwise fall through to the monochrome `load_glyph_by_id_from_face`.

CPAL: resolve palette index 0; map the special foreground index (0xFFFF) to the
run's text color so monochrome-on-color emoji honor the cascade.

### 2. Caching (`runtime/` — extend `GlyphOutlineCache`)

Color-glyph layers are static per `(font, glyph id, palette)`, so cache the
flattened, packed layers the same way `GlyphOutlineCache` caches monochrome
packed glyphs. Keyed off `GlyphKey` plus a palette discriminator. This keeps the
per-frame in-place update cheap (see diegetic-text-perf.md) — a moving emoji
re-packs nothing.

### 3. Run build (`render/run_data.rs`)

Expand a color glyph instance into one quad per layer. Extend `RunRenderData`
with per-layer brush data:

```text
RunRenderData {
    mesh, curves, bands, glyphs,        // unchanged; glyphs now include layer outlines
    brushes: Vec<BrushRecord>,          // one per layer-quad, indexed like glyphs
    gradient_stops: Vec<StopRecord>,    // referenced by gradient brushes
}
```

Each quad already indexes its `GlyphRecord` via `UV_1.x`; add a parallel brush
index (a second `UV_1` channel or `UV_2`) so the shader can look up the layer's
brush.

### 4. Material + shader (`render/material.rs`, `shaders/slug_text.wgsl`)

Add two read-only storage bindings: `brushes` (104) and `gradient_stops` (105).
`BrushRecord` holds a kind tag + solid color, or gradient geometry + a
`(stop_start, stop_count)` range into `gradient_stops`. In the fragment shader,
after `render_coverage`:

- **Solid** — `rgb = brush.color.rgb` (constant). Identical cost to today.
- **Gradient** — compute the gradient parameter `t` at the design-space point
  (`design_position(uv, glyph)` already exists), apply the extend mode, sample
  the stop ramp, get `rgb`. Linear and radial first; sweep later.

Then `final_alpha = coverage * brush.color.a`, feed `base_color`, and the PBR +
OIT tail is unchanged. The single `fill_color` uniform stays as the fallback for
monochrome runs.

### 5. Shaping route (`render/text_shaping.rs`)

No change to parley itself. After shaping, when an instance's glyph is a color
glyph, mark it so run build expands it into layers. parley 0.10 / harfrust 0.8
is the matching prerequisite (fixes color-emoji matching with non-printing
variation selectors, #617).

## Milestones

- **M1 — COLRv0 flat color.** Layered solid fills, no gradients or transforms.
  Proves the painter → multi-layer expand → palette → per-layer solid brush path
  end-to-end on the existing coverage shader. Validate with a COLRv0 font (or a
  COLRv1 font's v0 fallback) in a new example.
- **M2 — COLRv1 solid + linear/radial gradients.** Covers the bulk of Noto Color
  Emoji / Twemoji. Adds the gradient `BrushRecord` + stop buffer + shader
  evaluation; transforms baked into the outline at extraction.
- **M3 — sweep gradients, nested `PaintColrGlyph`, composite/blend modes.** The
  conformance tail. OIT alpha-over already handles the common overlap case, so
  non-alpha-over composite modes are last and optional.

## Design decisions

- **Lighting.** Diegetic panels are physically lit by design, so emoji layers
  render PBR-lit like the rest of the text by default — consistent, not a
  special case. Provide a per-run opt-out to emissive if a flat, self-lit emoji
  look is wanted. Decide before M2 (gradients under lighting need the look
  confirmed).
- **Color space.** CPAL colors and gradient stops are sRGB; convert to linear at
  extraction, reusing the `LinearRgba` conversion already in `text_material`.
- **Layer paint order.** Layers paint back-to-front. Enforce order with
  per-layer `oit_depth_offset` increments, mirroring how `command_index` already
  drives the run depth bias, so overlapping opaque-over layers composite in the
  right order under OIT.
- **Clipping.** Color layers route through the existing
  `build_run_render_data_with_clip` so panel `clip_rect` applies to emoji too.
- **Perf / buffer sizing.** A detailed emoji is dozens of layers → more
  curves/bands/quads per glyph. Caching per glyph id keeps it static, so this is
  buffer-capacity and fill-rate, not re-pack cost. Track emoji-heavy frames in
  diegetic-text-perf.md once M1 lands.

## Open questions

- COLRv1 + variable fonts (gradient/transform deltas at a variation instance) —
  out of scope initially; pin to the default instance.
- `PaintComposite` modes beyond alpha-over — do any target fonts actually use
  them? Survey Noto Color Emoji before committing M3.
- Brush index transport: second channel on `UV_1` vs a new `UV_2` vs folding the
  brush index into the `GlyphRecord` — pick during M1 based on vertex layout.
- Does any target emoji rely on the foreground (text-color) palette entry? If
  so, the cascade fill color must reach the brush resolve.

## Testing / validation

- Unit: painter flattening against a known COLR font fixture — assert layer
  count, per-layer palette colors, gradient stop ranges.
- Render: a `color_emoji` example showing a row of emoji at several zoom levels
  next to a bitmap reference, to demonstrate the any-zoom crispness claim.
- Regression: confirm monochrome text is byte-identical (the color path is
  additive; the single-`fill_color` route must not change).

## Team review — cycle 1 (2026-06-02)

Five lenses (correctness, architecture, risk, type-system, GPU/shader), each
grounded in the slug renderer source. No premise-challenge survived: two lenses
framed the layer-ordering and parley-version items as "blockers", but both are
fixable within the committed analytic-COLR approach, so they are recorded as
refinements (R6/R11), not challenges to the design.

### Recorded refinements (determined — fold into M1/M2 work)

- **R1 — Cache key needs a palette discriminator.** `GlyphKey` (runtime/run.rs:27)
  carries only `font` / `glyph_id` / `preprocess_version`. Add a `Palette`
  newtype field so a color glyph's identity includes its palette. M1 uses palette
  0 only; multi-palette / dark-mode deferred to M2+ via a per-run palette
  selector. (all 5 lenses)
- **R2 — Model monochrome-vs-color as an enum, not a flag.** Cache returns
  `CachedGlyph = Monochrome(GlyphOutline) | Color(ColorGlyph)`, with `ColorGlyph
  { layers: Vec<ColorLayer> }` and `ColorLayer { outline, transform: Affine2,
  brush: Brush }`. Mark color status on `PositionedGlyph` at the shaping→build
  boundary so run-build never re-queries the font. (type-system, correctness)
- **R3 — `Brush` ↔ `BrushRecord` via explicit `From` + `#[repr(u32)]` tags.**
  `BrushTag`, `ExtendMode::{Pad,Repeat,Reflect}`, `CompositeMode` as enums;
  `impl From<&Brush> for BrushRecord` is the single source of truth for the GPU
  tag mapping. Add a `ShaderType` layout test asserting `BrushRecord` /
  `StopRecord` size+offsets match the WGSL structs (mirroring the existing
  packing.rs records). (type-system)
- **R4 — `PaletteEntry::{Static(LinearRgba), Foreground}`.** Extraction never
  bakes a color for the 0xFFFF foreground index; it stores a `Foreground` marker
  so a cached layer stays correct under any run fill color. (ties to DEC-2)
- **R5 — Brush-index transport = `UV_1.y`.** `.y` is hardcoded `0.0`
  (run_data.rs:243) and unread by the shader; pack the per-quad brush index
  there. Monochrome quads keep `.y = 0`. Avoids a new `UV_2` attribute and the
  prepass change it would force. (closes the doc's open question)
- **R6 — Brush/stop buffers are per-run, quad-indexed.** Keep two update
  semantics: curves/bands/glyphs ride the stable-handle in-place re-upload
  (diegetic-text-perf.md); `brushes` / `gradient_stops` are sized to the run's
  layer-quad count and may realloc on rebuild. Model the quad↔brush pairing
  structurally (one record per layer-quad), not as length-correlated parallel
  `Vec`s. (architecture, type-system — critical)
- **R7 — Transform and gradient geometry in one space.** When a layer's affine
  is baked into its outline, transform the gradient endpoints (`p0`/`p1`,
  center/radii) by the same affine so the shader evaluates `t` in post-transform
  design space. Guard degenerate transforms: skip a layer whose transformed
  bounds have zero width/height (packing.rs divides by extent → NaN otherwise).
  (risk, GPU, correctness)
- **R8 — Gradient color in linear space.** Convert CPAL / stop colors sRGB→linear
  at extraction (reuse `LinearRgba`, material.rs:118); store and interpolate stops
  in linear, consistent with the PBR `base_color` path. (risk, GPU)
- **R9 — Verify ttf-parser 0.25 API + cycle handling (M1 prerequisite).** Confirm
  `Face::paint_color_glyph` signature (incl. foreground), the `colr::Painter`
  callbacks the design assumes, and that `PaintColrGlyph` recursion/cycles are
  bounded; add a depth cap in the painter if ttf-parser does not guard. (correctness, risk)
- **R10 — M1 scope clarifications.** M1 excludes `PaintGlyph` per-layer clip paths
  and non-alpha-over `PaintComposite` modes; the painter matches all `Paint`
  variants and logs+skips unsupported ones rather than panicking. Panel
  `clip_rect` still applies via `build_run_render_data_with_clip`. (correctness)
- **R11 — parley 0.10 / harfrust 0.8 is a hard M1 blocker.** Color-emoji matching
  with variation selectors (U+FE0F) is broken on 0.9 (#617); "❤️" would miss or
  monochrome-fall-back. Land the bump first. (risk — reframed from a premise-challenge)
- **R12 — Binding + perf notes.** `brushes` (104) / `gradient_stops` (105) are
  per-run buffers like curves/bands/glyphs; audit that 104/105 are free against
  StandardMaterial + OIT bindings. Add a color-emoji overdraw stress case to
  diegetic-text-perf.md once M1 lands (dozens of coplanar layers under OIT add
  fill-rate to an already render-bound test). (GPU, risk)
- **R13 — Stop-ramp storage (M2 open).** Choose buffer-of-stops + in-shader
  interpolation (simple, more ALU) vs a baked 1D ramp texture (less ALU, one more
  texture binding); define `StopRecord` and the `(stop_start, stop_count)` range
  either way. Radial/sweep `t` math and extend-mode handling specified at M2/M3. (GPU)

### Proposed user decisions

- **DEC-1 — Layer paint-ordering mechanism.** *(critical; GPU / risk / type /
  architecture)* status: proposed.
  Problem: the "Layer paint order" decision says to enforce order with per-layer
  `oit_depth_offset` increments. But `oit_depth_offset` is a single per-run
  uniform (material.rs:54; `oit_pos.z += uniforms.oit_depth_offset`,
  slug_text.wgsl:537), and a color glyph's layer-quads are coplanar in one mesh
  with identical `in.position.z` — a per-run offset cannot separate them, so
  overlapping layers composite in undefined order under OIT.
  Recommendation: carry a per-quad layer index (in the `BrushRecord` / alongside
  R5's `UV_1.y`) and apply `oit_pos.z += base_offset + layer_index * stride`
  per-fragment before `oit_draw`, layer 0 nearest. This supersedes the doc's
  stated mechanism — approve the structural correction.
- **DEC-2 — Foreground-color cascade: M1 scope.** *(critical; all lenses)*
  status: proposed.
  Problem: the doc commits to mapping the 0xFFFF foreground index to the run's
  text color so mono-on-color emoji honor the cascade. Baking at extraction
  cannot do that (the cached layer would freeze one color). Honoring it needs an
  in-shader `Brush::Foreground` kind reading `uniforms.fill_color` plus the cache
  storing a `Foreground` marker (R4).
  Choice: (a) M1 honors the cascade (add `Brush::Foreground` + in-shader resolve)
  — matches the doc's stated intent, more work; or (b) M1 ships with palette-0
  colors baked (no foreground cascade) and adds it in M2 — faster M1, a temporary
  behavior gap. Recommend (a).
- **DEC-3 — Monochrome fast-path vs unified brush path.** *(important; GPU /
  architecture)* status: proposed.
  Problem: adding the brush lookup risks taxing the monochrome path, which
  carries the bulk of text on an already render-bound test
  (diegetic-text-perf.md).
  Choice: (a) keep monochrome runs entirely on the current single-`fill_color`
  uniform path, gated by a run-level has-color flag, so they emit zero extra
  color-path work (protects the render floor); or (b) route all quads through a
  brush record (sentinel `brushes[0]` = `fill_color`) for one uniform code path
  at a small per-fragment cost. Recommend (a) given the perf posture.

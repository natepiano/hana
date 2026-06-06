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

> **Update (glyph_instancing Step 4b, 2026-06-06):** the per-run mesh path
> described below was deleted — `RunRenderData` / `RunStorage` /
> `build_run_render_data` / `commit_run_storage` no longer exist, and text
> renders only through the batched-records path (`GlyphInstanceRecord` /
> `RunRecord` GPU tables expanded by `slug_text_vertex_pull.wgsl`). A color
> glyph therefore lands as **N glyph records with a brush field** (one per
> COLR layer) in its batch's instance table, not N layer-quads in a run
> mesh; everything below about outlines, packing, coverage, and brushes is
> unchanged.

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
transform and an optional clip region). An outline is exactly what `packing.rs`
already turns into curves/bands, and coverage is exactly what the shader already
evaluates. So a color glyph expands into N layer-quads in the run mesh; the only
genuinely new pieces are (a) a per-layer **brush** (solid color or gradient)
replacing the single per-run `fill_color`, (b) gradient evaluation in the shader,
and (c) clip evaluation — a clipped layer multiplies its coverage by a clip
outline's coverage, reusing the same analytic band path a second time.

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
  Brush, clip: Option<Glyph> }`. `clip` is a COLRv1 `PaintGlyph` /
  `PaintClipBox` region captured as an outline (a clip box is converted to a
  four-segment rectangle outline at capture; `None` = unclipped), so box and
  outline clips reduce to one mechanism. Color status is marked on
  `PositionedGlyph` at the shaping→build boundary so run build never re-queries
  the font.
- **`Brush`** — `Solid(PaletteEntry) | LinearGradient {…} | RadialGradient {…}
  | SweepGradient {…}`. `PaletteEntry = Static(LinearRgba) | Foreground`. A whole
  gradient brush has no top-level `Foreground` state, but its individual color
  stops do: COLRv1 stops may reference the 0xFFFF foreground index, so
  `StopRecord = Static(LinearRgba) | Foreground` and the shader resolves a
  foreground stop per stop (decision D2; corrects the earlier "unrepresentable"
  claim, M6). Extraction never bakes a color for the 0xFFFF foreground index — it
  stores `Foreground`, resolved in-shader from `uniforms.fill_color` (both color
  and alpha).
- **`GlyphKey`** — gains a `Palette` newtype field so a color glyph's identity
  includes its palette (palette 0 only until a multi-palette / dark-mode need
  appears). The palette, brush, and layer indices are newtypes mirroring the
  existing `FontKey` / `GlyphKey` pattern, and their ranges are validated at pack
  time so an out-of-range index cannot reach the GPU.
- **`QuadRecord { glyph_outline_index, brush_index, layer_index,
  clip_outline_index }`** — one storage entry per layer-quad, a `ShaderType`
  struct at binding 106. The mesh keeps a single per-quad index in `UV_1.x` that
  addresses it; `glyph_outline_index` points into the deduped `glyphs` buffer. The
  main fragment takes one indirection
  (`glyphs[quad_records[i].glyph_outline_index]`); the prepass does not — color
  runs are `AlphaMode::Blend` (below) and so are excluded from the opaque depth
  prepass, which keeps its direct `glyph_index(uv)` coverage path. Pure-mono runs
  never address a `QuadRecord`. `clip_outline_index` points into the same deduped
  `glyphs` buffer (a clip outline packs exactly like a glyph outline); a no-clip
  sentinel marks an unclipped layer, and a clipped layer costs one extra
  indirection (`glyphs[quad_records[i].clip_outline_index]`) plus a second
  coverage evaluation. `GlyphRecord` stays monochrome-only — per-layer data lives
  in `QuadRecord`, not a widened glyph record.
- **`BrushRecord` / `StopRecord`** — `Brush` → `BrushRecord` via an explicit
  `impl From<&Brush>` with `#[repr(u32)]` tags (`BrushTag`, `ExtendMode`,
  `CompositeMode`). A `ShaderType` layout test asserts size+offsets match the
  WGSL structs, mirroring the `packing.rs` record tests.
- **GPU bindings** — `brushes` (104), `gradient_stops` (105), `quad_records`
  (106) are declared on the single shared `TextExtension`, so every run supplies
  them (a StandardMaterial extension is one bind-group layout, and wgpu requires
  the bound group to match it). A monochrome run binds one app-global
  `ColorBindings::empty()` — shared zero-length brush / stop / quad tables,
  reached through the `RenderMode::Text` arm and never read by the shader's mono
  path. `ColorBindings::empty()`'s name and doc comment are the canonical
  explanation for why a mono run carries empty color tables. Confirm 104–106 are
  free against StandardMaterial + OIT bindings.
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
- **Clipping** — the painter keeps a clip stack alongside the transform stack:
  `push_clip` pushes a glyph outline, `push_clip_box` an axis-aligned rectangle,
  `pop_clip` pops. When a layer is emitted under one or more clips it captures the
  composed clip (transformed by the same affine as its outline, a box reduced to a
  rectangle outline) into `ColorLayer.clip`; each clip outline packs into the
  deduped `glyphs` buffer and the layer's `QuadRecord` carries its
  `clip_outline_index`. The fragment evaluates the clip's coverage through the
  band path and multiplies it into the layer coverage (`coverage *=
  clip_coverage`), so color cannot spill past the clip region; nested clips
  compose by multiplying each enclosing clip's coverage. An unclipped layer stores
  `None` / the no-clip sentinel and pays nothing. COLRv0 carries no clips, so this
  path is dormant for Phase 7 and the shader-side multiply lands with the COLRv1
  work in Phase 8.
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
- `push_clip` / `push_clip_box` / `pop_clip` maintain the clip stack; the
  composed clip is captured into `ColorLayer.clip` at emit time (per the clipping
  rule above). COLRv0 fonts emit no clip callbacks, so the field is always `None`
  for Phase 7 fonts.
- A solid paint's palette index `0xFFFF` → `PaletteEntry::Foreground`; any other
  index → CPAL lookup → sRGB→linear `PaletteEntry::Static`.
- `paint` resolves a `Paint` into a `Brush`; gradients capture endpoints, stops,
  and `ExtendMode`.
- `push_layer` / `pop_layer` carry the composite mode (alpha-over only at
  first).

Detect a color glyph by `paint_color_glyph(...)` returning `Some`; otherwise
fall through to the monochrome `load_glyph_by_id_from_face`. Unsupported `Paint`
variants log and skip rather than panicking. An out-of-range palette index skips
the layer with a dedup warning (decision M31, consistent with the other
log-once-and-skip font-data fallbacks); a `PaintColrGlyph` reference to a missing
glyph skips that subtree. Every fallback logs once and never panics.

### Phase 4 — Glyph cache (`runtime/glyph_cache.rs`)

Cache the flattened, packed color layers the same way `GlyphOutlineCache` caches
monochrome packed glyphs, keyed off `GlyphKey` plus the new `Palette` field.
Return the `CachedGlyph` enum. Adding the `Palette` field to `GlyphKey` and
widening the cache accessor from `&GlyphOutline` to `&CachedGlyph` migrates the
monochrome call sites in the same change. The accessor is a total `layer_iter()`
(decision D1): `Color` yields its layers, `Monochrome` yields one synthetic
`Solid(Foreground)` layer, so Phase 5 iterates both uniformly with no partial
accessor to panic. Color-glyph layers are static per `(font, glyph id, palette)`,
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

All run buffers — curves/bands/glyphs and the brush / quad / stop buffers — ride
the same stable `Handle<ShaderBuffer>` with in-place `set_data()` re-upload
(stored in `RunStorage`, overwritten in `commit_run_storage` on rebuild), so a
per-frame-updated color run allocates nothing. The quad↔brush pairing is modeled
structurally (one `QuadRecord` per layer-quad), not as length-correlated parallel
`Vec`s.

When a layer's baked transform collapses its bounds to zero width or height, the
quad is dropped with a warning before packing (`packing.rs` divides by extent).
In a color or mixed run every quad carries a `QuadRecord`; a monochrome glyph in
such a run emits one quad with a `Solid(Foreground)` brush at layer 0.

A clipped layer's clip outline packs into the same deduped `glyphs` buffer as the
layer outlines, and the layer's `QuadRecord` records its `clip_outline_index`; an
unclipped layer records the no-clip sentinel. COLRv0 (Phase 7) emits only the
sentinel, so the `glyphs` buffer gains no clip entries until COLRv1 layers arrive
in Phase 8.

### Phase 6 — Material + shader (`render/material.rs`, `shaders/slug_text.wgsl`)

Add the `brushes` (104), `gradient_stops` (105), and `quad_records` (106)
storage bindings and the `RenderMode::ColorLayer` discriminant. Color runs set
`AlphaMode::Blend`. In the fragment shader, after `render_coverage`:

- Read the quad's `QuadRecord` for its glyph outline, brush, layer index, and
  clip outline index.
- **Solid brush** — `rgb = brush.color.rgb`; a `Foreground` brush reads
  `uniforms.fill_color` (color and alpha). Identical cost to today for the
  constant case.
- Apply the per-layer depth term `oit_pos.z += f32(layer_index) * stride` before
  `oit_draw` (the per-run bias already rides in `uniforms.oit_depth_offset`).

`final_alpha = coverage * brush_alpha`, feed `base_color`, PBR + OIT tail
unchanged. Phase 6 lands solid brushes only; the gradient branch is Phase 8.
Phase 6 plumbs `clip_outline_index` through the `QuadRecord`, but COLRv0/solid
layers only ever carry the no-clip sentinel, so the clip-coverage multiply itself
lands with the COLRv1 work in Phase 8. Pure-monochrome runs stay on the
`RenderMode::Text` single-`fill_color` path.

Add a test that bindings 104–106 do not collide with StandardMaterial + OIT, one
that a color run's material is built with `AlphaMode::Blend`, and one that a
monochrome run resolves to the shared `ColorBindings::empty()` (so a later
cleanup that drops the empty tables fails the bind-group layout loudly). Color
runs (Blend) and monochrome runs (Opaque) land in different render phases, so
they do not batch together — measure the bind-group / draw-call impact in
Phase 7.

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

Clip handling lands here too (decision D4: model clipping now, not deferred to
the conformance tail). The shader reads `clip_outline_index`, evaluates the clip
outline's coverage through the band path, and multiplies it into the layer
coverage; nested clips compose by multiplying each enclosing clip. Gate: a
`PaintGlyph` clipping a multi-layer child (e.g. a Noto/Twemoji part) renders with
no color past the clip outline at 1× / 5× / 10× zoom.

### Phase 9 — Conformance tail

Sweep gradients, nested `PaintColrGlyph`, and `PaintComposite` / blend modes
beyond alpha-over. Clip regions (`PaintGlyph` / `PaintClipBox`) are not here —
they land in Phase 8 (decision D4). OIT alpha-over already handles the common
overlap case, so non-alpha-over composite modes are last and optional. Survey
Noto Color Emoji first to confirm which composite modes any target font actually
uses before implementing them.

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
- Monochrome text mesh + its 100–103 buffers stay byte-identical (the color path
  is additive). Mono runs bind the shared `ColorBindings::empty()` for 104–106,
  which the `RenderMode::Text` path never reads; the single-`fill_color` route
  must not change.
- An N-layer emoji composites top-to-bottom under OIT, not shuffled. Verify OIT
  linked-list ordering with a 40+-layer emoji.
- The Phase 1 shaping-output regression (glyph ids + advances across the parley
  bump) stays green.
- Extracting a cyclic or adversarial COLR font terminates without hanging and
  logs the skipped subtree.
- The example's frame cost is gated against the monochrome render floor by a
  normalized per-layer budget `frame_time ≤ mono_floor × (1 + k · layer_ratio)`
  (`layer_ratio = color_layers / mono_quads`; decision D3), so a per-layer
  regression trips the gate. The added fill-rate from coplanar layers is recorded
  alongside the budget.

## Team review (cycle 1) — auto-recorded refinements

Five-lens review (correctness, architecture, risk, type system, implementation
quality). No premise-challenge surfaced: no lens argued the layer-as-N-coverage
approach cannot achieve the intent. The items below converged to a single
in-intent outcome and are accepted; the three genuine choices are under
**Proposed user decisions**.

- **M1 — Pin the layer-depth stride (5-lens consensus). Accepted.** Define
  `LAYER_DEPTH_STRIDE` as a named constant with documented precision headroom;
  choose `LayerIndex` width as `u8` (≤255 layers, truncate-and-warn beyond) so
  the depth term cannot exhaust f32 precision; add the per-layer term *additively*
  to `uniforms.oit_depth_offset` (compose, never overwrite). The OIT-ordering gate
  (line 378) must run the 40+-layer fixture across several `oit_depth_offset`
  depth ranges (e.g. 0, 1, 10, 100), not one, since mis-sort only appears at
  certain depths.
- **M2 — Bindings 104–106 audit is a hard go/no-go. Accepted.** Keep the existing
  Phase 6 gate (line 274) but run it *first*, before any Phase 6 implementation:
  instantiate `TextExtension` with dummy 104–106 buffers + OIT and confirm the
  material compiles. Record the verified Bevy version in a `material.rs` comment.
- **M3 — Intern brushes and stops per run. Accepted.** `RunPacker` deduplicates
  identical `Brush` (and `StopRecord`) values within a run via a hash map;
  `QuadRecord` indexes the deduped set. Bound and document the per-run
  brush/stop/quad counts and the overflow path so a pathological run is a named
  internal error, not silent truncation.
- **M4 — Indices are checked newtypes. Accepted.** `Palette`, `BrushIndex`,
  `LayerIndex`, `GlyphOutlineIndex` are newtypes with constructors that validate
  against the run's buffer lengths at pack time (this is what makes
  "an out-of-range index cannot reach the GPU" true rather than aspirational).
  Pin `Palette` as `u16` (CPAL palette-index width), palette 0 only for now.
  Run-overflow (internal invariant) uses `debug_assert`/error; font-data issues
  keep the log-and-skip path.
- **M5 — Widen the parley 0.9→0.10 regression gate. Accepted.** The glyph-id +
  advance check (line 179) misses line-break/bidi/cluster regressions. Add a
  mesh-level snapshot on the multi-line RTL/CJK sample asserting cluster
  boundaries and line-break positions, so reflow changes cannot pass green.
- **M6 — Correct the "gradient cannot be Foreground" claim. Accepted (factual
  correction).** COLRv1 gradient color stops *can* use the 0xFFFF foreground
  index, so the line-100–104 claim that the state is unrepresentable is
  inaccurate about COLR. The factual correction is recorded here; whether to
  *support* foreground stops or warn-and-fallback is **Decision D2**, and the
  `StopRecord` layout follows from that choice.
- **M7 — `RenderMode::ColorLayer` discriminant. Accepted.** Keep the
  test-asserted Rust↔WGSL pairing (consistent with the existing `Text`/`PunchOut`
  `From<u32>`); single-source the discriminant value if convenient. The existing
  pattern is sufficient.
- **M8 — Mixed-run monochrome glyph spelled out. Accepted.** A mono glyph in a
  color/mixed run emits exactly one quad: `Solid(Foreground)` brush,
  `layer_index` 0, alpha `= coverage * uniforms.fill_color.a`, depth at the run's
  nominal offset. Add a mixed-run OIT-ordering gate (one mono letter + one
  40+-layer emoji in the same run).
- **M9 — Zero-extent layer guard before packing. Accepted.** A layer whose
  baked-transform bounds collapse to zero width or height is skipped with a
  `warn!` *before* packing — `packing.rs` clamps `extent.max(1.0)` and so would
  emit degenerate bands rather than NaN, wasting shader work, if a collapsed
  layer slipped through (sharpens lines 160–161, 252–253).
- **M10 — Font-fallback logging is deduplicated. Accepted.** Out-of-range palette
  index, missing `PaintColrGlyph` target, and unsupported `Paint` each log once
  per `(font, issue)` via a dedup set to avoid per-frame spam; the cyclic-font
  termination fixture (gate line 381) covers the recursion cap.
- **M11 — sRGB→linear at extraction, verified. Accepted.** CPAL colors and stops
  convert to linear at extraction; the shader consumes linear directly (no second
  conversion). Add a Phase 3 unit test asserting a known sRGB color maps to its
  linear value.
- **M12 — Non-uniform/shear affine survey is a Phase 8 prerequisite. Accepted.**
  Survey Noto Color Emoji / Twemoji for non-uniform or shear layer affines before
  Phase 8 ships (already noted lines 300–304); record the survey result in the
  Phase 8 commit so the deferral is evidence-backed.
- **M13 — Measure mixed-run fragment cost separately. Accepted.** Phase 7 records
  pure-mono, pure-color, and 50/50 mixed fragment cost separately, since a mixed
  run routes every quad through `quad_records` (one indirection on the mono quads
  too).
- **M14 — Pin the example builder-chain order. Accepted.** Document
  `.with_stable_transparency()` placement (after the OrbitCam helper, before
  panels) and which faces set `AlphaMode::Blend`; mono panels keep their current
  alpha mode.

## Team review (cycle 2) — auto-recorded refinements

Second five-lens review (correctness, architecture, risk, type system,
implementation quality), building on cycle 1. No premise-challenge surfaced
again. One genuine new choice (clip handling) is added below as **D4**; the rest
converged to a single in-intent outcome and are accepted. Items that sharpen a
cycle-1 entry name it.

- **M15 — Painter transform-stack timing + per-layer outline builder. Accepted.**
  Phase 2's probe painter must record *when* `push_transform`/`pop_transform`
  fire relative to `outline_glyph` (the transform stack is live at outline
  emission). Phase 3 instantiates a fresh `QuadraticOutlineBuilder` per
  `outline_glyph` (one builder per layer, never reused across layers) and applies
  the composed affine to the extracted control points after extraction. Sharpens
  Phase 2/3 (lines 156–161, 201).
- **M16 — Pin the actual `LAYER_DEPTH_STRIDE` value with an f32 formula. Accepted
  (sharpens M1).** Define `max_z = max(command_index · per_run_stride) + 255 ·
  LAYER_DEPTH_STRIDE` and require it to stay inside the f32 mantissa window;
  choose a small stride (≈1e-5, not 1.0) so the per-layer term keeps distinct
  ULPs. Add a unit test asserting layers 0..255 stay distinctly ordered across
  `oit_depth_offset` ∈ {0, 1, 10, 100, 1000}; the OIT gate uses a named 40+-layer
  fixture and a programmatic depth-order readback, not visual inspection alone.
- **M17 — OIT linked-list capacity stress (NEW, critical). Accepted.** The
  ordering gate verifies *sort order* but never *capacity*. A picker-grid-scale
  run (e.g. 100 emoji × 40 coplanar layers ≈ 4000 transparent quads) can exceed
  Bevy's per-pixel OIT layer budget and silently drop/corrupt fragments. Add a
  Phase 7 stress gate at picker scale; document the assumed OIT max-per-pixel
  layer count and the Bevy version it was verified against; record the fill-rate
  floor. Cross-references the diegetic-text-perf OIT-dominant-cost note.
- **M18 — Prepass exclusion verified, not assumed (critical). Accepted.** Confirm
  (and comment in `slug_text.wgsl`) that the `#ifdef PREPASS_PIPELINE` path reads
  only `RenderMode::Text` + binding 103 and never references 104–106, so a
  Blend-excluded color run cannot reach the prepass with an unbound 106. Add a
  side-by-side mono+color prepass test. Folds in the prepass doc note (cycle-2
  correctness C6).
- **M19 — Bindings 104–106 audit gets a CI gate + version comment (sharpens
  M2). Accepted.** Beyond running the compile probe first, record the verified
  binding layout in a `material.rs` doc comment naming the Bevy version, the
  StandardMaterial binding ceiling, and the OIT bindings in use; run the M2 probe
  in CI as a required gate before any Phase 6 merge.
- **M20 — `ColorBindings::empty()` ownership, injection point, live test.
  Accepted.** It is a startup-initialized resource holding one shared zero-length
  brush/stop/quad handle set (created once with size-0 buffers, app lifetime).
  Document which module selects it for mono runs (the run-build caller, before
  `text_material`). Replace the comment-only guard (line 276) with a live test
  asserting mono runs bind the shared handle and that mono and color materials
  produce an identical bind-group layout.
- **M21 — `RunRenderData` buffer ownership + byte-identical check. Accepted.**
  Each of curves/bands/glyphs/brushes/stops/quad_records is its own stable
  `Handle<ShaderBuffer>` in `RunStorage` (or, if concatenated, with documented
  offsets); a pure-mono run never creates the color buffers. The Phase 7
  byte-identical gate (line 373) snapshots only the 100–103 buffers.
- **M22 — `GlyphKey::Palette` lands before Phase 3 extraction. Accepted.** Move
  the field addition out of Phase 4 into Phase 2 / early Phase 3, since extraction
  must key the cache by `(font, glyph id, palette)`. Sharpens Phase 4 (line 222).
- **M23 — Interning bounds, fallible packer, validated-index boundary (sharpens
  M3/M4). Accepted.** Index width `u16` (≤65535 distinct brushes/stops per run);
  `RunPacker::push_*` returns `Result<_, RunPackError::{BrushesExceeded,
  StopsExceeded}>` — a named error, never silent truncation or a bare panic;
  `finish()` asserts the brush/stop/quad_record lengths agree. Keep the
  GPU-serialized `u32` distinct from the validated newtype so a deserialize path
  cannot bypass the pack-time check. Add layout tests and invalid-`#[repr(u32)]`-
  discriminant-rejection tests for `BrushRecord`/`StopRecord`/`QuadRecord`.
- **M24 — `AlphaMode::Blend` enforced structurally (sharpens M14). Accepted.** A
  typed color-run material constructor (or an assertion inside `text_material`)
  makes a color run un-buildable without `Blend`, so a future refactor that
  bypasses the builder cannot silently route a color run through the opaque pass;
  mono keeps `Opaque`.
- **M25 — `layer_iter()` is an `ExactSizeIterator` (sharpens D1). Accepted.** The
  total accessor reports its length; `Monochrome` yields exactly one synthetic
  `ColorLayer { outline, brush: Solid(Foreground), layer_index: 0, transform:
  Affine2::IDENTITY }`. Phase 5 asserts the reported count before emitting quads.
- **M26 — `StopRecord` layout for foreground stops (sharpens D2). Accepted.**
  `StopRecord { color: vec4<f32>, is_foreground: u32 }`; when `is_foreground` the
  shader reads `uniforms.fill_color` (rgb *and* a). Static stops keep their CPAL
  color+alpha; a foreground stop follows the run color and alpha by COLR
  definition (resolves cycle-2 risk R9). Add the `From<&GradientStop>` impl plus a
  layout test in Phase 8.
- **M27 — Foreground brush alpha cascade spelled out. Accepted.** For a
  `Solid(Foreground)` brush, `final_alpha = coverage · uniforms.fill_color.a`,
  identical to `Solid(Static)`; coverage always multiplies the foreground alpha
  (sharpens lines 264–265).
- **M28 — Non-uniform/shear survey precedes `BrushRecord` layout + a transform
  marker (sharpens M12). Accepted.** Run the Noto/Twemoji shear survey *before*
  locking the `BrushRecord` layout (not after Phase 8 starts), since a shear find
  widens the record. Add a `TransformKind { Uniform | Affine }` (or `has_shear`)
  marker so Phase 7 warns on shear and Phase 8 cannot silently mis-render a
  sheared gradient; document `BrushRecord`'s current uniform-affine assumption.
- **M29 — Concrete cyclic-font fixture + cap aligned to `LayerIndex` (sharpens
  M10 / Phase 2). Accepted.** Build a real cyclic/adversarial COLR fixture (TTX or
  synthetic) for gate line 381; set the recursion cap to `u8::MAX` to match
  `LayerIndex`; Phase 2's probe records whether ttf-parser already bounds
  `PaintColrGlyph` recursion.
- **M30 — Enumerate Paint-variant support + structured dedup logging. Accepted.**
  Phase 3 classifies every COLRv1 `Paint` as supported-now / deferred-Phase-8 /
  deferred-Phase-9, warns once per `(font, variant)`, and a Phase 7 test asserts a
  font using a deferred paint extracts with a reduced layer count and a warning
  (sharpens line 212).
- **M31 — Out-of-range palette index → skip-with-warning. Accepted.** Resolve the
  "fall back to palette 0 *or* skip" ambiguity (lines 213–216) to skip the layer
  with a dedup warning, consistent with the other log-once-and-skip font-data
  fallbacks (M10).
- **M32 — Parley bump: pinned pair, rollback note, shaping-output snapshot
  (sharpens M5). Accepted.** Pin `parley` and `harfrust` versions together in
  `Cargo.toml`; record the rollback cost (is `outline.rs` API-coupled to parley?)
  in the Phase 1 commit. M5's "mesh-level snapshot" is a structured shaping-output
  snapshot — glyph ids, advances, `glyph.cluster` boundaries, line-break positions
  serialized as JSON — not a brittle mesh-vertex byte compare.
- **M33 — `sRGB→linear` test concretized (sharpens M11). Accepted.** Assert
  `srgb_u8(255,255,255).to_linear()` → `1.0` per channel and a mid-tone
  (`0x808080` → ≈0.216) via Bevy's conversion, covering both ends of the curve.
- **M34 — D3 perf gate pinned + M13 reframed. Accepted.** D3's whole-frame budget
  is measurable, but its terms are not pinned: set `k`, define `mono_floor` (a
  pure-mono 40+-emoji run under the same lighting/OIT settings, captured per Bevy
  version + machine), and state the gate is run manually via fairy_dust
  `with_perf_mode` (continuous + `AutoNoVsync`) with fixed, focused window
  geometry. M13's per-fragment isolation is not measurable with current tooling,
  so reframe it as visual correctness of pure-mono / pure-color / 50-50 runs plus
  a documented ≈≤5% indirection hypothesis; note that a future instancing path
  would remove the mixed-run per-quad indirection entirely.
- **M35 — Example spec concreteness (sharpens M14). Accepted.** Add an
  alpha-mode column to the face matrix (which faces are `Blend`); pin
  `CameraHomeTarget` to the Front hero; start the cube spin paused (P to spin) so
  each face screenshots cleanly; confirm `.with_stable_transparency()` is an
  existing fairy_dust capability or add it in this PR.
- **M36 — Implementation-notes section + minor type notes. Accepted.** Add an
  "Implementation notes" subsection: dedup happens before index assignment; the
  shader indirection path is `quad_index(uv) → quad_records → brush_index →
  brushes`, and mono runs skip the indirection. Add a `TODO` plus a multi-palette
  cache-separation test for the `Palette` newtype against a future dark-mode need;
  note why the `CachedGlyph` enum alone suffices versus a run-build typestate.
- **M37 — Zero-extent guard scope clarified (sharpens M9). Accepted.** The layer
  guard fires after the affine, before packing; separately, the run builder's
  `clipped()` returns `None` for a zero-extent *quad*. Document both as distinct
  layer-level and quad-level filters so neither is mistaken for the other.
- **M7 reaffirmed.** The type lens proposed compile-time `RenderMode`↔WGSL
  discriminant enforcement; M7 already accepts the test-asserted pairing with
  "single-source the value if convenient," which covers it. No new action.

## Proposed user decisions

Genuine choices the review could not settle to one answer. Each must be ruled on;
the rest above is accepted.

- **D1 — Cache return type and mono-path accessor.**
  Severity: important. Source: Architecture + Type system (conflicting
  recommendations). Class: design-improvement. Status: proposed.
  *Problem:* the plan returns `CachedGlyph = Monochrome | Color` and adds an
  `outline_ref()` helper for the mono call sites. Two lenses object for opposite
  reasons: the type lens says `outline_ref()` re-introduces a partial accessor
  that panics on the `Color` variant (defeating the enum's exhaustiveness); the
  architecture lens says the enum itself adds a branch to the per-glyph mono hot
  path that today returns `&GlyphOutline` directly.
  *Impact:* sets the Phase 4 cache API and the Phase 5 expansion loop; wrong
  choice means a rewrite at integration or a latent panic.
  *Options:* (A) keep enum + `outline_ref()` as written; (B) keep the enum but
  replace `outline_ref()` with a total accessor (e.g. `layer_iter()` that yields
  one synthetic `Solid(Foreground)` layer for mono) so callers handle both
  variants uniformly and Phase 5 iterates layers identically; (C) drop the enum —
  keep `get()` returning `&GlyphOutline` with an optional color-layer payload, so
  the mono hot path keeps zero branching at the cost of the enum's explicitness.
  **Decision: B — keep `CachedGlyph` but replace `outline_ref()` with a total
  `layer_iter()` accessor (one synthetic `Solid(Foreground)` layer for mono); no
  partial accessor to panic, and Phase 5 iterates layers uniformly.**

- **D2 — Foreground-indexed gradient color stops.**
  Severity: important. Source: Type system + Correctness. Class: design-improvement
  (scope/behavior). Status: proposed.
  *Problem:* COLRv1 gradient stops may reference the 0xFFFF foreground index
  (resolved from the run's text color), which the current data model cannot
  represent (M6). The fix changes intended behavior and `StopRecord` layout.
  *Impact:* fonts that use a foreground stop render with a wrong color if
  unsupported; supporting it adds per-stop foreground resolution (a little shader
  ALU) and a `Foreground` case in `StopRecord`.
  *Options:* (A) support it — `StopRecord` carries `Static | Foreground`, shader
  resolves per stop (correct per COLR, slightly more ALU); (B) pin stops to
  Static-only, warn-and-fallback on a foreground stop, defer real support to the
  Phase 9 conformance tail.
  **Decision: A — support it. `StopRecord` carries `Static | Foreground`; the
  Phase 8 shader resolves a foreground stop from `uniforms.fill_color` per stop.
  Update the line-100–104 data-model text accordingly (per M6).**

- **D3 — `color_emoji` perf-gate methodology.**
  Severity: minor. Source: Implementation quality. Class: design-improvement.
  Status: proposed.
  *Problem:* the Phase 7 gate (line 384) records the example's frame cost "not
  treated as a regression." With a 40+-layer picker grid the example can become
  its own bottleneck, so the gate cannot catch a per-layer regression later.
  *Impact:* a standing visual regression that does not actually gate performance.
  *Options:* (A) keep one comprehensive example, frame cost recorded as an
  informational floor (as written); (B) add a normalized per-layer budget
  (`frame_time ≤ mono_floor × (1 + k · layer_ratio)`) so the gate tracks per-layer
  cost; (C) split a lightweight single-emoji example as the perf gate and keep the
  full picker as a visual-only check.
  **Decision: B — gate on a normalized per-layer budget
  `frame_time ≤ mono_floor × (1 + k · layer_ratio)` (with `layer_ratio =
  color_layers / mono_quads`), so a per-layer regression trips the gate without
  splitting the example.**

- **D4 — COLRv1 clip handling and nested-clip scope.**
  Severity: important. Source: Correctness + Architecture. Class: design-improvement
  (data model + scope/behavior). Status: resolved.
  *Problem:* the data model and phases never address clipping. ttf-parser's
  `colr::Painter` surfaces `push_clip` / `push_clip_box` / `pop_clip` alongside
  the transform callbacks; the design's painter handles transforms but says
  nothing about clips. The common `PaintGlyph(glyph_id, fill/gradient)` maps
  directly to one `(outline, brush)` layer — the referenced outline *is* the fill
  region, so no separate clip field is needed. The gap is *nested* clipping: a
  `PaintGlyph`/`PaintClip` whose child is itself multi-layer (e.g.
  `PaintColrLayers`), where the outer outline must clip several inner layers. The
  flat-layer model has no way to express "this layer is also intersected with that
  clip outline." Noto Color Emoji and Twemoji use clip regions for parts.
  *Impact:* a glyph that nests a multi-layer child under a clip renders with color
  spilling past the intended region; the choice sets the Phase 3 data model
  (whether `ColorLayer` carries clip data) and the feature's scope.
  *Options:* (A) model clipping now — add a clip representation to `ColorLayer` (a
  clip-outline index or pre-intersected coverage) and handle nested clips in
  Phase 8 with gradients (most correct, most data-model + shader work); (B)
  confirm the common `PaintGlyph→fill/gradient` case is already covered by the
  `(outline, brush)` layer, survey Noto/Twemoji for nested-clip glyphs, and defer
  genuine nested-clip handling to the Phase 9 conformance tail — warn-and-skip the
  clipped subtree until then so nothing renders *wrong*, only incomplete; (C)
  reject (warn + skip) any glyph that pushes a clip beyond its immediate
  `PaintGlyph` fill region until full clip support lands (safest visually, drops
  whole glyphs that use nested clips).
  *Recommendation:* B — it matches the existing conformance-tail structure, ships
  Phase 7/8 for the overwhelming-majority case, records the nested-clip count from
  the survey so the deferral is evidence-backed, and never renders a wrong color
  in the meantime.
  **Decision: A — model clipping now. Add a clip representation to `ColorLayer` (a
  clip-outline index or pre-intersected coverage) and handle nested clips in
  Phase 8 alongside gradients. The painter tracks the `push_clip` / `push_clip_box`
  / `pop_clip` stack; the data model carries clip data from Phase 3, not deferred
  to the Phase 9 conformance tail.**

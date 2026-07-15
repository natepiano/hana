# Slug analytic text renderer

Slug is `hana_diegetic`'s sole text renderer. It draws each glyph from its
quadratic Bézier contours with per-pixel analytic coverage — no distance-field
atlas, no pre-rasterized texels. The glyph silhouette is evaluated in the
fragment shader from packed curve data.

The name "Slug" is the algorithm origin (Eric Lengyel's Slug reference shaders,
https://jcgt.org/published/0006/02/02/). Only the algorithm shape (band-indexed
quadratic curves, non-zero winding + nearest-curve distance) is borrowed; the
implementation is an original WGSL/Rust port. Attribution lives in
`crates/hana_diegetic/NOTICE`.

## Module map

The `text/slug/` module owns everything text-specific: shaping input, font
outline extraction, the glyph cache, and per-run positioning. The generic
analytic-coverage renderer (records, packing, material, shader, batch store)
lives in `render/analytic_paths/` and is shared with panel-line vector marks.

`text/slug/`:
- `mod.rs` — `SlugPlugin` (only inits the `GlyphCache` resource). Re-exports
  `GlyphCache`, `PositionedGlyph`, `PreparedTextRun`, `RunStorageKey`,
  `glyph_quad_extents`.
- `glyph/outline.rs` — outline extraction with `ttf-parser`. `Glyph`,
  `OutlineError`, `load_glyph_by_id_from_face`,
  `font_glyph_id_has_visible_outline`, and the cubic→quadratic conversion.
- `glyph/mod.rs` — `build_packed_glyph`: the one-line bridge from a font
  `Glyph` to `render::build_packed_path`.
- `glyph/coverage_probe.rs` — test-only CPU model of the shader's distance /
  band / anisotropic-sample / hairline logic, for debugging coverage math.
- `runtime/run.rs` — CPU run/cache types: `FontKey`, `GlyphKey`,
  `GlyphInstance`, `TextRun`, `BuiltTextRun`, `GlyphOutlineCache`,
  `CachedGlyphOutline`.
- `runtime/glyph_cache.rs` — the `GlyphCache` resource, run preparation, and
  the shared-atlas GPU upload.
- `render/run_data.rs` — `glyph_quad_extents`: padded, clipped quad rect + UVs
  for one glyph instance.

`render/analytic_paths/` (shared renderer; `AnalyticPathPlugin` registered from
`render/mod.rs`):
- `geometry.rs` — `QuadraticSegment`, `Bounds`, `PathContour`, `PathOutline`.
- `packing.rs` — GPU records and `build_packed_path` / `DEFAULT_BAND_COUNT`.
- `material.rs` — `PathExtendedMaterial`, `RenderMode { Text = 1, PunchOut = 2 }`.
- `batching.rs` — `TextRunBatchStore`, `PathBatchKey`, run upsert/removal.
- `analytic_path.wgsl` / `analytic_path_vertex_pull.wgsl` — the coverage shader.

Panel and world text drive Slug from `render/panel_text/{shaping,batching}.rs`
and `render/world_text/`. They own the ECS systems; `text/slug` owns the data.

## Glyph pipeline (font bytes → packed curves)

1. **Outline extraction** (`glyph/outline.rs`). `load_glyph_by_id_from_face`
   parses the exact resolved font face with `ttf-parser` and walks the glyph
   outline into `Glyph { character, id, bounds, contours }`. Contours are
   `Vec<PathContour>` of `QuadraticSegment`s in font design-space units.
   - Lines are encoded as degenerate quadratics (control = midpoint).
   - TrueType quadratics pass through directly.
   - CFF/CFF2 cubics are converted adaptively to quadratics
     (`append_cubic_quadratics`): tangent-intersection control point with a
     midpoint fallback, recursively split until the error at t=0.25/0.75 is
     within `CUBIC_TO_QUADRATIC_TOLERANCE` (0.25 design units) or
     `CUBIC_TO_QUADRATIC_MAX_DEPTH` (10) is hit. This is why Latin TrueType and
     CJK CFF fonts both render.
   - A glyph with no bounding box (space, other blank glyphs) has no visible
     outline; the loader never reaches, and callers skip it (see cache below).

2. **Packing** (`glyph/mod.rs` → `render::build_packed_path`). The outline is
   band-indexed into a `PackedPath`: `CurveRecord`s (one per quadratic, with
   precomputed distance-solver coefficients) plus `BandRecord`s. Text uses the
   uniform `BandLayout` with `DEFAULT_BAND_COUNT` (96) bands per axis. Along-Y
   bands hold every curve a +x winding ray can cross in each y-slab; along-X
   bands do the same by x. Both are sorted by descending max-coordinate so the
   shader's per-fragment scan breaks early. Text contours pack with
   `min_feature = 0` and `fade_exponent = 0` — text never dilates (that is the
   hairline path for panel lines).

## Runtime: glyph cache and run preparation

`GlyphCache` (`runtime/glyph_cache.rs`) is the single `Resource`. It owns:
- `outline_cache: GlyphOutlineCache` — the CPU outline cache **and** the shared
  append-only GPU atlas (curve / band / glyph-record tables).
- `units_per_em: HashMap<FontKey, f32>` — parsed once per font.
- `batch_store: TextRunBatchStore` — the batched-run routing state.
- `atlas: Option<PathAtlasHandles>` + `uploaded_revision` — GPU upload state.
- `preprocess_version` — bumped to invalidate cached glyph data on a
  preprocessing change (folded into every `GlyphKey`).

### Preparing a run

Callers pass `&[PositionedGlyph]` (a `&ShapedGlyph` plus its resolved `&Font`
and collection index) to `prepare_positioned_run_with_scale(glyphs, anchor,
layout_font_size, placement_scale, band_count)`. For each glyph:
- `FontKey` is derived from `glyph.font_face.blob_id` (the parley-resolved
  face, not the requested style font — this is the cache identity).
- The outline is fetched/packed once via
  `GlyphOutlineCache::get_or_insert_packed_from_face`, returning
  `CachedGlyphOutline::Visible(PackedPath)` or `::Invisible`.
- `Invisible` glyphs are skipped (no instance, no atlas entry). A space-only run
  yields zero instances and is a valid (not failed) prepared run.
- Visible glyphs become a `GlyphInstance` positioned at
  `((x − anchor.x)·scale.x, −(baseline + y − anchor.y)·scale.y)` with
  `bounds_scale = placement_scale · (layout_font_size / units_per_em)`.
  Non-uniform X/Y scale is supported (panel children scale axes independently).

The result is a `PreparedTextRun` wrapping a `TextRun` of `GlyphInstance`s.
Steady-state per-glyph cost is two map hits (packed outline, `units_per_em`)
plus the instance math; font tables are parsed only on first sighting.

A glyph id absent from the resolved face returns
`OutlineError::MissingGlyphId` and fails the whole run — Slug does not
substitute a fallback face (parley fallback is disabled for Slug; a missing
glyph is an explicit unsupported state, never a silently-swapped face).

### Shared atlas (`GlyphOutlineCache`)

The atlas is append-only and never evicted. The first time a `GlyphKey` is
packed, its curves/bands/glyph-record are appended to the shared tables with
global offsets, its slot recorded in `record_indices`, and `revision` bumped.
Every run that draws that glyph stores the single global index in its mesh
instead of copying curves per run. `PackedPathRecord` for a text glyph is built
with the glyph bounds, the two band ranges, and `min_feature = 0`.

### GPU upload gotcha — `commit_glyph_atlas`

`commit_glyph_atlas` uploads the three shared tables to `ShaderBuffer` assets
and returns `PathAtlasHandles`. **On atlas growth it creates three NEW buffer
assets and repoints every live text batch material at them.** It must not call
`set_data` with a longer payload: that re-creates the wgpu buffer behind
existing material bind groups, which keep reading the dead buffer, so glyphs
packed after a material's creation would render invisible. A frame that packs no
new glyph (same `revision`) re-uploads nothing and reuses the handles. It
returns `None` before any glyph is packed, so no zero-length buffer is created.
Only text-owned batch materials are repointed — other `PathExtendedMaterial`
producers (panel lines, probes) own separate atlases.

## Feeding the batched renderer

Slug produces per-glyph geometry; `render/analytic_paths` owns the draw. The
handoff, per frame, in the panel/world text systems:

1. `prepare_positioned_run_with_scale` → `PreparedTextRun`.
2. For each `GlyphInstance`, `glyph_quad_extents(glyph, scale, clip_rect)`
   (`render/run_data.rs`) computes the padded quad rect and UVs. The quad is
   padded by `GLYPH_PADDING_DESIGN_UNITS` (16) design units so the AA ramp
   clears the quad edge; clipping trims the rect to `clip_rect` and remaps UVs
   into the glyph, returning `None` when the clip removes the whole quad (fully
   clipped glyphs drop out). This is how panel overlap/padding/scissor clips
   preserve glyph-local shader coordinates.
3. The extents become `PathQuadRecord`s (rect + coverage UV + material box UV +
   `packed_path_index` into the shared atlas + `render_index` into the run
   table). Runs are keyed by `RunStorageKey` (derived from the label entity, so
   the same label addresses the same batch slot every frame) and upserted into
   `batch_store`. Per-run state (transform, material slot, render mode, OIT
   offset, `aa_flags`, `text_coverage_bias`) lives in `PathRenderRecord`.
4. `commit_glyph_atlas` uploads the shared curve/band/glyph tables.

The vertex-pulling shader (`FRAGMENT_DATA_FROM_BATCHED_PATHS`) expands each
`PathQuadRecord` into four corners and reads its `PackedPathRecord` and
`PathRenderRecord` in the fragment stage.

## Coverage and anti-aliasing (`analytic_path.wgsl`)

Per fragment, `render_coverage` maps the coverage UV to a design-space point,
then evaluates **non-zero winding** (inside/outside) plus **distance to the
nearest curve** (the AA ramp). Both are banded: the along-Y band gives the
complete +x-ray crossing set (winding) and near curves; a second along-X pass
adds near curves the winding pass missed. `curve_winding` solves the quadratic
against the scanline with a half-open y rule so a ray through a join is not
double-counted; `exact_quadratic_distance` uses the precomputed
`solver.xyz` cubic coefficients (one sqrt after the nearest curve is known).

**Text-relevant AA modes** come from the run's `aa_flags`, encoded from the
global `AntiAlias` resource (`Off` / `Anisotropic` / `Supersample` / `Both`;
default `Both`):
- `AA_FLAG_BAND` (screen-space anisotropic band, `Anisotropic`/`Both`): the AA
  edge width is a true 1px box filter at any view angle. `Both` adds
  `AA_FLAG_SUPERSAMPLE`, striding samples along the foreshortened footprint axis
  (`aniso_band_coverage`) to erase the grazing-angle convex-corner wing, up to
  `MAX_ANISO_SAMPLES_TEXT` (64) samples.
- Without the band flag, coverage falls back to `distance_coverage` (scalar
  band), optionally 4-sample supersampled — reference modes.

`text_coverage_bias` (per-run) applies a signed coverage transfer after the AA
math: positive makes fractional edge pixels more opaque, negative thins them.
Only text paths consume it (`path.min_feature == 0`).

`RenderMode::PunchOut` inverts coverage (`1 − coverage`) to cut the glyph out of
the quad. In the prepass/shadow pipeline coverage collapses to a single
`winding_at` test — the shadow map stores a binary silhouette, so one
inside/outside test answers it (PunchOut inverts the test).

Coverage feeds PBR lighting (Slug text is lit, not unlit — panel content is
physical) and, when enabled, OIT. `OIT_MIN_DEPTH` (3e-6) floors the offset
fragment depth so a coplanar fragment with alpha < 1 is not silently dropped by
bevy's OIT resolve.

## Gotchas for editors

- **The shader is shared with panel-line vector marks.** Text is only the
  `min_feature == 0`, `fade_exponent == 0`, single-winding case. The two-lane
  `CoverageTerms` (exempt / faded), hairline dilation, fade factor, polygon
  half-plane path, and `analytic_line_coverage` are all for panel lines. They
  are dormant for text, but a change to the shared winding/distance/band code
  must not break the line paths. The CPU `coverage_probe.rs` models these.
- **Atlas growth swaps buffers, never `set_data`s** (see `commit_glyph_atlas`).
- **Atlas is append-only, never evicted** — memory grows with the distinct
  glyph set; runs are removed from `batch_store` but glyph records persist.
- **Cache identity is the resolved face** (`font_face.blob_id`), not the
  requested style font. A missing glyph id in that face fails the run rather
  than falling back.
- **Fragment record index rounds, not floors.** `instance_index` /
  `path_index` recover the interpolated varying with `u32(x + 0.5)`; a `floor`
  reads the previous record on long sliver quads and renders another path's
  coverage.
- **Quad padding is load-bearing.** `glyph_quad_extents` pads by 16 design
  units so the analytic AA ramp is not clipped at the quad boundary; shrinking
  it clips the ramp.

## Supported scope

- Monochrome outline glyphs: TrueType quadratic and CFF/CFF2 cubic (via
  conversion). Verified across JetBrains Mono, Noto Sans, EB Garamond, Crimson
  Text, Liberation Sans, and Noto Sans CJK SC.
- **Not** supported: color/emoji glyph formats, parley font fallback (a run must
  shape entirely in registered faces), true outlines/glow/blur. Unsupported
  glyphs surface as an `OutlineError`, never dropped silently or mixed with
  another renderer inside one run.
- Shadows are a second pass over the same run data (`GlyphShadowMode::Cast`
  evaluates the silhouette in the prepass; ghost text uses `Cast` with fill
  alpha 0).

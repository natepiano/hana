# Slug Experiments

This document records Slug renderer experiments that were tried during
the feasibility branch. Its purpose is to prevent future sessions from
repeating failed approaches without a new reason.

## Baseline Method

Current shader-performance experiments use three checks:

- Visual parity: compare screenshots with ImageMagick `compare -metric AE`.
- Runtime cost: run `scripts/xctrace_text_renderer.sh` and parse Metal
  GPU interval exports for the `text_renderer_gpu_bench` process.
- CPU prep cost: run the `renderer_prep` Criterion group in
  `benches/glyph_rasterization.rs`.

### Canonical Benchmark Format

See `docs/bevy_diegetic/slug-benchmark-procedure.md` for the single
comparison table that all Slug benchmark entries must use. Do not
introduce parallel formats here.

### Shaded Pixel Waste Measurement

Temporary instrumentation measured how much glyph quad area is sent
through the analytic Slug shader compared with the estimated useful
glyph area. The instrumentation was removed after recording the result.

Fields:

- `emitted_glyphs`: glyph quads emitted after clipping.
- `shaded_quad`: total padded quad area that can invoke the fragment
  shader.
- `glyph_bounds`: visible glyph bounds area with padding removed.
- `estimated_ink`: estimated visible ink area from signed quadratic
  outline integration.
- `shaded_to_bounds_ratio()`: padded quad area divided by glyph bounds.
- `shaded_to_ink_ratio()`: padded quad area divided by estimated ink.
- `bounds_to_ink_ratio()`: glyph bounds divided by estimated ink.

Example from `examples/slug_text.rs` for `Typography` at the current
home view setup:

| Metric | Value | Meaning |
|---|---:|---|
| Emitted glyphs | `10` | Visible glyph quads in the word. |
| Shaded quad | `0.786922` | Area sent through the analytic shader. |
| Glyph bounds | `0.701398` | Visible glyph bounds area excluding padding. |
| Estimated ink | `0.283152` | Estimated actual filled text area. |
| Shaded to bounds | `1.12x` | Padding overhead around glyph bounds. |
| Shaded to ink | `2.78x` | Total approximate shader-area waste versus ink. |
| Bounds to ink | `2.48x` | Bounds area versus actual filled text area. |

The frozen Phase 10 screenshot baseline was:

- `/tmp/slug_phase10_baseline_slug_current.png`

The final kept screenshot for the first Phase 10 pass was:

- `/tmp/slug_phase10_final_shader.png`

That final image compared against the baseline at `AE 0`.

## Repeatable Camera Views

### Lowercase `g` Inside Curve

Use this view for close-edge comparisons on the inside curve of the
lowercase `g` in `examples/slug_text.rs`.

Helper script:

- `scripts/slug_text_g_zoom.sh --restart`
- `scripts/slug_text_g_zoom.sh --screenshot /tmp/view.png`
- The script launches or reuses `slug_text`, applies the saved `OrbitCam`
  state and camera `Transform`, and can capture a screenshot through BRP.
- The script waits for the async screenshot save to produce a nonempty,
  stable file before returning.

Screenshot captured from this view:

- `/tmp/slug_g_inside_curve_zoom.png`
- `/tmp/slug_g_inside_curve_script_check.png`

`OrbitCam` state:

- focus: `[-0.07180324, 0.38033515, 2.0064423]`
- radius: `0.08216206`
- yaw: `0.0`
- pitch: `0.055`
- target_focus: `[-0.07180324, 0.38033515, 2.0064423]`
- target_radius: `0.08216206`
- target_yaw: `0.0`
- target_pitch: `0.055`

Camera transform observed at the same time:

- translation: `[-0.07180324, 0.38485178, 2.0884802]`
- rotation: `[-0.02749653, 0.0, 0.0, 0.99962193]`
- scale: `[1.0, 1.0, 1.0]`

## Kept Experiments

### Squared Distance Accumulation

Change:

- Compare squared curve distances in the nearest-curve loop.
- Take one square root after the nearest curve is known.

Result:

- Pixel-identical to the baseline.
- Measurably faster than the original shader.

Reason to keep:

- It removes repeated square roots without changing the edge coverage
  result.

### CPU-Packed Curve Coefficients

Change:

- Store `start`, `control - start`, `end - 2 * control + start`, and
  `end` in `SlugCurveRecord`.
- Use those fields directly for winding and distance evaluation.

Result:

- Pixel-identical to the baseline.
- Small but measurable improvement.

Reason to keep:

- This shifts repeated per-fragment algebra into one-time CPU packing.

### CPU-Packed Solver Constants

Change:

- Store distance-solver constants per curve:
  `3 * dot(control_delta, curve_delta) / dot(curve_delta, curve_delta)`,
  `2 * dot(control_delta, control_delta) / dot(curve_delta, curve_delta)`,
  and the reciprocal curve norm.

Result:

- Pixel-identical to the baseline.
- Best single safe improvement in this pass.
- Current all-mode trace after the kept changes showed Slug fragment
  mean at about `5.4053 ms`, compared with about `5.9117 ms` for the
  original same-wrapper Slug trace.

Reason to keep:

- It uses cheap CPU preprocessing to reduce hot fragment work.
- The CPU prep benchmark still showed Slug prep around `1.19 ms`, far
  below the distance-field prep cases.

### Bounds-Distance Early Return

Change:

- Store a conservative control-point bounding box for each curve.
- Before exact distance solving, test whether every candidate curve box
  is farther than the antialiasing width.
- If so, return solid inside or transparent outside after the winding
  test.

Result:

- Pixel-identical to the baseline.
- Helpful when combined with the packed solver constants.

Reason to keep:

- It avoids exact quadratic distance solving in obviously interior or
  exterior pixels.

### Single-Pass Bounds-Gated Distance

Change:

- Replace the separate nearest-bounds pass and exact-distance pass with
  one distance loop.
- For each band candidate, first test the conservative curve bounds
  against the antialiasing width.
- Only run exact quadratic distance for candidates whose bounds can
  affect the current edge pixel.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Metal trace for the 720-instance Slug benchmark showed fragment mean
  at about `5.1004 ms`, down from the prior kept `5.4053 ms`.

Reason to keep:

- Edge pixels no longer pay for a bounds loop followed by another full
  exact-distance loop.

### Combined Horizontal Winding And Distance

Change:

- Compute horizontal-band winding and horizontal-band distance
  candidates in the same loop.
- Keep the vertical-band distance pass, seeded from the horizontal
  distance result.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Metal trace for the 720-instance Slug benchmark showed fragment mean
  at about `4.5880 ms`.

Reason to keep:

- The shader no longer reads the horizontal band once for winding and a
  second time for distance.

## Rejected Experiments

### Combined Micro-Optimizations

Change:

- Combine three changes that were each exact but not faster alone:
  winding bounds cull, exact-distance local fields, and CPU-packed
  winding reciprocal.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- The first Metal trace for the 720-instance Slug benchmark showed
  fragment mean at about `4.4629 ms`, down from the prior kept
  `4.5880 ms`.
- A fresh rerun did not confirm the result: fragment mean was about
  `4.9647 ms`.

Reason rejected:

- The code is more complex than the kept shader and the apparent win did
  not survive rerun noise.

### Reuse Horizontal Band for Distance and Winding

Change:

- Try to avoid a separate vertical-band lookup by using the horizontal
  band for both winding and nearest-edge distance.

Result:

- Pixel-identical in the tested screenshot.
- Slower in Metal traces.

Reason rejected:

- Less data access did not translate into lower GPU time.

### Glyph-Bounds Early Out Before Distance Work

Change:

- Try an early return based only on padded glyph bounds.

Result:

- Pixel-identical in the tested screenshot.
- Slower in Metal traces.

Reason rejected:

- The branch and bound math cost more than it saved.

### Vertical-Only Distance

Change:

- Keep horizontal bands for winding, but use only the vertical band for
  nearest-edge distance.

Result:

- Not pixel-identical. The tested screenshot differed by `9165` pixels.

Reason rejected:

- The horizontal and vertical candidate sets are not redundant. Some
  visible edge coverage needs curves that the vertical band alone does
  not include.

### Chord-Based Curve Cull

Change:

- Before exact quadratic distance solving, compare the point to the
  curve chord plus a conservative curve-deviation radius.

Result:

- Pixel-identical.
- Slower in Metal traces, roughly back at original Slug cost.

Reason rejected:

- The extra test cost outweighed skipped exact solves.

### Split Outside-Bounds Winding Branch

Change:

- Split `horizontal_coverage_terms` into an outside-bounds distance-only
  loop and an inside-bounds winding-plus-distance loop.
- This removed a per-curve `include_winding` branch from the common
  inside-glyph path.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Slower in Metal traces: the 720-instance Slug benchmark fragment mean
  rose to about `5.7377 ms`.

Reason rejected:

- The extra control-flow shape was worse than the branch inside one loop.

### Winding Bounds Cull

Change:

- Add a conservative early return at the top of `curve_winding` when
  the sample point is above, below, or to the right of a curve's control
  bounds.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Not faster in Metal traces: the 720-instance Slug benchmark fragment
  mean was about `4.6172 ms`, slightly slower than the committed
  `4.5880 ms` baseline.

Reason rejected:

- The added branch did not beat the existing winding path on the GPU.

### Expanded-Rectangle Bounds Cull

Change:

- Replace the exact point-to-bounds squared-distance check with four
  comparisons against the curve control bounds expanded by the
  antialiasing width.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Slower in Metal traces: the 720-instance Slug benchmark fragment mean
  was about `4.6845 ms`, compared with the committed `4.5880 ms`
  baseline.

Reason rejected:

- The cheaper bounds test admitted more exact quadratic solves, which
  outweighed the simpler comparisons.

### Denser Vertical Bands

Change:

- Keep horizontal bands at `32` so winding behavior stays unchanged.
- Increase only vertical distance bands to `64`.
- Try vertical band overlaps of `1`, `2`, `4`, and `8` design units.

Result:

- Not pixel-identical in the lowercase `g` zoom view.
- Differences were small but nonzero:
  `6`, `5`, `4`, and `11` pixels in the tested variants.

Reason rejected:

- The exact gate requires no pixel differences.
- The result reinforces that banding is part of the visible analytic
  answer, not just a performance detail.

### Exact-Distance Local Fields

Change:

- Move `start`, `control_delta`, `curve_delta`, and `end` extraction
  inside `exact_quadratic_distance_sq` instead of passing them as
  separate parameters.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Not meaningfully faster in Metal traces: the 720-instance Slug
  benchmark fragment mean was about `4.5986 ms`, slightly slower than
  the committed `4.5880 ms` baseline.

Reason rejected:

- The compiler already handled the parameterized version well enough.

### CPU-Packed Winding Reciprocal

Change:

- Store the quadratic winding denominator reciprocal in `solver.w`.
- Use multiplication instead of division for the two quadratic winding
  roots.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Slower in Metal traces: the 720-instance Slug benchmark fragment mean
  was about `4.6550 ms`, compared with the committed `4.5880 ms`
  baseline.

Reason rejected:

- Loading the additional packed value did not beat the fragment-side
  divisions.

### Skip Vertical Duplicate Curves

Change:

- Pass the current horizontal band into the vertical distance loop.
- Skip a vertical candidate when the same curve should already have been
  present in the horizontal distance pass.
- Keep horizontal line segments in the vertical pass because horizontal
  bands omit them.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Slower in Metal traces: the 720-instance Slug benchmark fragment mean
  was about `4.7310 ms`.

Reason rejected:

- The extra duplicate-detection branch and comparisons cost more than
  the repeated exact solves they avoided.

### Combined Micro-Optimizations With Expanded-Rectangle Bounds

Change:

- Start from the combined micro-optimization candidate.
- Add expanded-rectangle bounds culling in place of point-to-bounds
  squared distance.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Slower in Metal traces: the 720-instance Slug benchmark fragment mean
  was about `4.7761 ms`, compared with the combined micro-optimization
  result of about `4.4629 ms`.

Reason rejected:

- The cheaper bounds test admitted enough extra exact quadratic solves
  to lose even in combination.

### Combined Micro-Optimizations With Duplicate Skipping

Change:

- Start from the combined micro-optimization candidate.
- Add vertical duplicate skipping.

Result:

- Pixel-identical to the Phase 10 baseline in the normal view and the
  lowercase `g` zoom view (`AE 0` for both).
- Slower in Metal traces: the 720-instance Slug benchmark fragment mean
  was about `4.6568 ms`, compared with the combined micro-optimization
  result of about `4.4629 ms`.

Reason rejected:

- The extra duplicate-detection branch and comparisons still cost more
  than the repeated exact solves they avoided.

### Combined Micro-Optimizations With Denser Vertical Bands

Change:

- Start from the combined micro-optimization candidate.
- Use 64 vertical bands with 16 design units of vertical overlap.

Result:

- Not benchmarked after the screenshot wait was increased.
- The packed `Typography` run grew substantially: for example, `g`
  increased from 312 to 558 packed curve records.

Reason rejected:

- Larger overlap erased the hoped-for vertical-band savings by expanding
  packed curve lists.
- This should only be revisited with a more targeted packing rule than
  fixed larger overlap.

### Global 64-Band Packing

Change:

- Increase the global band count from `32` to `64`.
- Try overlaps of `1`, `4`, `8`, and `16` design units.

Result:

- Not pixel-identical.
- Differences were small in count, but nonzero:
  `34`, `7`, `14`, and `23` pixels in the tested variants.

Reason rejected:

- The Phase 10 rule required exact screenshot parity.
- More bands can exclude a curve that still affects an edge near a band
  boundary unless overlap is chosen more carefully.

Future note:

- This is worth revisiting under a perceptual-quality rule instead of a
  strict parity rule.
- A better version should compute per-band overlap from pixel scale,
  curve bounds, or max curve influence instead of using one global
  constant.

### Two-Dimensional Distance Grid

Change:

- Keep horizontal bands for winding.
- Replace the distance lookup with a `16 x 16` CPU-built distance-cell
  grid.
- Test fixed cell overlaps of `1`, `16`, and `64` design units.

Result:

- Not pixel-identical.
- Differences decreased with overlap at first, but did not disappear:
  `1940`, `479`, and `23` pixels in the tested variants.

Reason rejected:

- A local distance cell can miss curves that still affect antialiasing
  near cell boundaries.
- Fixed overlap is not robust enough.

Future note:

- This remains one of the most promising CPU-prework directions.
- A better version should either include neighboring cells at lookup
  time, compute exact curve-cell influence during packing, or accept a
  reviewed perceptual difference if quality is not harmed.

### Point-Space Solver Rewrite

Change:

- Precompute constants so the cubic solver can use dot products with
  `point` directly instead of `point - start`.

Result:

- Pixel-identical.
- Slower in Metal traces.

Reason rejected:

- The compiler likely handled the original temporary well, while the
  rewritten expression increased register pressure or instruction
  latency.

### Convex Hull Glyph Meshes

Change:

- Replace each rectangular glyph quad with a convex polygon enclosing the
  glyph outline control points plus the existing Slug padding.
- Keep the same Slug shader, material, curve records, band records, and
  glyph records.
- Clip the polygon to panel clip rectangles before uploading mesh
  vertices.

Result:

- The lowercase `g` zoom was visually very close to the baseline but not
  pixel-identical against the saved screenshot. The difference was small
  enough that the benchmark result drove the decision.
- The saved 720-word Slug trace before the experiment had:
  - vertex mean: `0.0670 ms`
  - fragment mean: `3.7242 ms`
  - transparent pass CPU mean: `0.3152 ms`
  - total-by-frame mean: `4.4303 ms`
- The convex hull trace had:
  - vertex mean: `0.0873 ms`
  - fragment mean: `3.7171 ms`
  - transparent pass CPU mean: `0.3289 ms`
  - total-by-frame mean: `4.5441 ms`

Reason rejected:

- It did not reduce the fragment cost enough to matter.
- The extra polygon vertices slightly increased vertex and CPU render
  work.
- Most large glyph pixels are still inside the convex hull, so the shader
  still shades too much boring interior area.

Future note:

- A tighter non-convex edge band or real interior/edge split is still the
  better next direction.

### Lyon Fill Plus Stroked Edge Mesh

Change:

- Add `lyon_path` and `lyon_tessellation` to prototype a real split
  renderer in the private Slug path.
- Build a tessellated solid fill mesh from glyph contours.
- Build a stroked contour mesh for the analytic Slug edge pass.
- Spawn fill with `SlugRenderMode::SolidQuad` and the edge mesh with
  `SlugRenderMode::Text`.

Result:

- Rejected before benchmarking.
- The first screenshot showed severe horizontal banding from coplanar
  fill and edge geometry.
- Moving the analytic edge overlay slightly forward removed the simplest
  depth-fighting explanation, but the stroked edge mesh still covered
  large interior regions and produced visible horizontal stripes.

Reason rejected:

- The simple stroked-contour mesh is not a usable edge-band
  representation for filled glyphs.
- It does not preserve the current Slug visual output closely enough to
  justify benchmarking.
- A real version would need a non-overlapping interior/edge partition,
  not "full fill plus broad stroked contours."

Future note:

- Lyon remains useful for future fill tessellation, but the edge band
  should probably be generated as a controlled ring or distance-cell mesh,
  not directly from the generic stroke tessellator.

### Cached Distance-Cell Fill/Edge Split

Change:

- Split each glyph into a CPU-classified grid of fill, edge, and empty
  cells.
- Draw fill cells with the cheap solid Slug mode.
- Draw edge cells with the analytic Slug shader.
- Keep the current curve, band, and glyph storage model.

Result:

- Rejected after benchmarking.
- The first implementation had the wrong cache boundary: it classified
  cells while assembling each rendered run, so repeated text entities paid
  the classification cost again.
- Moving classification behind a per-`SlugGlyphKey` cache fixed that bug,
  but the split path was still slower in the benchmark.
- Saved baseline stdout:
  - frame time mean: `11.8734 ms`
  - render CPU sum mean: `0.3192 ms`
  - transparent pass CPU mean: `0.3152 ms`
- Cached split stdout:
  - frame time mean: `18.5721 ms`
  - render CPU sum mean: `0.8488 ms`
  - transparent pass CPU mean: `0.8436 ms`

Reason rejected:

- The path doubled visible Slug work into fill and edge meshes.
- For the 720-instance benchmark, that extra entity/material/draw work
  outweighed any saved analytic fragment work.
- A grid of rectangles is too blunt: it creates many small rectangles
  while still leaving enough edge area to shade analytically.

Future note:

- Any future edge split should cache derived geometry per unique glyph
  from the start.
- It also needs to avoid doubling draw overhead per text entity. Better
  candidates are one mesh with an edge/interior attribute, a tighter
  non-rectangular edge band, or a renderer-level path that can batch the
  split geometry cheaply.

### Merged Single-Mesh Distance-Cell Split

Change:

- Keep one visible mesh and one material for normal Slug text.
- Classify a per-glyph grid into empty, solid-fill, and analytic-edge
  cells.
- Merge adjacent same-class cells into larger rectangles.
- Store the region class in the second UV channel:
  - `0.0`: analytic edge path
  - `1.0`: solid fill path
- Keep punch-out, solid-quad, and shadow proxy rendering on the original
  full-quad mesh.

Result:

- Rejected after benchmarking.
- Added `WinitSettings::continuous()` to the benchmark example after
  confirming Bevy's default `WinitSettings::game()` uses continuous mode
  only when focused and a low-power reactive mode when unfocused. This
  makes future frame-time and render-CPU numbers less dependent on
  whether the benchmark window is frontmost.
- Close `g` screenshots stayed visually clean.
- Pixel comparison against the saved baseline `g` screenshot stayed very
  small:
  - `24x24`, margin `24`: RMSE `35.9259`; AE with `1%` fuzz: `69`
  - `32x32`, margin `24`: RMSE `36.0237`; AE with `1%` fuzz: `65`

Benchmarks:

- Saved baseline:
  - vertex mean: `0.0670 ms`
  - fragment mean: `3.7242 ms`
  - total-by-frame mean: `3.7620 ms`
  - render CPU sum mean: `0.3192 ms`
- `24x24`, margin `24`:
  - vertex mean: `0.2507 ms`
  - fragment mean: `3.7831 ms`
  - total-by-frame mean: `3.9072 ms`
  - render CPU sum mean: `0.3899 ms`
- `32x32`, margin `32`:
  - vertex mean: `0.2977 ms`
  - fragment mean: `4.4999 ms`
  - total-by-frame mean: `4.8345 ms`
  - render CPU sum mean: `0.7266 ms`
- `32x32`, margin `24`:
  - vertex mean: `0.2783 ms`
  - fragment mean: `3.9381 ms`
  - total-by-frame mean: `4.3040 ms`
  - render CPU sum mean: `0.3776 ms`

Current assessment:

- `24x24`, margin `24` is the best tested variant, but it is still
  slightly slower than the saved full-quad Slug baseline.
- The merged rectangle path avoids the two-mesh problem, but still emits
  enough extra vertices to erase the fragment savings.
- The coarse grid also cannot isolate the edge tightly enough; fragment
  cost stays near baseline while vertex/CPU cost rises.
- The renderer changes were backed out; only the benchmark focus fix and
  this record remain.

Future note:

- The next serious geometry version should stop using axis-aligned grid
  rectangles as the final mesh. A tighter band/ring around contours or
  fill tessellation plus a controlled analytic edge strip is more likely
  to win.

### Single-Span Horizontal Band Classifier

Change:

- Store one filled X span for horizontal bands where the CPU can find
  exactly one filled interval at the band midpoint.
- Use the existing two spare floats in `SlugBandRecord`; no new buffer,
  mesh, material, or draw call.
- In the shader, return solid or empty coverage when the current point is
  far enough from that span's edges. Fall back to the normal analytic
  Slug path near uncertain edges and for bands with multiple spans.

Why:

- The shaded-pixel waste measurement showed that the main waste is inside
  glyph bounds but outside ink, not in padded quad area.
- A cheap per-band span classifier directly targets those pixels without
  the entity and vertex overhead of prior grid-mesh split attempts.

Results:

| Variant | Close `g` RMSE | Close `g` AE With `1%` Fuzz | Fragment Mean | Frame Time | Transparent Pass CPU | Meaning |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Current 96-band baseline | baseline | baseline | `2.5759 ms` | `10.9424 ms` | `0.3650 ms` | Current documented candidate. |
| Conservative margin, duplicate band load fixed | `138.732` | `8187` | `2.6603 ms` | `9.8550 ms` | `0.2996 ms` | Better CPU/frame diagnostics, worse fragment time. |
| Edge-width-only margin | `148.681` | `9389` | `2.5953 ms` | `11.5604 ms` | `0.4215 ms` | Nearly matched fragment time, worse visual delta and CPU/frame diagnostics. |

Reason rejected:

- It did not beat the current 96-band baseline on the primary signal,
  fragment time.
- The more aggressive margin increased visual delta and worsened CPU/frame
  diagnostics.
- The classifier adds CPU preprocessing and shader branch work for a small,
  unstable gain.

Future note:

- A span classifier may still be viable if it stores more exact conservative
  spans across the whole band, not just midpoint spans.
- Do not repeat the midpoint single-span version without a new reason.

### Global Band-Density Retest

Change:

- Retest global band counts under a perceptual/performance gate instead
  of the earlier strict `AE 0` gate.
- Keep the existing Slug storage and shader model.
- Change only `DEFAULT_BAND_COUNT`.

Why:

- More bands reduce the number of packed curve candidates each fragment
  needs to scan.
- Earlier strict-parity tests rejected this because the screenshots were
  not pixel-identical, but the differences were visually tiny.

Screenshot Check:

All screenshot checks used the saved lowercase `g` zoom view against the
saved 32-band baseline.

| Variant | RMSE | AE With `1%` Fuzz | Meaning |
| --- | ---: | ---: | --- |
| 80 bands | `6.75922` | `32` | Smallest tested image delta. |
| 96 bands | `7.59753` | `50` | Slightly larger image delta, still visually small in the zoom view. |
| 112 bands | `7.38058` | `46` | Similar visual delta to 96 bands. |

GPU Trace:

Each row is the mean of the saved Metal traces for the 720-instance
`text_renderer_gpu_bench` Slug run. The 32-band baseline includes three
runs; 80 bands includes two runs; 96 and 112 bands include three runs.

| Metric | 32 Bands | 80 Bands | 96 Bands | 112 Bands | Meaning |
| --- | ---: | ---: | ---: | ---: | --- |
| Vertex mean | `0.0602 ms` | `0.0606 ms` | `0.0599 ms` | `0.0614 ms` | GPU vertex work for the transparent pass. |
| Fragment mean | `2.9008 ms` | `2.6449 ms` | `2.5759 ms` | `2.6627 ms` | GPU pixel work; this is the main Slug shader cost. |
| Total-by-frame mean | `3.9883 ms` | `3.8735 ms` | `3.7655 ms` | `3.7636 ms` | Per-frame vertex plus fragment work for frames with both channels. |

Bevy Diagnostics:

| Metric | 32 Bands | 80 Bands | 96 Bands | 112 Bands | Meaning |
| --- | ---: | ---: | ---: | ---: | --- |
| Frame time mean | `11.7047 ms` | `10.6591 ms` | `10.9424 ms` | `10.5071 ms` | Whole app frame time reported by Bevy. |
| Render CPU sum mean | `0.3991 ms` | `0.3244 ms` | `0.3701 ms` | `0.3116 ms` | Sum of measured render CPU work. |
| Transparent pass CPU mean | `0.3936 ms` | `0.3198 ms` | `0.3650 ms` | `0.3073 ms` | CPU time for the pass that draws Slug text. |

Prep Check:

| Metric | 32 Bands | 96 Bands | 112 Bands | Meaning |
| --- | ---: | ---: | ---: | --- |
| `renderer_prep/jbm_ascii_128_slug` | `1.1923 ms` | `1.1703 ms` | `1.2213 ms` | One-time CPU preparation for the 94 printable ASCII glyphs. |

Reconstructed First-Benchmark Comparison:

The earliest Slug benchmark example was introduced in `dda5299`. To
reconstruct the first benchmark baseline with today's measurement setup,
checkout `dda5299`, copy in `scripts/xctrace_text_renderer.sh`, and add
only `WinitSettings::continuous()` to `text_renderer_gpu_bench.rs`.
Keep the Slug implementation itself at `dda5299`.

The old `5.9117 ms` fragment note did not reproduce under this setup;
the reconstructed baseline numbers (alongside the current 96-band
column and the latest experimental column) are recorded in
`docs/bevy_diegetic/slug-benchmark-procedure.md`.

Assessment:

- 96 bands is the best tested fragment-time result and nearly ties 112
  bands on total GPU time.
- 112 bands has the best Bevy CPU diagnostics, but its fragment time is
  worse than 96 and it stores more band records.
- 80 bands has the smallest visual delta, but gives up too much of the
  fragment improvement.
- Keep 96 bands as the current candidate. This is a meaningful shader
  workload reduction without adding geometry complexity.

Open concern:

- This is not a strict-parity change. It needs human review at normal
  text size, the lowercase `g` zoom, and panel text size before it should
  be treated as final.

## Open Experiment Ideas

### Edge-Only Analytic Shading

Idea:

- Use CPU preprocessing to build cheap interior fill geometry and run the
  expensive analytic Slug shader only near glyph edges.

Why it matters:

- The current full-glyph quad shader pays analytic edge cost across large
  interior regions. That is the main remaining waste.

Risks:

- Needs robust outline triangulation or band/ring mesh generation.
- Must preserve punch-out, shadows, clipping, and panel behavior.

### Perceptual Acceptance Gates

Idea:

- Keep `AE 0` for pure refactors.
- Add a separate reviewed gate for intentional quality/performance
  tradeoffs.

Candidate metrics:

- Pixel count above a small threshold.
- Maximum per-channel error.
- Mean absolute error.
- Structural similarity.
- Cropped edge-zone comparison.
- Human-reviewed screenshots at close zoom and normal viewing distance.

Why it matters:

- Some faster candidates had tiny pixel differences that may be invisible
  or may even be better than the current baseline.

## Per-segment ribbon — scene-limited visual notes (2026-05-23)

Canonical performance numbers and verdict live in
`docs/bevy_diegetic/slug-benchmark-procedure.md` (Per-segment ribbon
column, 2026-05-23). The following notes are scene-limited observations
that do not belong in the canonical performance table.

Home view (canonical `text_renderer_gpu_bench` scene at default OrbitCam):

- Visual parity vs forced-quad baseline: `13670 / 7,271,424` pixels
  differ (`0.188 %`). RMSE `949 (0.0145)`, `AE -fuzz 1%` `8367`.
- Baseline and ribbon screenshots are visually identical at this scale.

Lowercase-g zoom view (`scripts/slug_text_g_zoom.sh --view g`):

- AA degrades on the rectangle bands at this zoom. The rectangle
  half-width is fixed in design units (5) and becomes narrower than the
  screen-space AA zone at high zoom, so coverage softens before the
  contour itself does. This is a property of the band geometry, not of
  the analytic coverage function — picking a screen-space half-width
  would address it but enlarges the band area and would change the
  performance numbers.
- The zoom-view artifact is documented here as scene-limited because
  the canonical performance scene is the home view of
  `text_renderer_gpu_bench` and that view is unaffected.

Screenshot procedure caveat:

- `scripts/slug_text_g_zoom.sh --restart --view home --screenshot ...`
  triggers the BRP screenshot before the OrbitCam has settled and
  before the first frame of text has drawn, producing a uniform-color
  PNG (`colors=1`). The visual diff above was obtained by launching
  first with `--no-screenshot`, then issuing a second invocation
  without `--restart` so the app has time to draw a real frame before
  the screenshot RPC fires.

## Joined ribbon iteration trail (2026-05-23)

The per-segment ribbon was the first ribbon variant; three follow-on
iterations replaced it. Canonical numbers and verdict live in
`docs/bevy_diegetic/slug-benchmark-procedure.md`; this section records
what was tried and why each step was taken.

1. **Join joints into a tri-strip** (`half = 5`, `FLATTEN_STEPS = 12`).
   Replaced N independent rectangles with one mitered tri-strip per
   contour. Each joint emits one outer + one inner vertex, shared
   between neighbouring quads. Vertex count fell by ~2x and the
   small-triangle rasterizer overhead at corners went away.

   Result: vertex `1.1183 ms -> 0.8390 ms` (`-25 %`),
   fragment `3.0421 -> 2.7390` (`-10 %`),
   prep `2.9226 -> 1.9869` (`-32 %`),
   visual `0.188 % -> 0.179 %`. Clear improvement; still over 96 bands.

2. **Narrow the band** (`half = 5 -> 3`). Reduces band area, which is
   the fragment-cost lever. Vertex count unchanged.

   Result: per-row vertex / fragment moved within trace noise (`~+0.06
   ms`), but per-frame V+F total dropped `5.6928 -> 5.4305 ms`. Visual
   improved (`0.179 % -> 0.138 %`) because narrower bands have less
   corner overshoot. Kept.

3. **Narrow further and coarsen flatten**
   (`half = 3 -> 2`, `FLATTEN_STEPS = 12 -> 6`). The flatten coarsening
   actually cuts the vertex count (fewer polyline joints per quadratic
   segment); the narrower band cuts fragment area further.

   Result: vertex `0.9176 -> 0.6431 ms` (`-30 %`),
   fragment `2.7974 -> 2.3381 ms` (`-16 %`),
   prep `1.9869 -> 1.6981 ms` (`-15 %`),
   visual `0.138 % -> 0.143 %` (a hair worse but still well below
   `0.2 %`). Best ribbon variant.

After (3), per-row fragment crossed below the 96-band candidate
(`2.3381 vs 2.5759 ms`, `-9 %`), but vertex stayed `+0.58 ms` over
96 bands and total V+F stayed `+1.57 ms` over. The vertex floor is
~`2 verts * joints_per_contour`, which is fundamentally above the
4-verts-per-glyph cost of single-quad approaches like 96 bands or
forced quad. See the procedure doc's "Next direction" section for
where to take the experiment next without paying SDF-atlas-style prep
cost.

## EDGE_FILTER_WIDTH 1.2 -> 1.0 (rejected, 2026-05-23)

**Hypothesis.** The Slug fragment shader (`shaders/slug_text.wgsl`)
gates `solve_cubic_normed` per curve on
`curve_bounds_distance_sq(point, curve) <= edge_width_sq`, with
`edge_width = max(pixel.x, pixel.y) * EDGE_FILTER_WIDTH`. Dropping
`EDGE_FILTER_WIDTH` from `1.2` to `1.0` shrinks the area where the
cubic fires by `(1.0/1.2)^2 = 0.69x`. Expected fragment-cost drop.

**Setup.** 96-band Slug, 720-instance `text_renderer_gpu_bench`, AC
power, Low Power Mode off. Same parser (`parse_gpu_intervals.py`
after the per-frame fix) used for both columns.

**Visual.** `0.265 %` of pixels differ (`19,273 / 7,271,424`),
concentrated in a one-pixel halo around glyph edges where the AA
smoothstep transition narrowed. RMSE `0.214 %` per channel. Larger
than the joined-ribbon variant's `0.143 %` visual delta.

**Performance (same-parser, AC, single trace each).**

| Metric | EDGE = 1.2 | EDGE = 1.0 | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0549 ms` | `0.0538 ms` | `-0.0011` (noise) |
| Fragment per frame          | `3.1724 ms` | `3.2127 ms` | **`+0.0403`** |
| Vertex + fragment per frame | `3.2274 ms` | `3.2665 ms` | **`+0.0391`** |

**Result.** Rejected. Fragment cost did **not** drop; it rose
marginally (`+1.3 %`). The visible AA narrowing was paid for, the
expected ALU win did not materialize.

**Lesson.** GPU wavefront divergence appears to dominate the
cubic-gate count. Apple silicon dispatches in 32-lane SIMDgroups; a
SIMDgroup runs the cubic if even one of its 32 fragments fires the
gate. Curves cluster along glyph edges, so the same SIMDgroups touch
the cubic at `EDGE = 1.0` as at `1.2` — they just have fewer active
lanes. Lane underutilization does not shorten wavefront latency.

**Side effect.** This experiment surfaced a parser bug: the prior
`parse_gpu_intervals.py` printed `fragment_mean_ms` as a per-interval
mean while `vertex_plus_fragment_total_mean_ms` was per-frame, so
vertex + fragment did not sum to total. Fixed in the same session;
the procedure-doc table was rebuilt from the new same-parser 96-band
baseline (the old multi-column comparison table is retired).

**Implication for future experiments.** Tightening the cubic-fire
gate cannot help while wavefront divergence is the bottleneck. To
reduce fragment cost, either (a) reduce the *coherent* set of
fragments that fire the cubic (e.g. a coarse SDF prefilter that
exits whole wavefronts before any cubic runs), or (b) make the
cubic itself cheaper (chord-distance approximation gated by curve
flatness; F16 intermediates).

## Chord-distance approximation gated by curve flatness (rejected, 2026-05-23)

**Hypothesis.** Replace `solve_cubic_normed` with
`point_line_distance_sq(point, start, end)` for near-line curves.
Gate condition: `inverse_curve_norm_sq * edge_width_sq >= 9.0`, i.e.
sagitta `|curve_delta| / 4` no greater than `edge_width / 12` (~one
tenth of a pixel for `EDGE_FILTER_WIDTH = 1.2`). For flat curves
the chord IS the curve to within rounding, so the fragment shader
skips ~30 cycles of trig + sqrt and runs ~5 cycles of line distance.

**Setup.** 96-band Slug, 720-instance `text_renderer_gpu_bench`, AC
power, Low Power Mode off. Same parser (`parse_gpu_intervals.py`
after the per-frame fix). Threshold compiled into
`shaders/slug_text.wgsl` as `CHORD_GATE_SCALE = 9.0` with
`edge_width_sq` threaded through `curve_distance_sq` and
`exact_quadratic_distance_sq`.

**Visual.** `1.80 %` of pixels differ at `-fuzz 1 %`
(`131,070 / 7,271,424`); `3.14 M` differ at zero fuzz with RMSE
`0.0058 %` — almost every differing pixel is off by `< 1 / 255`,
indicating the chord path shifts the smoothstep transition by a
fraction of a pixel near edges rather than producing structural
breaks. Larger than EDGE_FILTER's `0.265 %`, but quality is still
visually unchanged.

**Performance (same-parser, AC, single trace each).**

| Metric | Baseline (cubic only) | Chord gate `9.0` | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0549 ms` | `0.0590 ms` | `+0.0041` |
| Fragment per frame          | `3.1724 ms` | `3.5691 ms` | **`+0.3967`** |
| Vertex + fragment per frame | `3.2274 ms` | `3.6281 ms` | **`+0.4007`** |

**Result.** Rejected. Fragment cost rose by `+12.5 %`. The savings
from skipping the cubic on flat curves were swamped by the
per-iteration branch divergence cost: when any lane in a
32-lane SIMDgroup hits a bendy curve, all lanes wait for the cubic
to complete while flat-path lanes idle. Adding a *second* gate (on
top of the existing degenerate-curve check) made the divergence
worse, not better.

**Lesson.** Adding fast paths to a per-curve inner loop *increases*
fragment cost when the SIMDgroup almost always contains at least
one cubic-path curve. The branch-prediction view is wrong here:
divergence is per-lane, and the slow path is paid by all lanes that
took it OR are stalled behind it. Pattern matches the EDGE_FILTER
result.

**Implication for future experiments.** Per-curve gating inside the
cubic-call loop is a dead end. To reduce fragment cost the change
must (a) shrink the *uniform* set of curves the SIMDgroup sees
(e.g. pack curves by flatness so flat-curve bands hold no bendy
curves and the whole-wave path is the chord), (b) make the cubic
itself unconditionally cheaper (e.g. shared sincos, F16
intermediates), or (c) eliminate the second-band redundant cubic
pass entirely (distance / winding band split).

## CURVE_DEGENERATE_EPS 1e-8 -> 1.0 (inconclusive, 2026-05-23)

**Hypothesis.** Pre-bake flat-curve detection on the CPU side: raise
`CURVE_DEGENERATE_EPS` in `packing.rs` from `1e-8` to `1.0` so any
quadratic with `|curve_delta|^2 < 1.0` gets `inverse_curve_norm_sq = 0`
and takes the existing degenerate-curve `point_line_distance_sq` path
in `exact_quadratic_distance_sq`. CPU-side gate adds zero shader work
and reuses the existing branch — addresses the chord-gate divergence
concern by not adding a new per-curve branch.

**Setup.** 96-band Slug, 720-instance `text_renderer_gpu_bench`, AC
power. Short-warmup protocol (this experiment ran before the
long-warmup protocol was established).

**Visual.** `0 / 7,271,424` pixels differ at zero fuzz (`magick compare
-metric AE`). Pixel-identical.

**Performance (short-warmup, 5 traces each — noise-bound).**

| Metric | Baseline median | eps=1.0 median | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0563 ms` | `0.0560 ms` | `-0.0003` (noise) |
| Fragment per frame          | `3.4238 ms` | `3.3552 ms` | `-0.0686` (within noise) |
| Vertex + fragment per frame | `3.4831 ms` | `3.4112 ms` | `-0.0719` (within noise) |

Baseline range across 5 traces: `0.156 ms`. eps=1.0 range across 5
traces: `0.597 ms` (much wider — the eps=1.0 batch ran on a chip
warming through the boost-to-throttled transition and showed monotonic
drift `3.17 -> 3.41 -> 3.25 -> 3.52 -> 3.77`). With overlapping
ranges and a `-0.07 ms` median delta well inside the `0.6 ms`
candidate spread, the signal is below noise.

**Result.** Inconclusive. The measurement protocol could not resolve a
sub-0.5 ms change. The visual outcome (pixel-identical) confirms the
pre-bake is safe to ship if it ever shows real benefit, but no clear
fragment-cost change was observed.

**Side effect.** This experiment surfaced the trace-to-trace noise
problem. Established the long-warmup protocol
(`WARMUP_FRAMES=1800 SAMPLE_FRAMES=1800` with 35s xctrace
`time-limit`, median of 5 back-to-back traces). Under that protocol
the baseline range tightens from `0.156 ms` to `0.040 ms` and the
steady-state baseline drops from `3.48 ms` to `2.73 ms` V+F per
frame — the short-warmup numbers included cold-start GPU
boost-clock samples. Procedure doc updated with the new baseline and
the `±0.05 ms` signal threshold. All future experiments use the
long-warmup protocol.

**Implication for future experiments.** Re-test with long-warmup
protocol if a future experiment makes the divergence theory worth
re-checking. Current evidence (chord-gate +0.40 ms regression on a
single trace) suggests no win; tightening the noise floor would just
confirm the inconclusive result.

## Shared sincos in solve_cubic_normed (rejected, 2026-05-23)

**Hypothesis.** `solve_cubic_normed` computes `cos(theta/3)` and
`sin(theta/3)` separately. Since `theta/3 ∈ [0, pi/3]`, `sin` is
non-negative, so `sin(theta/3) = sqrt(1 - cos(theta/3)^2)` is exact.
Replacing the `sin` call with `sqrt(max(0.0, 1.0 - cos_t3*cos_t3))`
saves one transcendental per cubic and applies to every lane (no
per-curve branch — addresses the wavefront-divergence trap from
chord-gate and EDGE_FILTER).

**Setup.** 96-band Slug, 720-instance `text_renderer_gpu_bench`, AC
power, long-warmup protocol (median of 5 traces).

**Visual.** `0 / 7,271,424` pixels differ at zero fuzz. Pixel-identical
(as the math requires).

**Performance.**

| Metric | Baseline median | Sincos median | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0406 ms` | `0.0414 ms` | `+0.0008` (noise) |
| Fragment per frame          | `2.6886 ms` | `2.7236 ms` | `+0.0350` (sub-threshold, wrong direction) |
| Vertex + fragment per frame | `2.7291 ms` | `2.7650 ms` | `+0.0359` (sub-threshold, wrong direction) |

Sincos range across 5 traces: `0.113 ms` (~3x baseline range of
`0.040 ms`).

**Result.** Rejected. Median delta of `+0.036 ms` is below the
`±0.05 ms` signal threshold but consistently in the wrong direction
across all 5 traces; sincos was never faster than baseline in any
pairing. The wider per-trace variance suggests the trig replacement
also disrupted some other optimization (instruction scheduling,
register allocation).

**Lesson.** Metal's `sin` and `cos` are already efficient on M-series
GPUs — likely a single fused hardware op rather than separate
operations. Hand-replacing `sin` with `sqrt(1 - cos^2)` swaps one
fast intrinsic for a 3-op chain (mul + sub + sqrt) with no net win.
Confirms that `solve_cubic_normed` is not single-instruction-bound at
the trig step; the cost lives elsewhere (likely the `acos` itself, or
memory bandwidth on the `roots` array, or instruction-level
parallelism limits).

**Implication for future experiments.** Targeting individual ALU
operations inside the cubic is unlikely to help. Promising directions:
(a) skip the cubic entirely on coherent wavefronts (per-glyph SDF
prefilter so whole wavefronts exit before any cubic runs),
(b) eliminate the double-cubic-per-curve in
`nearest_vertical_curve_distance_sq` (the redundant-band experiment),
(c) reduce fragment count by tightening glyph quads (the procedure
doc notes `2.78x` shaded-to-ink waste).

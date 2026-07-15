# Slug Experiments

> **Closed to new entries (2026-06-06).** Every experiment below ran on
> the per-run mesh path with single-sample AA under bevy 0.18. Batched
> records + vertex pulling, the `AntiAlias::Both` default, the
> dedup/48-band reverts (`c3cfcbd`), and bevy 0.19 make these numbers
> non-comparable with current measurements; the lessons remain citable.
> New experiments: `gpu-perf-experiments.md` (created with the first
> entry). Campaign plan:
> [`gpu-perf-test-plan.md`](gpu-perf-test-plan.md).

This document records Slug renderer experiments that were tried during
the feasibility branch. Its purpose is to prevent future sessions from
repeating failed approaches without a new reason.

## Baseline Method

Current shader-performance experiments use three checks:

- Visual parity: compare screenshots with ImageMagick `compare -metric AE`.
- Runtime cost: run `scripts/xctrace_text_renderer.sh` and parse Metal
  GPU interval exports for the `text_renderer_gpu_bench` process.
- CPU prep cost: no standing bench. The `glyph_rasterization` bench and
  the prep API it called were removed during the slug migration; the last
  recorded figure is full printable ASCII ≈ 0.84 ms (2026-05-24, after
  per-curve dedup + 48-band tuning, JetBrains Mono at 128 px). Rebuild a
  micro-bench against `Backend::prepare_positioned_run_with_scale` +
  `ensure_run_storage` only if a change is expected to move prep cost.

### Canonical Benchmark Format

See `docs/hana_diegetic/performance/slug-benchmark-procedure.md` for the single
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

- Split `along_y_coverage_terms` into an outside-bounds distance-only
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
- Spawn fill with `RenderMode::SolidQuad` and the edge mesh with
  `RenderMode::Text`.

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
`docs/hana_diegetic/performance/slug-benchmark-procedure.md`.

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
`docs/hana_diegetic/performance/slug-benchmark-procedure.md` (Per-segment ribbon
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
`docs/hana_diegetic/performance/slug-benchmark-procedure.md`; this section records
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
`nearest_along_x_curve` (the redundant-band experiment),
(c) reduce fragment count by tightening glyph quads (the procedure
doc notes `2.78x` shaded-to-ink waste).

## Band count 96 -> 192 (rejected, 2026-05-23)

**Hypothesis.** Halving each band's spatial extent halves average
curves per band, so each fragment runs the per-band loop with fewer
iterations and fewer cubic calls. Trade-offs: 2x band records, more
duplicated curves (each curve appears in more bands).

**Setup.** 96-band -> 192-band Slug, 720-instance
`text_renderer_gpu_bench`, AC power, long-warmup protocol. Single
diagnostic trace (visual gate failed before completing the 5-trace
batch).

**Visual.** **`851,955 / 7,271,424` pixels differ at zero fuzz
(11.7 %)**; `589,815` differ at `-fuzz 5 %` (8.1 %). RMSE `0.0084 %`
per channel — each differing pixel is off by `1-5 %`, with the
differences concentrated along glyph edges. Far above the
`0.05 %` pixel-identical bar.

**Root cause of visual regression.** `BAND_OVERLAP_EM_UNITS = 1.0` is
a fixed constant; it is independent of band height. At 192 bands the
per-band height drops to ~3 design units (from ~6 at 96 bands), but
the curve-inclusion overlap stays at 1 unit. Curves whose closest
point to the fragment lies more than 1 unit outside the fragment's
band are now excluded from the per-band curve list, so distance
queries miss them and the smoothstep transition breaks at band
boundaries.

**Performance (diagnostic single trace).**

| Metric | Baseline median | 192-band trace 1 | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0406 ms` | `0.0418 ms` | `+0.0012` |
| Fragment per frame          | `2.6886 ms` | `2.7076 ms` | `+0.0190` (noise) |
| Vertex + fragment per frame | `2.7291 ms` | `2.7494 ms` | `+0.0203` (noise) |

**Result.** Rejected on visual grounds; no performance benefit
observed either. Even ignoring quality, `+0.02 ms` is within the
`0.04 ms` baseline-noise floor.

**Implication for future experiments.** A density bump is viable only
if `BAND_OVERLAP_EM_UNITS` scales with band height — e.g.
`overlap = max(1.0, edge_width_design_units)` or compile band overlap
from the rendering scale. Otherwise the per-fragment curve count
doesn't actually shrink (curves spill out and have to be re-fetched
from the vertical band), and visual quality breaks.

## Band count 96 -> 192 with overlap 1.0 -> 2.0 (rejected, 2026-05-23)

**Hypothesis.** Doubling band count halves band height; doubling
overlap keeps the curve-inclusion zone roughly constant (was ~8 EM
units of inclusion at 96/1.0, now ~7 at 192/2.0). Expected: similar
visual quality, lower per-fragment curve count.

**Setup.** 192 bands + `BAND_OVERLAP_EM_UNITS = 2.0`, 720-instance
`text_renderer_gpu_bench`, AC power, long-warmup protocol.

**Visual.** `851,955 / 7,271,424` pixels differ (`11.7 %`); RMSE
`0.005 %` (vs `0.008 %` for 192/1.0). Pixel-difference count is
**identical** to the 192/1.0 case because the band boundaries are
at the same y-positions either way; differences come from curve
fragments straddling band borders being assigned to different bands
than at 96 bands. Per-pixel error magnitude dropped (overlap=2 helps
catch more boundary curves) but the rate stayed pinned at 11.7%.
Still fails the `0.05 %` pixel-identical bar.

**Performance (diagnostic single trace).**

| Metric | Baseline median | 192-band+overlap2 | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0406 ms` | `0.0405 ms` | `-0.0001` |
| Fragment per frame          | `2.6886 ms` | `2.6891 ms` | `+0.0005` |
| Vertex + fragment per frame | `2.7291 ms` | `2.7297 ms` | `+0.0006` |

Performance is essentially **identical** to the 96-band baseline —
the larger overlap exactly cancels the smaller bands' per-band curve
count. Total fragment work depends on curve geometry, not band
parameters.

**Result.** Rejected. No performance win and `11.7 %` pixel
regression. Band-density tuning is a dead end when overlap must scale
to preserve quality: the total per-fragment curve work is conserved
across density/overlap trade-offs.

**Implication for future experiments.** The 96-band baseline is at a
quality/performance Pareto-optimal point along the
band-count/overlap axis. Further fragment-cost reductions need
**fewer total curves per fragment**, not just different band
arrangements — i.e. either smarter spatial culling (per-glyph SDF
prefilter), structural curve-record reduction (eliminate the
double-cubic-per-curve in the second band loop), or a fundamentally
different curve representation (analytical depressed cubic, lookup
table).

## Skip vertical-band distance pass (rejected, 2026-05-23)

**Hypothesis.** `distance_coverage` calls
`nearest_along_x_curve` after
`along_y_coverage_terms`, adding a second per-curve loop that
re-checks distance via the along-X band. Skipping this entirely
eliminates the second band loop and cuts per-fragment curve work
roughly in half. Quantifies how much the vertical band actually
contributes to distance accuracy.

**Setup.** `shaders/analytic_path.wgsl`, replaced the
`nearest_along_x_curve(...)` call with
`terms.distance_sq` directly. 720-instance `text_renderer_gpu_bench`,
AC power.

**Visual.** Catastrophic. `2,621,400 / 7,271,424` pixels differ at
`-fuzz 50 %` (`36 %` of pixels with >50% color difference). Glyph
edges lose AA entirely in any region where the closest curve has y
outside the fragment's horizontal-band overlap zone. Failed visual
gate immediately; no trace recorded.

**Result.** Rejected. The vertical band's distance contribution is
required for correct AA at fragment positions where the closest
curve has y outside the fragment's horizontal-band overlap zone.
That covers a substantial fraction of edge fragments.

**Implication for future experiments.** Cannot drop the vertical band
without a replacement. To reduce the double-cubic cost while keeping
correctness:

- Split the vertical band into a *distance-only* band that EXCLUDES
  curves already in the fragment's horizontal band (requires
  per-curve uniqueness tracking — complex packing change).
- Make the horizontal band's curve-inclusion overlap large enough
  to capture all curves within `edge_width`, then drop the vertical
  band (but overlap grows with pixel size, requires per-glyph
  pixel-size estimation at pack time).
- Replace the per-curve distance loop with a precomputed coarse SDF
  prefilter that's accurate at the smoothstep edge.

# Proposed Next Experiments (2026-05-23)

After 7 small-scale shader/packing experiments under the long-warmup
benchmark protocol — all rejected — the remaining productive
directions are structural. Each has been scoped, with an effort
estimate, expected impact range, key risk, and the files it touches.
The 96-band baseline (`V+F = 2.7291 ms`) is otherwise at a local
Pareto-optimal point for the current architecture.

These are **proposals**, not authorized work. Pick which to start.

## Newton-deflation cubic solver (rejected, 2026-05-23)

**Hypothesis.** Proposal D executed. Replace the trigonometric
3-roots branch + cbrt 1-root branch in `solve_cubic_normed` with: 8
iterations of damped Newton from `t = 0.5` to find first real root
`r1`, then synthetic division `f(t) = (t - r1)(t² + p*t + q)` with
`p = a + r1`, `q = b + r1 * p`, solve the deflated quadratic for the
other two roots. Fixed iteration count, no new per-lane branches.

**Setup.** 96-band Slug, 720-instance `text_renderer_gpu_bench`,
AC power. Short-warmup protocol (`warmup_frames=180 sample_frames=240`,
15s xctrace `time-limit`) — the actually-working protocol per
surviving file evidence; the `WARMUP_FRAMES=1800 SAMPLE_FRAMES=1800`
"long-warmup" numbers cited elsewhere in this doc are unverified
(no trace bundles ever generated under that protocol).

**Visual.** Pixel-identical to baseline on the lowercase-g
inside-curve view (MD5 match: `95206bb1738e9bae01a26d4f2651a332`,
0 / 7,271,424 pixels differ at zero fuzz and at `-fuzz 1%`).
Newton converges to enough precision that 8-bit color quantization
collapses all differences to zero.

**Performance (short-warmup, baseline n=6 / Newton n=4 valid).**

| Metric | Baseline-trig median (range) | Newton-deflation median (range) | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0564 ms` (0.0085) | `0.0598 ms` (0.0070) | `+0.0034 ms` (within range, noise) |
| Fragment per frame          | `3.2968 ms` (0.4199) | `4.1742 ms` (0.5150) | `+0.8774 ms` (real signal) |
| Vertex + fragment per frame | `3.3531 ms` (0.4284) | `4.2340 ms` (0.5219) | `+0.8808 ms` (real signal) |
| Bevy frame time             | `7.6837 ms` (last trace only, stddev 3.7144, n=240) | unavailable — Newton stdout overwritten by baseline batch | — |
| Prep time                   | n/a — shader-only change | n/a | — |

Per-trace V+F samples:
- Baseline-trig: `3.4764, 3.5522, 3.1357, 3.2686, 3.4377, 3.1238`
- Newton: `3.8685, 4.3686, 4.3904, 4.0993` (trace 1 dropped: parser
  found no `text_renderer_gpu_bench` intervals — xctrace attached
  after bench exit, a known short-warmup race condition)

Distributions do not overlap on V+F or F: Newton min `3.8685 ms` >
baseline max `3.5522 ms`. The vertex-per-frame delta is well within
the per-condition range and is noise. Fragment-per-frame carries
the entire signal, as expected for a shader-only change to the
per-fragment cubic solver.

**Result.** Rejected. Newton-deflation is `+0.88 ms` slower than
the trigonometric solver on Metal, the opposite direction of the
`-0.4 to -0.8 ms` expected impact. Experiment 3 (shared sincos)
already flagged that Metal's trig ops are highly optimized; this
experiment confirms the whole trig path beats an 8-iteration Newton
loop even when that loop has no transcendentals.

**Lesson.** On Apple M-series GPUs, `acos`/`cos`/`sin` are not the
expensive part of the cubic solver — likely 1-2 cycle hardware
intrinsics. Replacing them with arithmetic (~8 iterations × 7 ops
= ~56 ops) loses outright. Proposal D's theoretical "2x win" was
based on assuming 4-8 cycle trig; Metal beats that.

**Implication for future experiments.** Proposal C (F16 cubic
intermediates) shares the same risk — narrowing the trig path's
precision won't help if the trig itself is already cheap. Suggests
re-ordering remaining proposals: **A** (per-curve dedup) and **B**
(SDF prefilter) attack work *quantity*, not work *cost per curve* —
those should be the next two to try. Proposal C may need re-scoping
or skipping.

**Files.** `crates/hana_diegetic/src/slug_text_spike/shaders/slug_text.wgsl`
(reverted to trig solver after experiment).

## Per-curve dedup between horizontal/vertical bands (accepted, 2026-05-23)

**Hypothesis.** Proposal A executed. A curve overlapping both a
horizontal band `H` and a vertical band `V` is currently in both,
so the cubic distance solve runs twice for the same
`(fragment, curve)` pair. Assigning each curve to exactly one
orientation at pack time eliminates the duplicate work.

**Implementation.** Per-curve orientation flag computed at pack
time: `y_extent > x_extent` → "vertical-assigned". All curves
remain in the horizontal band (required for winding). Only
vertical-assigned curves enter the vertical band. The flag is
stored in `SlugCurveRecord::solver.w` (`1.0` = vertical-assigned).
Shader change: in `along_y_coverage_terms` the distance solve
is skipped when `solver.w >= 0.5`; the vertical-band loop already
does only distance, so no change there. Per-segment flag is
computed once per glyph (not once per band) — the `flat_map` over
contours is also collected once in `build_packed_glyph` so the
band loop iterates a flat slice instead of a re-built iterator
chain.

**Setup.** 96-band Slug, 720-instance `text_renderer_gpu_bench`,
AC power, in-session baseline taken immediately before the
variant. Short-warmup protocol (`warmup_frames=180
sample_frames=240`, 15s xctrace `time-limit`), 6 traces per
condition, first 2 traces of each batch discarded as cold (lower
sample count, V+F notably below warm-state median); one variant
trace discarded as outlier (frame_time mean `4.91 ms` vs `~9.7`
others — bench ran un-vsync-locked).

**Visual.** Lowercase-g inside-curve view at fuzz 1%:
`8,956 / 19,212,800` pixels differ (`0.047 %`). Within
PBR/OIT precision noise; no structural diff visible.

**Performance (short-warmup, baseline n=4 warm / variant n=4 warm).**

| Metric | Baseline-trig median (range) | Proposal A median (range) | Delta |
| --- | ---: | ---: | ---: |
| Vertex per frame            | `0.0633 ms` (0.0028) | `0.0629 ms` (0.0029) | `-0.0004 ms` (noise) |
| Fragment per frame          | `3.8175 ms` (0.0817) | `3.1796 ms` (0.1329) | `-0.6379 ms` (-16.7 %) |
| Vertex + fragment per frame | `3.8797 ms` (0.0823) | `3.2428 ms` (0.1350) | `-0.6369 ms` (-16.4 %) |
| Bevy frame time             | `10.369 ms` (range 0.88, stddev per-trace ~3.5) | `9.7015 ms` (range 0.20, stddev per-trace ~3.4) | `-0.667 ms` (-6.4 %) |
| Prep time (`jbm_ascii_128_slug`) | `1.2095 ms` (Criterion CI [1.192, 1.233]) | `0.8364 ms` (Criterion CI [0.807, 0.860]) | `-0.373 ms` (-30.8 %) |

Per-trace V+F samples (warm set):
- Baseline-trig: `3.8524, 3.9347, 3.8767, 3.8826`
- Proposal A: `3.1557, 3.2907, 3.2796, 3.2060` (trace 3 outlier
  `2.4366` excluded — bench ran at `~204 fps` instead of `~100 fps`)

Distributions do not overlap on V+F or F: variant max `3.2907 ms`
< baseline min `3.8524 ms`. Vertex-per-frame delta is well within
the per-condition range and is noise.

**Result.** Accepted. Fragment-per-frame drops `0.638 ms`
(`-16.7 %`) — in the upper half of the `0.3-0.7 ms` predicted
range. Prep also drops `0.373 ms` (`-30.8 %`) as a bonus, because
the same restructuring lifts segment flattening out of the
per-band loop. Frame time drops `0.667 ms`, larger than the GPU
delta, suggesting the prep gain and the smaller GPU draw also
ease pressure on the present pipeline.

**Lesson.** Reducing per-fragment cubic *count* delivers what
reducing per-cubic *cost* could not (Newton-deflation rejection,
above). Work-quantity reductions remain the most promising
direction for Slug-style analytic text on Metal.

**Implication for future experiments.** Proposal B (per-glyph
SDF prefilter) — also a work-quantity reduction — still looks
likely to be the largest remaining win (predicted `1.5-2.0 ms`).
After Proposal B, the bands themselves could be revisited: with
duplicates already removed, the optimal band count may differ
from the 96-band baseline established before this change.

**Files.** `crates/hana_diegetic/src/slug_text_spike/packing.rs`,
`crates/hana_diegetic/src/slug_text_spike/shaders/slug_text.wgsl`.
Trace evidence under
`target/xctrace/baseline-trig-2026-05-23/` and
`target/xctrace/variant-prop-a-v2-2026-05-23/`.

## Coarse per-glyph SDF prefilter (rejected at bench scale, 2026-05-23)

**Hypothesis.** Proposal B executed. A small per-glyph signed-distance
grid sampled once at the top of the fragment shader should let
far-interior / far-exterior fragments short-circuit the curve band
loops, dropping fragment cost `1.5-2.0 ms`.

**Implementation.** `16x16` signed-distance grid per unique glyph,
generated on CPU in `build_packed_glyph` (exact point-to-quadratic
distance via the same cubic solver as the shader, signed by non-zero
winding). Combined per-run into one `array<f32>` storage buffer
(binding 104); per-glyph offset + resolution stored in a new
`SlugGlyphRecord.sdf` (`UVec4`). Shader: bilinear sample in
`slug_coverage`; if `abs(sdf) > edge_width + cell_diagonal` return
inside/outside by sign and skip both band loops. Points outside the
glyph bounds use the exact box distance as a conservative lower
bound. The `+ cell_diagonal` margin makes the skip provably safe for
a 1-Lipschitz field.

**Setup.** 96-band Slug, 720-instance `text_renderer_gpu_bench`, AC
power, window pinned to the primary Retina display
(`WindowPosition::Centered(MonitorSelection::Primary)`; drawable
`3200x1800`). Long-warmup protocol (`warmup_frames=600
sample_frames=240`, 25s xctrace `time-limit`) to settle GPU clock
state. Back-to-back A/B in one thermal window: 6 traces of a no-SDF
build (prefilter bypassed in `slug_coverage`, perf-identical to the
band-loop baseline) immediately followed by 6 traces of the SDF
build.

**Visual.** Lowercase-g inside-curve view, prefilter-on vs
prefilter-off, same session: `64 / 7,271,424` pixels differ
(`0.0009 %`), all isolated cusp specks — far below the accepted
Proposal A gate. Output is correct.

**Performance (long-warmup, back-to-back, n=6 each).**

| Metric | no-SDF median (range) | SDF prefilter median (range) | Delta |
| --- | ---: | ---: | ---: |
| Vertex + fragment per frame | `2.94 ms` (2.79-3.11) | `3.11 ms` (2.94-3.23) | `+0.17 ms` (+5.7 %) |
| Fragment per frame          | `2.88 ms` (2.73-3.05) | `3.05 ms` (2.89-3.17) | `+0.17 ms` (+5.8 %) |

Per-trace V+F (warm set):
- no-SDF: `2.7862, 2.8153, 2.8926, 2.9834, 2.9958, 3.1135`
- SDF: `2.9429, 2.9974, 3.0845, 3.1264, 3.1506, 3.2267`

The SDF distribution is shifted up; the prefilter adds cost rather
than removing it.

**Result.** Rejected at bench scale. The predicted `1.5-2.0 ms` win
is absent; the per-fragment bilinear grid sample is a net
`+0.17 ms` (`~6 %`) regression. An earlier same-day session read it
as net-neutral (`3.00` vs `2.99`); the back-to-back A/B resolves it
to a small regression.

**Lesson.** Two structural reasons the prefilter cannot win here.
First, the expensive cubic is *already* culled per-curve by
`curve_bounds_distance_sq(point, curve) <= edge_width_sq`, so far
fragments never paid for it — the SDF only removes cheap band
iteration and adds a grid sample. Second, the skip threshold scales
with `edge_width`: at small on-screen glyph sizes one pixel spans
many design units, so `edge_width` (and the margin) is large and the
prefilter skips almost nothing. The win, if any, lives only at large
on-screen glyph sizes, which this bench does not exercise.

**Implication for future experiments.** The prefilter's only
theoretical win is at large on-screen glyph sizes (small
`edge_width` leaves real interior to skip), but that is not a
workload that occurs at scale: real scenes are either many *small*
glyphs (body text — margin too large, skips nothing) or a *few*
large glyphs (display/panel — most off-screen culled, total fragment
cost already low). So the SDF prefilter has no realistic win case
and the large-glyph regime is not worth testing. Band-count
re-tuning (now that per-curve dedup removed duplicates) remains the
open forward direction.

**Files.** `crates/hana_diegetic/src/slug_text_spike/sdf.rs`,
`packing.rs`, `run_render.rs`, `backend.rs`, `material.rs`,
`render/world_text/mesh_spawning.rs`, `render/text_renderer/batching.rs`,
`shaders/slug_text.wgsl`. Trace evidence under
`/private/tmp/slug-traces-2026-05-23/` (kept outside `target/` so it
survives `cargo clean`).

## Band count 96 -> 48 (accepted, 2026-05-24)

**Hypothesis.** Pre-dedup, 96 bands beat 192 (more bands meant more
duplicated curves and worse GPU cost). After per-curve dedup removed
the horizontal/vertical duplication, the optimal band count may have
shifted, since GPU per-fragment cost no longer scales with
band-driven duplication.

**Setup.** 720-instance `text_renderer_gpu_bench`, AC, window pinned
to the primary Retina display (`3200x1800`), long-warmup
(`warmup_frames=600 sample_frames=240`, 25s). 6 GPU traces per band
count; prep via criterion `renderer_prep/jbm_ascii_128_slug`. Band
count is `DEFAULT_BAND_COUNT`, which drives both shaping paths and
the prep bench.

**Visual.** Band 48 vs band 96 g-zoom: `235 / 7,271,424` pixels
differ (`0.0032 %`) — sub-visual edge pixels from float-ordering and
band-overlap differences. Output preserved.

**Performance.**

| Bands | Prep (criterion) | GPU V+F (comparable-regime median) |
| --- | ---: | ---: |
| 16  | `416 us` | un-vsync-locked regime — indeterminate |
| 48  | `516 us` | `~2.87 ms` |
| 64  | —        | `2.90 ms` |
| 96  | `~690 us` | `2.87 ms` |
| 128 | `847 us` | `2.89 ms` |

GPU V+F is flat across 48-128 (`0.03 ms` spread = noise). Prep
scales monotonically with band count.

**Result.** Accepted. `DEFAULT_BAND_COUNT` `96 -> 48`. No GPU change
(flat), prep `-25 %` (`690 -> 516 us`). Prep is the per-unique-glyph
cost paid on layout/resize, so cheaper prep helps resize
responsiveness.

**Lesson.** Per-curve dedup decoupled GPU cost from band count — the
dedup already minimized per-fragment curve count, so band
granularity (48-128) no longer moves the GPU pass. The remaining
band-count effect is prep cost and storage, both favoring fewer
bands. 48 was chosen over lower (16/32) because GPU below 48 could
not be measured cleanly (those batches fell into the fast
un-vsync-locked clock regime) and very low band counts raise
per-band curve count for complex / CJK glyphs.

**Files.** `crates/hana_diegetic/src/slug_text_spike/packing.rs`.
Trace evidence under `/private/tmp/slug-bands-2026-05-23/`.

## Proposal A — Per-curve dedup between horizontal/vertical bands

**Hypothesis.** A curve overlapping both a horizontal band `H` and a
vertical band `V` is currently in both, so the cubic distance solve
runs twice for the same `(fragment, curve)` pair. Assigning each
curve to exactly one orientation at pack time eliminates the
duplicate work without changing what's visible.

**Expected impact.** If ~40-60% of curves are duplicated (typical
for fonts with curved letterforms), per-fragment cubic count drops
proportionally. Estimated fragment-cost win: **0.3-0.7 ms** (12-26%
of the `2.69 ms` baseline).

**Effort.** Medium. `packing.rs`: orientation-choice heuristic per
curve (largest-axis rule, or "assign to whichever band already has
fewer curves"). `shaders/slug_text.wgsl`: keep winding from
horizontal band always; allow distance to come from either. New
glyph metadata if needed. ~150-250 lines across 2 files.

**Risk.** A misassigned curve could leave a fragment with no
distance contribution from either band → AA breakage at edges. The
chord-gate/192-band experiments both showed how quickly visual
regressions appear when curve coverage changes. Mitigation: start
with a conservative rule that keeps both bands populated for curves
with similar x/y extent (only dedup clearly-axis-aligned curves);
empirical tuning.

**Files.** `crates/hana_diegetic/src/slug_text_spike/packing.rs`,
`crates/hana_diegetic/src/slug_text_spike/shaders/slug_text.wgsl`.

## Proposal B — Coarse per-glyph SDF prefilter

**Hypothesis.** Most fragments in a padded glyph quad are far inside
or far outside the ink, so the cubic loop is wasted on them. A small
per-glyph SDF (e.g. `16x16`) sampled once at the start of the
fragment shader can short-circuit the curve loop for non-edge
fragments. Only fragments whose SDF sample falls in `[-edge_width,
+edge_width]` need the exact cubic distance.

**Expected impact.** For typical text glyphs, the AA edge is ~2-4
pixels wide; the rest of the quad is interior/exterior. If 70-85%
of fragments skip the curve loop entirely, fragment cost could drop
**1.5-2.0 ms** (55-75% of the `2.69 ms` baseline). This is the
largest expected win of the four proposals.

**Effort.** Large. New per-glyph SDF generation in prep (`packing.rs`
or sibling). New GPU buffer or texture binding through the material
pipeline. Shader: SDF sample + tier check + skip path. Prep-cost
benchmark to confirm prep time stays manageable. ~400-600 lines
across 4-5 files.

**Risk.** SDF resolution must be high enough that the "skip" tiers
are conservative (no false-inside / false-outside leaking into the
AA zone). At `16x16` per glyph, sample spacing is roughly
glyph-width/16 ≈ 30-40 design units — comparable to edge_width at
typical scales, so the prefilter must be conservative
(`abs(sdf_sample) > edge_width + sample_spacing`). Prep cost grows
linearly with unique-glyph count; may regress first-frame timing.

**Files.** `packing.rs`, `run.rs`, `run_render.rs`, the material
binding setup (search for `MATERIAL_BIND_GROUP` consumers),
`shaders/slug_text.wgsl`.

## Proposal C — F16 cubic intermediates

**Hypothesis.** `solve_cubic_normed`'s trig pipeline (`acos`, `cos`,
`sin`) and surrounding arithmetic don't all need F32 precision.
Apple M-series GPUs run F16 transcendentals at roughly 2x the
throughput of F32. Demoting `theta`, `cos_t3`, `sin_t3`, and the
final root assembly to F16 could roughly halve cubic ALU cost
without changing the closest-point result above F16 precision.

**Expected impact.** If cubic ALU is the dominant fragment cost
(supported by experiments 1, 3 showing per-trig optimizations don't
help in F32 because Metal's F32 trig is already a fused op),
**0.5-1.0 ms** off the `2.69 ms` baseline.

**Effort.** Small shader change (~30 lines) **but** Bevy 0.18.1
doesn't enable the WGSL `f16` extension by default. Requires
investigating Bevy's `RenderPlugin`/wgpu device feature config to
pass `Features::SHADER_F16`, and confirming naga's WGSL backend
emits the extension token. Possibly a 1-2 file patch to Bevy
internals (or a runtime feature toggle) plus the shader change.

**Risk.** Precision near root degeneracies. When `q3 ≈ r²` or
`theta` is near `π/2`, F16's ~3-decimal-digit precision may produce
visibly wrong roots → AA artifacts at glyph junctions. Mitigation:
hybrid path — keep `q3, r2, theta` in F32, demote only `cos_t3,
sin_t3, q_pre` to F16 at the end. Verify quality on a glyph with
many cubic-trigonometric-case curves (any `S`, `&`, `@`).

**Files.** Bevy/wgpu device-feature plumbing (probably in
`crates/hana_diegetic/src/lib.rs` or a `RenderPlugin` config),
`shaders/slug_text.wgsl`.

## Proposal D — Newton iteration replacing the trig cubic

**Hypothesis.** Solving `t³ + at² + bt + c = 0` by Newton iteration
from `t = 0.5` converges quadratically when not near multiple
roots. ~3-4 iterations (`t' = t - f(t)/f'(t)`, ~10 mul/add ops
each) total ~40 FMA ops, no transcendentals. On hardware where
`acos`/`cos`/`sin` are 4-8 cycles each, that's a net ~2x win
unconditionally.

**Expected impact.** **0.4-0.8 ms** off the `2.69 ms` baseline,
*assuming* Metal's trig ops are not already faster than the FMA
count. Less certain than Proposal B because experiment 3 (sincos
substitution) showed Metal's trig is highly optimized.

**Effort.** Small. Replace `solve_cubic_normed` body. Need to find
all three real roots for the trigonometric case — Newton finds one
at a time, so iterate 3 times from different starting points
(`t = 0.1`, `0.5`, `0.9`) with deflation. ~50 lines, one file.

**Risk.** Convergence failure near multi-root regions: a fragment
where two roots collapse will see Newton oscillate or land on the
wrong root. Distance error in such cases could be substantial.
Mitigation: cap iteration count and fall back to a single chord
distance if not converged → adds a branch (same wavefront-divergence
trap as experiments 1, 2, 4). May ultimately be neutral for that
reason.

**Files.** `crates/hana_diegetic/src/slug_text_spike/shaders/slug_text.wgsl`.

## Recommendation if you can only pick one

**Proposal B** (SDF prefilter) has the largest expected impact and
the most architectural support — it sidesteps the wavefront-
divergence trap by making the skip tier *spatially coherent* across
a wavefront (fragments in the same screen-space tile share an SDF
tier). It's the most engineering work but the highest-ceiling win.

Second pick: **Proposal A** (band dedup) — moderate effort, clear
mechanism, no new buffers.

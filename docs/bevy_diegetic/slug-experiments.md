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

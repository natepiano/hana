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

## Rejected Experiments

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

# Slug Benchmark Procedure

This document is the **single canonical procedure** for measuring and
recording Slug shader benchmark comparisons. It defines the comparison
table, the steps to populate a new column, and the tooling used.
Older empty templates and duplicate tables in other Slug documents
have been removed; new benchmark entries must use the table below and
link back here.

## Status (2026-06-06)

The 2026-05-23 baseline column below is **retired**. Every number in this
table was measured on the per-run mesh path with single-sample AA under
bevy 0.18. Since then: batched glyph records + vertex pulling replaced
the per-run mesh path, `TextAntiAlias::Both` became the default fragment
path, per-curve dedup was reverted and `any_outside_neighbor` added
(`c3cfcbd`), and the engine moved to bevy 0.19.0-rc.2 / wgpu 29. No new
column may be compared against the retired baseline. The new campaign
starts with a fresh baseline column — see
[`gpu-perf-test-plan.md`](gpu-perf-test-plan.md), Phase 0.

## Comparison Table

One row per metric, one column per configuration, plus `Delta` and
`Meaning`. Delta is always `(newest configuration) − 96 Bands baseline`.

| Metric | 96 Bands (baseline, 2026-05-23, long-warmup median of 5) | Delta | Meaning |
| --- | ---: | ---: | --- |
| Vertex per frame            | `0.0406 ms` | — | GPU vertex work, summed per frame. Range across 5 traces: `0.0006 ms`. |
| Fragment per frame          | `2.6886 ms` | — | GPU fragment work, summed per frame (Slug analytic + OIT sub-pass). Range across 5 traces: `0.0395 ms`. |
| Vertex + fragment per frame | `2.7291 ms` | — | Total GPU transparent-pass work per frame. Range across 5 traces: `0.0400 ms`. |
| Bevy frame time             | `8.5586 ms` | — | Whole-app frame pacing. Heavy stddev (~4 ms); sanity check only. Not re-measured under the long-warmup protocol; reported value is from the short-warmup baseline and may shift. |
| Prep time                   | `1.1703 ms` | — | One-time Slug prep (Criterion `jbm_ascii_128_slug`). Carried forward from pre-parser-fix measurement; unchanged by shader-only experiments. |

The baseline is the **96-band Slug candidate** measured on AC with the
current `parse_gpu_intervals.py`. Numbers are the **median across 5
back-to-back traces** using the **long-warmup protocol**
(`WARMUP_FRAMES=1800 SAMPLE_FRAMES=1800` with a 35s xctrace
`time-limit`), which gives steady-state GPU thermal numbers. Trace-to-
trace variation under this protocol is ≤`0.05 ms`; experiments are
considered to have signal only when the **median delta exceeds
`±0.05 ms`** relative to the baseline median.

Previous short-warmup baselines (3.22 ms, 3.48 ms) included cold-start
GPU boost-clock samples and had trace-to-trace variation of
`0.16–0.60 ms`. They are not comparable to long-warmup numbers and are
retired. The pre-2026-05-23 multi-column comparison table is also
retired (per-interval vs per-frame aggregation mismatch — see
`slug-experiments.md` for history).

## How To Populate A New Column

Run these steps from the repository root. The canonical scene is the
720-instance `text_renderer_gpu_bench` Slug run.

### 1. Preflight

1. Make sure the worktree contains the exact implementation you want to
   measure.
2. Close every other Bevy example. The benchmark script refuses to run if
   another example process is active, but check before starting so the
   trace is not polluted:

   ```bash
   pgrep -fl 'target/(debug|release)/examples/' || true
   ```

3. If a `slug_text` example is still running from screenshot work, shut it
   down through the helper:

   ```bash
   bash scripts/slug_text_g_zoom.sh --shutdown-only
   ```

4. If any other Bevy example remains, close it before continuing.

### 2. Visual Check

1. Capture a baseline image from the current accepted candidate before
   changing the renderer, or use a saved baseline that was captured from
   the same camera view:

   ```bash
   bash scripts/slug_text_g_zoom.sh --restart --view g --screenshot /tmp/slug-baseline-g.png
   bash scripts/slug_text_g_zoom.sh --shutdown-only
   ```

2. After applying the candidate implementation, capture the same view:

   ```bash
   bash scripts/slug_text_g_zoom.sh --restart --view g --screenshot /tmp/slug-candidate-g.png
   bash scripts/slug_text_g_zoom.sh --shutdown-only
   ```

3. Compare the images:

   ```bash
   magick compare -metric AE /tmp/slug-baseline-g.png /tmp/slug-candidate-g.png null:
   magick compare -metric AE -fuzz 1% /tmp/slug-baseline-g.png /tmp/slug-candidate-g.png null:
   magick compare -metric RMSE /tmp/slug-baseline-g.png /tmp/slug-candidate-g.png null:
   ```

4. Record the visual result in the experiment notes. Do not populate this
   canonical performance table if the candidate has a structural visual
   regression.

### 3. GPU Trace

1. Confirm again that no Bevy example is running:

   ```bash
   pgrep -fl 'target/(debug|release)/examples/' || true
   ```

2. Record the Slug trace for the canonical 720-instance scene using
   the **long-warmup protocol**. The script honors `WARMUP_FRAMES`
   and `SAMPLE_FRAMES` env vars. Default short-warmup numbers are
   noise-bound; comparisons against the canonical baseline above
   must use the long-warmup settings.

   ```bash
   WARMUP_FRAMES=1800 SAMPLE_FRAMES=1800 \
     bash scripts/xctrace_text_renderer.sh record slug 35s 720
   ```

   Run **5 traces back-to-back** and report the median. Single-trace
   numbers under the short-warmup default vary by 0.16–0.60 ms and
   cannot resolve sub-0.5 ms differences.

3. Export the Metal GPU interval table:

   ```bash
   bash scripts/xctrace_text_renderer.sh export slug
   ```

4. The expected trace outputs are:

   ```text
   target/xctrace/text-renderer-slug.trace
   target/xctrace/text-renderer-slug-gpu-intervals.xml
   target/xctrace/text-renderer-slug.stdout.log
   ```

### 4. GPU Metric Extraction

Run the parser:

```bash
scripts/parse_gpu_intervals.py target/xctrace/text-renderer-slug-gpu-intervals.xml
```

The script filters the `metal-gpu-intervals` rows to:

- process contains `text_renderer_gpu_bench`
- label contains `main_transparent_pass_3d:main_transparent_pass_3d`
- channel is `Vertex` or `Fragment`

and prints, one `key=value` per line. Use the **per-frame** columns
for the comparison table; the per-interval columns are diagnostic
only and do **not** add together cleanly because a frame can hold
more than one fragment interval (OIT sub-pass + main transparent
pass each count as one).

Per-frame columns (use these for the table):

- `vertex_per_frame_mean_ms` → `Vertex per frame` cell.
- `fragment_per_frame_mean_ms` → `Fragment per frame` cell.
- `vertex_plus_fragment_per_frame_mean_ms` → `Vertex + fragment per frame` cell.

These three sum: `vertex_per_frame + fragment_per_frame ==
vertex_plus_fragment_per_frame` exactly.

Per-interval columns (diagnostic only — do not put in the table):

- `vertex_per_interval_mean_ms` — mean cost of a single GPU vertex
  interval.
- `fragment_per_interval_mean_ms` — mean cost of a single GPU
  fragment interval. Usually lower than `fragment_per_frame` because
  most frames have two fragment intervals (OIT + main).

If the parser exits non-zero with "no matching intervals found",
inspect the XML — the column element names may have changed in a new
Xcode and the parser's heuristics need adjustment.

### 5. Bevy Diagnostic Extraction

Read these lines from `target/xctrace/text-renderer-slug.stdout.log`:

```text
frame_time: mean_ms=...
render_elapsed_cpu_sum: mean_ms=...
render/main_transparent_pass_3d/elapsed_cpu: mean_ms=...
```

Populate:

- `Bevy frame time` from `frame_time`.
- `Render CPU sum` from `render_elapsed_cpu_sum` if the current table
  includes that row.
- `Transparent pass CPU` from
  `render/main_transparent_pass_3d/elapsed_cpu` if the current table
  includes that row.

The benchmark example uses `WinitSettings::continuous()` so frame pacing
is less dependent on whether the window is focused.

### 6. Prep Time

Prep cost is no longer tracked by a Criterion bench: the `glyph_rasterization`
bench and the prep API it called (`build_slug_run_render_data` non-clip plus
`Backend::glyph_cache()`) were both removed during the slug migration. The
last recorded figure is full printable ASCII ≈ 0.84 ms (after per-curve dedup
and 48-band tuning) — below one frame and below frame-timing resolution, so
there is no warm-up cost worth a per-variant column. Leave the table's
`Prep time` row at that recorded figure unless a change is expected to move
prep cost, in which case rebuild a micro-benchmark against
`Backend::prepare_positioned_run_with_scale` + `ensure_run_storage` and
record the case name.

### 7. Populate The Table

1. Add the new column to the comparison table.
2. Set `Delta` to `(newest configuration) - (current accepted candidate)`.
3. Use signed deltas:
   - negative timings are better
   - positive timings are worse
4. Keep `Fragment per frame` as the primary decision signal. Use
   vertex, V+F total, frame, and prep deltas as supporting evidence.
5. Add a short verdict below the table: kept, rejected, or still active.

## Scope And Comparability Rules

- All columns must come from the **same scene** (currently the
  720-instance `text_renderer_gpu_bench` Slug run). Variants measured
  in a different scene cannot be added to this table.
- All columns must be measured with the bench window on the **built-in
  MacBook display** (monitor 0, scale factor 2.0). The bench pins
  `WindowPosition::Centered(MonitorSelection::Primary)`; on macOS that is
  the main display, which is the built-in unless System Settings
  designates an external as main. Confirm the drawable is `3200x1800`
  (1600×900 logical × scale 2.0) before keeping a trace — a `1600x900`
  drawable means the window landed on a scale-1.0 external and the trace
  is invalid.
- Every column must declare its `TextAntiAlias` mode (resource default:
  `Both`). Columns with different AA modes are different configurations,
  not deltas of one another.
- Every column must declare the warmup protocol actually run
  (`WARMUP_FRAMES`/`SAMPLE_FRAMES` and the xctrace time limit), backed by
  the exported trace bundles. The 2026-05 "long-warmup" numbers were
  recorded without surviving trace bundles (see the Newton-deflation
  entry in `slug-experiments.md`), so protocol claims without trace
  evidence don't count.
- After an engine upgrade, verify the parser's label filter
  (`main_transparent_pass_3d`) still matches the exported intervals
  before trusting any trace — pass labels can change between bevy
  releases.

## Verdict

After the table, every entry must close with one short paragraph that
states the verdict for the newest column and the reason:

- Kept / Rejected / Still active
- One or two sentences citing the `Fragment per frame` delta as the
  primary signal, with vertex / V+F total / frame / prep deltas as
  secondary evidence.

_No entries under the new baseline yet. The joined-ribbon, ribbon,
and pre-2026-05-23 entries lived under the old per-interval
methodology and are not comparable to the new baseline;

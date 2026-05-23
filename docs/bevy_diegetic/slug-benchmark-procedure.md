# Slug Benchmark Procedure

This document is the **single canonical procedure** for measuring and
recording Slug shader benchmark comparisons. It defines the comparison
table, the steps to populate a new column, and the tooling used.
Older empty templates and duplicate tables in other Slug documents
have been removed; new benchmark entries must use the table below and
link back here.

## Comparison Table

One row per metric, one column per configuration, plus `Delta` and
`Meaning`. Delta is always `(newest configuration) − 96 Bands baseline`.

| Metric | 96 Bands (baseline, 2026-05-23) | Delta | Meaning |
| --- | ---: | ---: | --- |
| Vertex per frame            | `0.0549 ms` | — | GPU vertex work, summed per frame. |
| Fragment per frame          | `3.1724 ms` | — | GPU fragment work, summed per frame (Slug analytic + OIT sub-pass). |
| Vertex + fragment per frame | `3.2274 ms` | — | Total GPU transparent-pass work per frame. |
| Bevy frame time             | `8.5586 ms` | — | Whole-app frame pacing. Heavy stddev (~4 ms); sanity check only. |
| Prep time                   | `1.1703 ms` | — | One-time Slug prep (Criterion `jbm_ascii_128_slug`). Carried forward from pre-parser-fix measurement; unchanged by shader-only experiments. |

The baseline is the **96-band Slug candidate** measured on AC with the
current `parse_gpu_intervals.py`. The previous procedure-doc table is
retired: it aggregated fragment costs per-interval rather than
per-frame, so its fragment numbers were too low by ~0.5 ms relative to
per-frame totals — vertex and fragment did not sum to the V+F total
column. New columns added to the right of this baseline, with `Delta`
set to `(new column) − (96 Bands baseline)`.

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

2. Record the Slug trace for the canonical 720-instance scene. The script
   builds `text_renderer_gpu_bench` in release mode, starts it with
   `--mode slug --instances 720 --warmup-frames 180 --sample-frames 240`,
   records a Metal System Trace, and writes stdout to
   `target/xctrace/text-renderer-slug.stdout.log`.

   ```bash
   bash scripts/xctrace_text_renderer.sh record slug 15s 720
   ```

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

### 6. Prep-Time Benchmark

Run the `renderer_prep` Criterion group for the same implementation:

```bash
cargo bench -p bevy_diegetic --bench glyph_rasterization renderer_prep
```

Use the `jbm_ascii_128_slug` result for the table's `Prep time` row
unless the experiment explicitly changes the canonical prep case. Record
the exact case name if a different prep case is used.

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
- Spike-only changes (anything that lives only in
  `crates/bevy_diegetic/src/slug_text_spike` and is not wired into the
  production text renderer used by `text_renderer_gpu_bench`) cannot
  be compared here until the spike change is wired into the
  benchmark's render path. Record such pre-integration runs separately
  in `slug-experiments.md` and clearly mark them as scene-limited.

## Verdict

After the table, every entry must close with one short paragraph that
states the verdict for the newest column and the reason:

- Kept / Rejected / Still active
- One or two sentences citing the `Fragment per frame` delta as the
  primary signal, with vertex / V+F total / frame / prep deltas as
  secondary evidence.

_No entries under the new baseline yet. The joined-ribbon, ribbon,
and pre-2026-05-23 entries lived under the old per-interval
methodology and are not comparable to the new baseline; see
`slug-experiments.md` for the full historical trail and individual
experiment records._

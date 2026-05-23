# Slug Benchmark Procedure

This document is the **single canonical procedure** for measuring and
recording Slug shader benchmark comparisons. It defines the comparison
table, the steps to populate a new column, and the tooling used.
Older empty templates and duplicate tables in other Slug documents
have been removed; new benchmark entries must use the table below and
link back here.

## Comparison Table

One row per metric, one column per configuration, plus `Delta` and
`Meaning`. Delta is always `(newest configuration) − (current accepted
candidate)`; for the current row of the table, that is the per-segment
ribbon column minus the 96-band column.

| Metric | Reconstructed `dda5299` | Current 96 Bands | Per-segment ribbon | Joined ribbon (best) | Delta | Meaning |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Vertex mean             | `0.0488 ms` | `0.0599 ms` | `1.1183 ms` | `0.6431 ms` | `+0.5832 ms` | GPU vertex work for the traced transparent pass. |
| Fragment mean           | `2.9079 ms` | `2.5759 ms` | `3.0421 ms` | `2.3381 ms` | `-0.2378 ms` | Main Slug shader pixel cost. |
| Vertex + fragment total | `4.2956 ms` | `3.7655 ms` | `5.8037 ms` | `5.3305 ms` | `+1.5650 ms` | GPU transparent-pass work. |
| Bevy frame time         | `10.2161 ms` | `10.9424 ms` | `10.6648 ms` | `11.3125 ms` | `+0.3701 ms` | Whole app frame pacing, including non-text work and scheduling. |
| Prep time               | `1.0761 ms` | `1.1703 ms` | `2.9226 ms` | `1.6981 ms` | `+0.5278 ms` | One-time Slug prep for the scene. |

`Joined ribbon (best)` is the joined mitered tri-strip with
`half_width = 2.0`, `FLATTEN_STEPS = 6` — the final iteration of the
ribbon experiment, after per-segment rectangles, joined mitered joints
at `half = 5`, and narrowing to `half = 3` (see
`slug-experiments.md` for the iteration trail).

`Reconstructed dda5299` is the first-Slug-benchmark baseline rebuilt
under today's measurement setup (see
`docs/bevy_diegetic/slug-experiments.md` for how that reconstruction
was produced). `Current 96 Bands` is the candidate adopted in the
`### Global Band-Density Retest` section of the same document.

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

and prints, one `key=value` per line:

- `vertex_mean_ms` → `Vertex mean` cell.
- `fragment_mean_ms` → `Fragment mean` cell.
- `vertex_plus_fragment_total_mean_ms` → `Vertex + fragment total` cell.
  This is the mean of per-frame vertex + fragment durations: rows are
  grouped by frame index when xctrace exports one, otherwise vertex
  and fragment rows are paired in document order.

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
4. Keep `Fragment mean` as the primary decision signal. Use vertex,
   total, frame, and prep deltas as supporting evidence.
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
- One or two sentences citing the fragment-mean delta as the primary
  signal, with vertex / total / frame / prep deltas as secondary
  evidence.

### Joined ribbon, best iteration (2026-05-23)

**Rejected, but informative.** After four iterations the joined mitered
strip at `half = 2`, `FLATTEN_STEPS = 6` reached `fragment_mean =
2.3381 ms` — a real `-0.24 ms` (`-9 %`) win over the 96-band candidate
on the per-row fragment column. Vertex still loses by `+0.58 ms`
(`~11×`) and the per-frame V+F total loses by `+1.57 ms`, so the
overall GPU transparent-pass work remains higher than 96 bands.
Frame delta is vsync-bounded noise. Visual parity at the canonical
home view is preserved (`10381 / 7,271,424` pixels differ, `0.143 %`).
Prep `+0.53 ms` (`~1.5×`) is acceptable in the broader context (SDF
atlas prep is `50-70 ms`), but the GPU-side loss is the deciding
signal. Methodology note: this column was extracted with
`scripts/parse_gpu_intervals.py`; the `dda5299` and `96 Bands`
columns predate that parser and may use a slightly different
aggregation — the ribbon vs 96 Bands per-row deltas should be treated
as directional rather than gold-standard absolute. Even so, the vertex
gap is large enough that the directional conclusion is not at risk.

### Iteration trail

| Variant | Vertex | Fragment | V+F total | Prep | Visual Δ |
| --- | ---: | ---: | ---: | ---: | ---: |
| Forced quad (same-parser ref)    | `0.0634 ms` | `2.1271 ms` | `4.1254 ms` | — | baseline |
| Per-segment rectangles           | `1.1183 ms` | `3.0421 ms` | `5.8037 ms` | `2.9226 ms` | `0.188 %` |
| Joined strip, `half=5, steps=12` | `0.8390 ms` | `2.7390 ms` | `5.6928 ms` | `1.9869 ms` | `0.179 %` |
| Joined strip, `half=3, steps=12` | `0.9176 ms` | `2.7974 ms` | `5.4305 ms` | — | `0.138 %` |
| Joined strip, `half=2, steps=6`  | `0.6431 ms` | `2.3381 ms` | `5.3305 ms` | `1.6981 ms` | `0.143 %` |

Each iteration moved the candidate closer to forced-quad parity on
fragment, but the vertex floor remained `~0.6 ms` because every
polyline joint emits two vertices and a typical glyph contour has
20-40 joints even at `FLATTEN_STEPS = 6`.

### Next direction if pushing further

The ribbon class of approaches has hit a vertex-cost floor that
single-quad approaches sit well below. The next natural class of
experiments is **single-quad geometry plus a shader-side classifier**
that early-outs interior and exterior fragments and only runs the
analytic in a narrow zone around the contour. Candidates, ordered by
prep-cost discipline:

1. **Tighter Slug band classifier.** The 96-band setup already plays
   this role: each fragment looks up its band and exits early when the
   band is empty (full interior or full exterior). Investigate whether
   denser bands along the orthogonal axis, or a 2D band table, cut
   more fragment work without paying atlas-style prep.
2. **Coarse SDF packed into the existing per-glyph Slug storage** (no
   new texture atlas). 8×8 or 16×16 samples per glyph extends the
   existing curve / band buffers by one field; per-fragment lookup is a
   storage-buffer read rather than a sampled texture. Avoids the
   separate atlas pack / upload cost that makes SDF atlases expensive
   in the first place.
3. **Procedural SDF from the polyline at fragment time.** Pass the
   flattened polyline as a storage buffer (the band-index buffer is
   already similar). Per fragment: distance to nearest segment. Cheap
   per-pixel for short polylines, scales with contour vertex count.
   No prep cost beyond the polyline build.

**Avoid**: a separate texture-atlas SDF. Atlas pack + GPU texture
upload + per-add reorganization is exactly the prep regression we
escaped by moving away from SDF atlases — recreating it for a
classifier would give back the prep budget Slug just won.

If none of (1)-(3) clear the 96-band fragment+vertex+total bar at
constant prep, the ribbon thread and its descendants are exhausted
and 96-band Slug is the right answer for this workload.

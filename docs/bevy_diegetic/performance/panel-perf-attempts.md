# Panel layout performance attempts

This document records the panel-layout performance work in this research worktree. It is intentionally a research log, not an integration plan. The goal was to find measured improvements, compare them against Clay, keep public API ergonomics intact, and avoid accepting changes that only looked good in noisy runs.

## Quick result

The accepted stack improves Diegetic's full-rebuild comparison against Clay, but Clay still wins when both systems rebuild everything every frame.

Diegetic's advantage is retained mode: when a panel is kept around and most frames do not change layout, Diegetic can skip most of the work that Clay repeats each frame.

| Scenario | What happens | Result |
|---|---|---|
| Full rebuild every frame | Clay and Diegetic both build and lay out the panel | Clay is still faster |
| No changes in retained mode | Diegetic keeps prior layout state and skips most work | Diegetic is much faster |
| Visual-only retained update | Diegetic reuses geometry and updates commands | Diegetic improved substantially, but retained update overhead remains |

Approximate 500-row comparison after the accepted changes:

| 500-row case | Time |
|---|---:|
| Clay immediate full rebuild | ~163 us |
| Diegetic immediate build + compute | ~194 us |
| Diegetic retained no-change update | ~72 us |

The Clay-facing benchmark improved from roughly `1.33x-1.52x` slower than Clay to roughly `1.19x-1.36x` slower than Clay, depending on row count.

## Accepted changes

These attempts are the keeper stack from this research pass:

| Attempt | Change | Main result |
|---:|---|---|
| 2/3 | Avoid eager wrapped-text slots and remove the parent lookup table by passing parent width during traversal | Small raw compute win, mainly at larger row counts |
| 5 | Skip second DFS visits for elements that cannot emit after-children commands | Strongest general layout win |
| 10 | Avoid no-op text-style scaling when emitting text commands with `font_scale == 1.0` | Improved text-heavy command regeneration and retained visual-only updates |
| 13 | Fuse the initial X and Y fit-size propagation passes | Clear layout-compute win |
| 14 | Skip defensive text remeasurement for tree-classified visual-only replacements | Large retained visual-only win |

## Rejected changes

These were tried, measured, and reverted:

| Attempt | Idea | Why rejected |
|---:|---|---|
| 1 | Store parent links in the tree | Hurt build/cache behavior |
| 4 | Single-grow-child sizing fast path | Mixed signal; wins below threshold |
| 6 | Skip draw-order projection refresh for all visual-only changes | Semantically unsafe |
| 6b | Add a conservative draw-order-stable classifier | Extra classifier pass cost more than it saved |
| 7 | Leaf fast path in positioning/regeneration | Helped some large cases, regressed 100-row regeneration |
| 8 | One-pass `LayoutTree::scaled` | Regressed scale benchmarks |
| 9 | Put identity-scale branch inside `TextStyle::scaled` | Mixed; branch in general helper was not worth it |
| 11 | Measurement-side identity-scale specialization | Regressed compute paths |
| 12 | Reuse retained render-command buffer | Regressed command regeneration |
| 15 | Lazy cached-measurer construction | Mixed; regressed key no-change cases |
| 16 | Cache editable-field count | Did not improve warm/color rebuild targets |
| 17 | Default `LayoutBuilder` preallocation | Too blunt; did not help target paths |
| 18 | Compute draw-order text anchor during enumeration | No measurable win |
| 19 | Classification hotspot diagnostic | Diagnostic only; showed classifier was not dominant enough for a blind Attempt 20 |

Attempt 20 was intentionally unused. The remaining cheap ideas were speculative, and the diagnostic evidence pointed away from another blind implementation attempt.

## Measurement policy

Criterion benchmark results are the source of truth. This machine may have other builds running, so single absolute timings are treated as noisy. The decision process was:

- Compare relative ratios inside the same run first.
- Watch Criterion outlier reports.
- Use unchanged slices as sentinels when possible.
- Rerun apparent wins or suspicious regressions before accepting them.
- Keep diagnostic evidence separate from public benchmark evidence.

Primary Clay-facing benchmark:

```bash
cargo bench -p bevy_diegetic --bench layout_comparison --features bench_support -- --noplot
```

Raw layout-engine diagnostic benchmark:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- --noplot
```

Retained public-panel benchmark:

```bash
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- --noplot
```

## Baseline: Clay-facing layout comparison

Run: 2026-06-15. Other machine activity may have been present.

| Rows | Clay mean | Diegetic mean | Diegetic / Clay | Notes |
| ---: | ---: | ---: | ---: | --- |
| 5 | 2.6673 us | 4.0592 us | 1.52x | Diegetic had high outliers. |
| 20 | 7.2393 us | 10.916 us | 1.51x | Diegetic had high outliers. |
| 100 | 31.591 us | 46.083 us | 1.46x | Both had outliers; Diegetic high outliers were heavy. |
| 500 | 164.22 us | 218.81 us | 1.33x | Low outlier rate in this run. |

Initial read: the gap was largest on small layouts. That points to fixed overhead and allocation/traversal setup, not only per-element layout work.

## Baseline: raw engine diagnostics

Run: 2026-06-15. Same caveat about possible competing machine activity.

| Rows | Build tree | Scale tree | Raw compute | Regenerate commands |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 1.8621 us | 1.7202 us | 2.3284 us | 1.2067 us |
| 20 | 4.4792 us | 5.0876 us | 5.9349 us | 3.5845 us |
| 100 | 17.903 us | 24.333 us | 28.939 us | 16.402 us |
| 500 | 92.655 us | 119.85 us | 143.08 us | 80.138 us |

Diff costs, selected means:

| Rows | Identical | Text color only | Background color only | Late layout change |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 543.82 ns | 506.74 ns | 538.04 ns | 527.96 ns |
| 20 | 1.8166 us | 1.7230 us | 1.8062 us | 1.8228 us |
| 100 | 9.7787 us | 8.8857 us | 9.6937 us | 10.368 us |
| 500 | 46.443 us | 42.946 us | 46.880 us | 46.283 us |

Read: full-tree identical or visual comparisons are row-scaled. That matters for retained panel updates, but it is not the main cost in the direct Clay comparison.

## Attempt 1: stored parent links during text rewrap

Purpose: avoid rebuilding a parent lookup table during text wrapping.

Planned change: add an internal parent index for each element, preserve it through clone/scale, and use it during wrapping.

Result: rejected and reverted.

Why: a separate `Vec<Option<usize>>` helped some compute samples but added build overhead, especially at 500 rows. Storing the parent on `Element` was worse because the larger element hurt cloning and traversal locality.

Lesson: removing the parent lookup is only useful if it does not increase element size or add another growing allocation during build.

## Attempt 2: keep wrapped-text slots empty when nothing wraps

Purpose: avoid allocating one `Option<WrappedText>` slot per element when no text wraps.

Planned change: start with an empty wrapped result, allocate slots only after the first actual wrapped result, and treat missing wrapped slots as `None`.

Filtered command:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support raw_status_panel_100_rows -- --noplot
```

| Slice | Mean |
| --- | ---: |
| `build_tree_only` | 25.380 us |
| `scale_tree_only` | 39.925 us |
| `raw_compute_prebuilt_tree` | 44.596 us |
| `regenerate_commands_only` | 27.309 us |

Result: inconclusive.

Why: absolute numbers were much worse than the original baseline, likely because the machine was busy. Criterion also compared against a polluted rejected-parent-link state.

Panel decision: do not accept from this run. Finish the idea by also removing the parent lookup table without storing parent metadata.

## Attempt 3: parent-aware wrapping traversal

Purpose: complete Attempt 2 by removing the `build_parent_of` allocation/traversal.

Change: walk the tree from the root and pass each child's parent content width down the traversal. Allocate wrapped-text slots only if a text element actually wraps.

### Filtered 100-row raw run

Command:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support raw_status_panel_100_rows -- --noplot
```

| Slice | Mean | Original baseline | Read |
| --- | ---: | ---: | --- |
| `build_tree_only` | 18.050 us | 17.903 us | Sentinel pass; close to baseline. |
| `scale_tree_only` | 24.368 us | 24.333 us | Sentinel pass; close to baseline. |
| `raw_compute_prebuilt_tree` | 28.510 us | 28.939 us | Small possible improvement, about 1.5%. Needs broader rows. |
| `regenerate_commands_only` | 15.656 us | 16.402 us | Possible improvement, but this slice includes clone cost in the benchmark. |

### Full raw run

Command:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- --noplot
```

| Rows | Build tree | Scale tree | Raw compute | Original raw compute | Read |
| ---: | ---: | ---: | ---: | ---: | --- |
| 5 | 1.6597 us | 1.7014 us | 2.1849 us | 2.3284 us | Tiny layouts do not show a decision-grade win. |
| 20 | 4.4897 us | 5.0955 us | 5.9384 us | 5.9349 us | Flat. |
| 100 | 17.630 us | 24.288 us | 28.054 us | 28.939 us | About 3.1% faster. |
| 500 | 78.983 us | 119.44 us | 137.34 us | 143.08 us | About 4.0% faster. |

Panel decision: provisionally keep. The shape was right: neutral at small row counts, better at 100/500, no build/scale regression.

### Clay-facing confirmation run

Command:

```bash
cargo bench -p bevy_diegetic --bench layout_comparison --features bench_support -- --noplot
```

| Rows | Clay mean | Diegetic mean | Diegetic / Clay | Baseline ratio | Read |
| ---: | ---: | ---: | ---: | ---: | --- |
| 5 | 2.6888 us | 3.9654 us | 1.47x | 1.52x | Small improvement. |
| 20 | 7.2814 us | 11.074 us | 1.52x | 1.51x | Neutral to slightly worse. |
| 100 | 31.865 us | 47.847 us | 1.50x | 1.46x | Not confirmed. |
| 500 | 168.89 us | 218.74 us | 1.30x | 1.33x | Ratio improved mostly because Clay moved. |

Rerun:

```bash
cargo bench -p bevy_diegetic --bench layout_comparison --features bench_support -- --noplot
```

| Rows | Clay mean | Diegetic mean | Diegetic / Clay | Baseline ratio | Read |
| ---: | ---: | ---: | ---: | ---: | --- |
| 5 | 2.6330 us | 3.9159 us | 1.49x | 1.52x | Small-row ratio improved. |
| 20 | 7.3564 us | 10.642 us | 1.45x | 1.51x | Improved. |
| 100 | 32.046 us | 48.008 us | 1.50x | 1.46x | Public 100-row slowdown reproduced. |
| 500 | 165.78 us | 218.72 us | 1.32x | 1.33x | Neutral. |

Problem: raw compute improved, but the 100-row public comparison looked worse. The next step was to isolate the benchmark-path mismatch.

### Diagnostic: unscaled Diegetic slices

Purpose: make the raw benchmark match the Diegetic side of `layout_comparison`, which uses an unscaled tree, `PANEL_SIZE`, and `font_scale = 1.0`.

Added diagnostic slices:

- `raw_compute_prebuilt_unscaled_tree`
- `build_compute_unscaled_tree`

Command:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support raw_status_panel_100_rows -- --noplot
```

| Slice | Mean | Read |
| --- | ---: | --- |
| `build_tree_only` | 17.541 us | Build sentinel valid. |
| `scale_tree_only` | 25.257 us | Slightly high; not part of unscaled direct comparison. |
| `raw_compute_prebuilt_tree` | 28.037 us | Scaled prebuilt compute remains near the improved raw result. |
| `raw_compute_prebuilt_unscaled_tree` | 28.205 us | Unscaled prebuilt compute is also fast. |
| `build_compute_unscaled_tree` | 45.717 us | Faster than the original 46.083 us public baseline and below the noisy 48.008 us rerun. |

Decision: accept Attempt 3. The public 100-row slowdown was likely session noise or benchmark-group interaction, not a library regression.

## Attempt 4: single-grow-child sizing fast path

Purpose: optimize rows with one grow child between fit text leaves.

Change: if there is exactly one grow child and positive remaining space, assign the available growth directly instead of using the general smallest-first expansion loop.

### First filtered run

Command:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support raw_status_panel_100_rows -- --noplot
```

| Slice | Mean | Previous Attempt 3 diagnostic | Read |
| --- | ---: | ---: | --- |
| `build_tree_only` | 23.504 us | 17.541 us | Invalid sentinel; this attempt cannot affect tree construction. |
| `scale_tree_only` | 24.308 us | 25.257 us | Fine, but not the target. |
| `raw_compute_prebuilt_tree` | 27.403 us | 28.037 us | About 2.3% faster. |
| `raw_compute_prebuilt_unscaled_tree` | 27.760 us | 28.205 us | About 1.6% faster. |
| `build_compute_unscaled_tree` | 45.238 us | 45.717 us | About 1.0% faster despite noisy build-only sentinel. |

Result: not accepted. Build sentinel invalidated the run.

### Clean rerun

Command:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support raw_status_panel_100_rows -- --noplot
```

| Slice | Mean | Attempt 3 diagnostic | Read |
| --- | ---: | ---: | --- |
| `build_tree_only` | 17.534 us | 17.541 us | Sentinel valid. |
| `scale_tree_only` | 24.540 us | 25.257 us | Sentinel acceptable. |
| `raw_compute_prebuilt_tree` | 28.523 us | 28.037 us | Worse than Attempt 3. |
| `raw_compute_prebuilt_unscaled_tree` | 28.059 us | 28.205 us | Slightly better, below 1%. |
| `build_compute_unscaled_tree` | 45.400 us | 45.717 us | Slightly better, below 1%. |

Decision: reject and revert. The clean run was mixed, the scaled path regressed, and the wins were below the threshold for adding more hot-loop branching.

## Attempt 5: skip no-op DFS up visits

Purpose: avoid scheduling a second DFS visit for elements that cannot emit after-children commands.

Change: add `needs_up_traversal(element)`. Only push the upward visit when the element has a border, a non-zero child divider, or clipped overflow.

Correctness caveat: the predicate must stay exactly aligned with `emit_up_traversal_commands`, especially for clipped overflow scissor start/end balance.

### Filtered raw run

Command:

```bash
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support raw_status_panel_100_rows -- --noplot
```

| Slice | Mean | Attempt 3 diagnostic | Read |
| --- | ---: | ---: | --- |
| `build_tree_only` | 17.491 us | 17.541 us | Sentinel valid. |
| `scale_tree_only` | 24.991 us | 25.257 us | Slightly high/noisy but unrelated. |
| `raw_compute_prebuilt_tree` | 25.790 us | 28.037 us | About 8.0% faster. |
| `raw_compute_prebuilt_unscaled_tree` | 26.394 us | 28.205 us | About 6.4% faster. |
| `build_compute_unscaled_tree` | 45.035 us | 45.717 us | About 1.5% faster despite build cost dilution. |
| `regenerate_commands_only` | 13.870 us | 15.660 us | About 11.4% faster. |

Panel decision: strong keep candidate. Broaden to all row counts.

### Full raw matrix

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- --noplot
```

| Rows | build | scale | compute prebuilt scaled | compute prebuilt unscaled | build + compute unscaled | regenerate commands |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 1.6420 us | 1.6607 us | 1.9392 us | 2.0527 us | 3.8255 us | 1.0196 us |
| 20 | 4.4358 us | 5.1810 us | 5.2587 us | 5.2765 us | 10.555 us | 2.9431 us |
| 100 | 17.591 us | 24.181 us | 25.842 us | 26.938 us | 43.823 us | 14.078 us |
| 500 | 79.729 us | 122.76 us | 127.60 us | 127.29 us | 207.71 us | 69.896 us |

Comparison to accepted Attempt 3 raw baseline:

| Rows | Attempt 3 compute scaled | Attempt 5 compute scaled | Delta | Attempt 3 regenerate | Attempt 5 regenerate | Delta |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 28.037 us | 25.842 us | -7.8% | 15.660 us | 14.078 us | -10.1% |
| 500 | 137.34 us | 127.60 us | -7.1% | 74.68 us | 69.896 us | -6.4% |

Panel decision: provisionally keep. The raw improvement is large, localized to the touched paths, and not explained by build/scale sentinels.

### Clay-facing guardrail

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_comparison --features bench_support -- --noplot
```

| Rows | Clay | Diegetic | Diegetic / Clay |
|---:|---:|---:|---:|
| 5 | 2.6306 us | 3.7445 us | 1.42x |
| 20 | 7.4810 us | 10.269 us | 1.37x |
| 100 | 31.694 us | 45.553 us | 1.44x |
| 500 | 165.14 us | 202.69 us | 1.23x |

Comparison to initial baseline ratios:

| Rows | Initial ratio | Attempt 5 ratio | Direction |
|---:|---:|---:|---|
| 5 | 1.52x | 1.42x | better |
| 20 | 1.51x | 1.37x | better |
| 100 | 1.46x | 1.44x | slightly better/noisy |
| 500 | 1.33x | 1.23x | better |

Panel decision: passed the Clay-facing guardrail. Move to retained/public-panel benchmark.

### Retained public guardrail

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- --noplot
```

Criterion did not emit prior-run change lines for these groups, so this run is a retained-path snapshot, not a quantified before/after delta.

| Rows | cold | no change | resize only | warm | color change full rebuild | visual only rebuild |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 209.83 us | 74.901 us | 84.432 us | 80.767 us | 80.738 us | 90.535 us |
| 20 | 215.93 us | 86.528 us | 88.614 us | 98.810 us | 96.529 us | 103.47 us |
| 100 | 276.03 us | 72.990 us | 118.50 us | 169.22 us | 172.53 us | 164.61 us |
| 500 | 532.71 us | 74.146 us | 271.39 us | 483.05 us | 514.11 us | 475.16 us |

Panel decision: accept Attempt 5. The retained snapshot did not contradict the stronger raw and Clay-facing evidence. Correctness coverage is still required before integration for borders, dividers, clipped overflow, nested clipping, and combinations with text/shapes.

## Attempt 6: skip draw-order projection refresh for visual-only reuse

Purpose: estimate the upper bound of avoiding draw-order projection rebuilds during retained visual-only updates.

Experimental change: skip `ComputedDiegeticPanel::refresh_draw_order_projection()` in the geometry-stable `VisualOnly` branch.

Correctness status: intentionally unsafe as a general optimization. Some visual-only edits can change z-index, command presence, or command ordering.

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- visual_only_rebuild --noplot
```

Compile note: the patch made `ComputedDiegeticPanel::refresh_draw_order_projection` unused, producing a dead-code warning.

| Rows | Attempt 5 snapshot | Attempt 6 point estimate | Criterion verdict |
|---:|---:|---:|---|
| 5 | 90.535 us | 85.969 us | no change detected |
| 20 | 103.47 us | 103.82 us | improved by Criterion baseline comparison |
| 100 | 164.61 us | 172.89 us | no change detected; noisy/worse point estimate |
| 500 | 475.16 us | 457.80 us | improved, about -5.7% by Criterion |

Decision: reject as implemented. The 500-row result shows projection refresh can matter, but the optimization is not semantically safe.

## Attempt 6b: conservative draw-order-stable classifier

Purpose: keep Attempt 6's large-panel benefit safely by skipping projection refresh only when command topology and draw-order keys are stable.

Implementation sketch:

- Keep public `commands().set_tree(entity, tree)` unchanged.
- Add an internal `draw_order_stable` bit to pending panel change classification.
- Compute it with a conservative `LayoutTree::draw_order_stable_change` comparison.
- Treat text visual style payload changes and image payload changes as stable only when structure, command presence, child order, z-index, clipping, draw commands, and child layout are unchanged.
- Refresh draw-order projection for all other visual-only changes.

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- visual_only_rebuild --noplot
```

Compile note: the patch left `DiegeticPanelChangeClassification::record` and `take` unused, producing dead-code warnings.

| Rows | Attempt 5 snapshot | Attempt 6b point estimate | Criterion verdict |
|---:|---:|---:|---|
| 5 | 90.535 us | 90.886 us | no change detected |
| 20 | 103.47 us | 114.65 us | regressed |
| 100 | 164.61 us | 177.36 us | regressed |
| 500 | 475.16 us | 502.74 us | regressed |

Decision: reject and revert. The safe second comparison costs more than projection reuse saves. This idea should only be revisited if draw-order stability can be fused into the existing `classify_change` pass.

## Attempt 7: leaf fast path in positioning/render regeneration

Purpose: after Attempt 5, avoid per-leaf child clip setup and child stack processing for text leaves that have no children and no up-pass commands.

Change: in `position_and_render` and `render_commands_from_geometry`, skip child setup when an element has no children. Still schedule an up visit for clipped or bordered leaves.

### Full raw run

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- --noplot
```

Run caveat: Cargo initially waited on the package-cache lock, and unrelated diff-only sentinels moved heavily. Treat this run as noisy.

| Rows | build | scale | compute scaled | compute unscaled | build + compute unscaled | regenerate |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 1.6904 us | 1.6931 us | 2.0048 us | 1.9601 us | 3.8361 us | 0.98068 us |
| 20 | 4.5597 us | 5.0374 us | 5.2940 us | 5.4533 us | 10.490 us | 2.9464 us |
| 100 | 17.929 us | 27.686 us | 25.210 us | 26.515 us | 43.884 us | 13.681 us |
| 500 | 79.712 us | 119.85 us | 128.67 us | 127.85 us | 205.68 us | 69.490 us |

Comparison to accepted Attempt 5 raw point estimates:

| Rows | Attempt 5 compute scaled | Attempt 7 compute scaled | Direction | Attempt 5 regenerate | Attempt 7 regenerate | Direction |
|---:|---:|---:|---|---:|---:|---|
| 100 | 25.842 us | 25.210 us | better, about -2.4% | 14.078 us | 13.681 us | better, about -2.8% |
| 500 | 127.60 us | 128.67 us | flat/slightly worse | 69.896 us | 69.490 us | flat/noise |

Panel decision: do not accept from this noisy run. Allow one focused rerun.

### First focused rerun discarded

A focused `raw_status_panel_` rerun was stopped early because untouched 20-row slices regressed by extreme amounts, including `regenerate_commands_only` moving by roughly +168%. Those samples were not used.

### Second focused rerun

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- raw_status_panel_ --noplot
```

| Rows | build | scale | compute scaled | compute unscaled | build + compute unscaled | regenerate |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 1.6617 us | 1.6740 us | 1.9485 us | 1.9391 us | 3.7886 us | 0.98578 us |
| 20 | 4.6289 us | 5.0334 us | 5.3649 us | 5.2799 us | 10.248 us | 2.9872 us |
| 100 | 17.386 us | 24.289 us | 25.531 us | 25.454 us | 43.461 us | 15.829 us |
| 500 | 80.296 us | 119.44 us | 123.90 us | 124.91 us | 201.91 us | 67.079 us |

Comparison to accepted Attempt 5 raw point estimates:

| Rows | Attempt 5 compute scaled | Attempt 7 rerun compute scaled | Direction | Attempt 5 regenerate | Attempt 7 rerun regenerate | Direction |
|---:|---:|---:|---|---:|---:|---|
| 100 | 25.842 us | 25.531 us | slightly better | 14.078 us | 15.829 us | worse |
| 500 | 127.60 us | 123.90 us | better, about -2.9% | 69.896 us | 67.079 us | better, about -4.0% |

Decision: reject and revert. The 500-row win was real-looking, but 100-row command regeneration regressed, which maps directly to retained visual-update risk.

## Attempt 8: one-pass `LayoutTree::scaled`

Purpose: reduce scaling cost by cloning and scaling each element in one push loop instead of cloning the whole tree and then mutating it.

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- scale_tree_only --noplot
```

| Rows | Attempt 5 scale | Attempt 8 scale | Criterion verdict |
|---:|---:|---:|---|
| 5 | 1.6607 us | 1.7999 us | regressed |
| 20 | 5.1810 us | 5.7335 us | regressed |
| 100 | 24.181 us | 25.095 us | regressed |
| 500 | 122.76 us | 122.49 us | noise-level/flat |

Decision: reject and revert. `Vec<Element>::clone` plus in-place mutation is already efficient enough; the manual push loop was slower.

## Attempt 9: skip no-op text-style scaling in `TextStyle::scaled`

Purpose: avoid cloning and multiplying text-style fields by `1.0` when `font_scale == 1.0`.

Change: add the identity branch inside the general `TextStyle::scaled` helper.

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- raw_status_panel_ --noplot
```

| Rows | build | scale | compute scaled | compute unscaled | build + compute unscaled | regenerate |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 1.6204 us | 1.6719 us | 1.9879 us | 1.9666 us | 3.6885 us | 1.0095 us |
| 20 | 4.4583 us | 4.9961 us | 5.5514 us | 5.3076 us | 10.229 us | 3.1137 us |
| 100 | 17.524 us | 24.171 us | 26.090 us | 25.624 us | 44.758 us | 14.271 us |
| 500 | 79.167 us | 115.24 us | 127.06 us | 127.03 us | 203.88 us | 65.553 us |

Comparison to accepted Attempt 5 point estimates:

| Rows | Attempt 5 compute scaled | Attempt 9 compute scaled | Direction | Attempt 5 regenerate | Attempt 9 regenerate | Direction |
|---:|---:|---:|---|---:|---:|---|
| 100 | 25.842 us | 26.090 us | worse/flat | 14.078 us | 14.271 us | worse/flat |
| 500 | 127.60 us | 127.06 us | flat | 69.896 us | 65.553 us | better, about -6.2% |

Decision: reject and revert. The 500-row regeneration win was interesting, but putting a branch in the general helper hurt or failed to help the main compute paths. The better follow-up was to specialize at the render-command call site.

## Attempt 10: render-command-only identity text scale

Purpose: keep the good part of Attempt 9 while avoiding a branch in the general text-style helper.

Change: in text render-command emission, clone the text config directly when `font_scale` is exactly `1.0`; otherwise call `TextStyle::scaled(font_scale)` as before.

### First focused raw run

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- raw_status_panel_ --noplot
```

| Rows | build | scale | compute scaled | compute unscaled | build + compute unscaled | regenerate |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 1.6559 us | 1.6782 us | 1.9395 us | 2.1461 us | 3.6436 us | 0.96145 us |
| 20 | 4.3951 us | 4.9733 us | 5.2572 us | 5.2615 us | 10.026 us | 2.8562 us |
| 100 | 17.261 us | 23.957 us | 25.965 us | 25.700 us | 44.129 us | 13.627 us |
| 500 | 78.244 us | 120.23 us | 123.65 us | 126.62 us | 203.90 us | 68.040 us |

Comparison to accepted Attempt 5 point estimates:

| Rows | Attempt 5 compute scaled | Attempt 10 compute scaled | Direction | Attempt 5 regenerate | Attempt 10 regenerate | Direction |
|---:|---:|---:|---|---:|---:|---|
| 100 | 25.842 us | 25.965 us | flat/slightly worse | 14.078 us | 13.627 us | better, about -3.2% |
| 500 | 127.60 us | 123.65 us | better, about -3.1% | 69.896 us | 68.040 us | better, about -2.7% |

Panel decision: promising candidate, but rerun focused 100/500 slices.

### Focused 100/500 rerun

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- 'raw_status_panel_(100|500)_rows' --noplot
```

| Rows | build | scale | compute scaled | compute unscaled | build + compute unscaled | regenerate |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 17.218 us | 23.832 us | 25.365 us | 25.577 us | 42.920 us | 13.595 us |
| 500 | 80.088 us | 117.58 us | 125.52 us | 129.51 us | 205.69 us | 67.736 us |

Comparison to accepted Attempt 5 point estimates:

| Rows | Attempt 5 compute scaled | Attempt 10 rerun compute scaled | Direction | Attempt 5 build+compute unscaled | Attempt 10 rerun build+compute unscaled | Direction | Attempt 5 regenerate | Attempt 10 rerun regenerate | Direction |
|---:|---:|---:|---|---:|---:|---|---:|---:|---|
| 100 | 25.842 us | 25.365 us | better | 43.823 us | 42.920 us | better | 14.078 us | 13.595 us | better |
| 500 | 127.60 us | 125.52 us | better | 207.71 us | 205.69 us | better | 69.896 us | 67.736 us | better |

Panel decision: passes raw gate. Run retained visual-only, then Clay-facing guardrail.

### Retained visual-only guardrail

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- visual_only_rebuild --noplot
```

| Rows | Attempt 5 snapshot | Attempt 10 point estimate | Criterion verdict |
|---:|---:|---:|---|
| 5 | 90.535 us | 79.488 us | improved |
| 20 | 103.47 us | 100.35 us | improved |
| 100 | 164.61 us | 159.23 us | improved |
| 500 | 475.16 us | 462.27 us | improved |

Panel decision: retained public path confirms the raw win.

### Clay-facing guardrail

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_comparison --features bench_support -- --noplot
```

| Rows | Clay | Diegetic | Ratio | Notes |
|---:|---:|---:|---:|---|
| 5 | 2.6750 us | 3.7115 us | 1.39x | Diegetic improved vs Attempt 5 guardrail 3.7445 us; Criterion called change within noise. |
| 20 | 7.3141 us | 10.365 us | 1.42x | Diegetic slightly slower vs Attempt 5 guardrail 10.269 us, but only +0.8010% within noise. |
| 100 | 31.867 us | 43.206 us | 1.36x | Material point-estimate win vs Attempt 5 guardrail 45.553 us. |
| 500 | 169.69 us | 203.02 us | 1.20x | Essentially flat vs Attempt 5 guardrail 202.69 us; high outliers on both Clay and Diegetic. |

Decision: accept Attempt 10. Public API unchanged. Keep the exact identity-scale invariant documented at the branch.

## Attempt 11: measurement-side identity font-scale specialization

Purpose: mirror Attempt 10 in text measurement setup by avoiding `TextMeasure::scaled(1.0)` in leaf sizing, geometry-reuse checks, and wrapping.

Change:

- Added private `TextStyle::as_measure_scaled(font_scale)`.
- Exact `+1.0` bit comparison returns `as_measure()` unchanged.
- Non-identity scale still uses `TextMeasure::scaled(font_scale)`.

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- 'raw_status_panel_(100|500)_rows' --noplot
```

Manual baseline: accepted Attempt 10 focused raw point estimates.

| Rows | Slice | Attempt 10 | Attempt 11 | Decision |
|---:|---|---:|---:|---|
| 100 | build_tree_only | 17.218 us | 17.976 us | Worse |
| 100 | scale_tree_only | 23.832 us | 24.288 us | Worse |
| 100 | raw_compute_prebuilt_tree | 25.365 us | 25.864 us | Worse |
| 100 | raw_compute_prebuilt_unscaled_tree | 25.577 us | 25.906 us | Worse/noise |
| 100 | build_compute_unscaled_tree | 42.920 us | 43.845 us | Worse |
| 100 | regenerate_commands_only | 13.595 us | 13.783 us | Worse/noise |
| 500 | build_tree_only | 80.088 us | 79.186 us | Slightly better/noise |
| 500 | scale_tree_only | 117.58 us | 121.09 us | Worse |
| 500 | raw_compute_prebuilt_tree | 125.52 us | 131.16 us | Worse |
| 500 | raw_compute_prebuilt_unscaled_tree | 129.51 us | 131.48 us | Worse by point estimate |
| 500 | build_compute_unscaled_tree | 205.69 us | 211.63 us | Worse |
| 500 | regenerate_commands_only | 67.736 us | 66.134 us | Better |

Decision: reject and revert. It regressed the compute paths it targeted. The 500-row regeneration blip did not match the attempted mechanism and was not pursued.

## Attempt 12: reuse retained render-command buffer

Purpose: preserve `Vec<RenderCommand>` capacity during retained command regeneration instead of assigning a freshly returned vector.

Change:

- Added private `positioning::render_commands_from_geometry_into(...)` to clear and refill a caller-provided vector.
- Kept the old returning wrapper for fresh callers.
- Routed `LayoutResult::regenerate_commands` through the in-place helper.

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- 'raw_status_panel_(100|500)_rows' --noplot
```

Manual baseline: accepted Attempt 10 focused raw point estimates.

| Rows | Slice | Attempt 10 | Attempt 12 | Decision |
|---:|---|---:|---:|---|
| 100 | build_tree_only | 17.218 us | 17.541 us | Slightly worse/noise |
| 100 | scale_tree_only | 23.832 us | 24.163 us | Slightly worse/noise |
| 100 | raw_compute_prebuilt_tree | 25.365 us | 25.491 us | Neutral/slightly worse |
| 100 | raw_compute_prebuilt_unscaled_tree | 25.577 us | 26.112 us | Worse/noise |
| 100 | build_compute_unscaled_tree | 42.920 us | 43.496 us | Worse/noise |
| 100 | regenerate_commands_only | 13.595 us | 14.561 us | Worse |
| 500 | build_tree_only | 80.088 us | 79.418 us | Slightly better/noise |
| 500 | scale_tree_only | 117.58 us | 117.45 us | Neutral |
| 500 | raw_compute_prebuilt_tree | 125.52 us | 128.41 us | Worse |
| 500 | raw_compute_prebuilt_unscaled_tree | 129.51 us | 125.31 us | Better/noise |
| 500 | build_compute_unscaled_tree | 205.69 us | 207.34 us | Worse/noise |
| 500 | regenerate_commands_only | 67.736 us | 69.142 us | Worse |

Decision: reject and revert. The primary target, `regenerate_commands_only`, regressed at both 100 and 500 rows. Fresh allocation appears cheaper or better-localized than clearing the retained buffer for this command stream.

## Attempt 13: fuse initial X/Y fit-size propagation

Purpose: remove one initial bottom-up traversal. Before wrapping, X and Y fit-size propagation are independent, so they can be computed in one pass. The existing Y-only repropagation after wrapping must remain separate.

Change:

- Added private `sizing::propagate_fit_sizes_xy(...)`.
- Replaced the initial separate X and Y calls in `LayoutEngine::compute`.
- Kept existing single-axis `propagate_fit_sizes(...)` for wrap-corrected Y repropagation.

Correctness caveat: fused pre-wrap X/Y propagation must remain equivalent to running X then Y separately. Layout parity tests are required before integration.

### Focused raw result

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- 'raw_status_panel_(100|500)_rows' --noplot
```

Manual baseline: accepted Attempt 10 focused raw point estimates.

| Rows | Slice | Attempt 10 | Attempt 13 | Decision |
|---:|---|---:|---:|---|
| 100 | build_tree_only | 17.218 us | 17.579 us | Worse/noise |
| 100 | scale_tree_only | 23.832 us | 25.141 us | Worse/noise |
| 100 | raw_compute_prebuilt_tree | 25.365 us | 23.631 us | Better |
| 100 | raw_compute_prebuilt_unscaled_tree | 25.577 us | 24.375 us | Better |
| 100 | build_compute_unscaled_tree | 42.920 us | 41.958 us | Better |
| 100 | regenerate_commands_only | 13.595 us | 13.705 us | Neutral/slightly worse |
| 500 | build_tree_only | 80.088 us | 79.326 us | Slightly better/noise |
| 500 | scale_tree_only | 117.58 us | 117.94 us | Neutral/noise |
| 500 | raw_compute_prebuilt_tree | 125.52 us | 117.83 us | Better |
| 500 | raw_compute_prebuilt_unscaled_tree | 129.51 us | 117.79 us | Better |
| 500 | build_compute_unscaled_tree | 205.69 us | 199.49 us | Better |
| 500 | regenerate_commands_only | 67.736 us | 70.045 us | Worse |

Panel decision: keep candidate. Compute-path win is large and coherent; watch unrelated command-regeneration caveat.

### Clay-facing guardrail

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_comparison --features bench_support -- --noplot
```

Manual baseline: accepted Attempt 10 `layout_comparison` point estimates.

| Rows | Attempt 10 Clay | Attempt 10 Diegetic | Attempt 13 Clay | Attempt 13 Diegetic | Ratio | Decision |
|---:|---:|---:|---:|---:|---:|---|
| 5 | 2.6750 us | 3.7115 us | 2.6285 us | 3.5845 us | 1.36x | Better |
| 20 | 7.3141 us | 10.365 us | 7.7963 us | 9.7040 us | 1.24x | Better Diegetic; Clay noisy/slower |
| 100 | 31.867 us | 43.206 us | 31.440 us | 44.503 us | 1.42x | Worse by point estimate; 30% outliers |
| 500 | 169.69 us | 203.02 us | 162.89 us | 194.12 us | 1.19x | Better |

Panel decision: 100-row public result was noisy and contradicted raw evidence. Rerun that slice.

### Focused 100-row public retest

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_comparison --features bench_support -- status_panel_100_rows --noplot
```

| Rows | Attempt 10 Clay | Attempt 10 Diegetic | Attempt 13 retest Clay | Attempt 13 retest Diegetic | Ratio | Decision |
|---:|---:|---:|---:|---:|---:|---|
| 100 | 31.867 us | 43.206 us | 31.152 us | 40.891 us | 1.31x | Better |

Decision: accept Attempt 13. The focused retest resolved the noisy 100-row contradiction.

### Retained visual-only guardrail

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- visual_only_rebuild --noplot
```

Manual baseline: accepted Attempt 10 retained visual-only snapshot.

| Rows | Attempt 10 | Attempt 13 | Decision |
|---:|---:|---:|---|
| 5 | 79.488 us | 85.612 us | Worse |
| 20 | 100.35 us | 93.582 us | Better |
| 100 | 159.23 us | 162.51 us | Near-flat/slightly worse |
| 500 | 462.27 us | 452.73 us | Better |

Panel decision: non-blocking mixed retained result. Keep Attempt 13 accepted. Remaining retained cost likely lives downstream of layout math.

## Attempt 14: skip geometry reuse re-measure for tree-classified visual-only changes

Purpose: improve retained visual-only tree replacements. `set_tree` already classifies old vs new trees. If it returns `VisualOnly`, layout-affecting properties and text content are unchanged, so the later geometry reuse check should not need to remeasure text.

Change:

- Added internal `tree_visual_geometry_stable` flag to `DiegeticPanelChangeClassification`.
- `set_tree_command` marks tree-classified `VisualOnly` replacements as geometry-stable.
- Runtime text edits still record `VisualOnly` but clear the geometry-stable flag.
- `compute_panel_layouts` skips `can_reuse_geometry` only for tree-classified geometry-stable replacements.

Correctness caveat: text-edit safety is the integration gate. Same-frame text edits must clear the geometry-stable flag before reuse is considered.

### Primary visual-only result

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- visual_only_rebuild --noplot
```

Manual baseline: accepted Attempt 13 retained visual-only guardrail.

| Rows | Attempt 13 | Attempt 14 | Decision |
|---:|---:|---:|---|
| 5 | 85.612 us | 87.089 us | Slightly worse/noise |
| 20 | 93.582 us | 95.706 us | Worse |
| 100 | 162.51 us | 145.78 us | Better |
| 500 | 452.73 us | 381.57 us | Better |

Panel decision: strong large-row candidate, but rerun because small rows regressed.

### Visual-only confirmation rerun

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- visual_only_rebuild --noplot
```

| Rows | Attempt 13 | Attempt 14 confirmation | Decision |
|---:|---:|---:|---|
| 5 | 85.612 us | 85.397 us | Flat/slightly better |
| 20 | 93.582 us | 90.699 us | Better |
| 100 | 162.51 us | 149.27 us | Better |
| 500 | 452.73 us | 376.21 us | Better |

Panel decision: accept as a retained visual-only keeper, pending broad retained guardrails.

### Broad retained guardrail

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- --noplot
```

| Rows | Cold | No change | Resize | Warm | Color full | Visual only |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 195.24 us | 71.591 us | 82.748 us | 80.135 us | 81.260 us | 83.686 us |
| 20 | 198.78 us | 78.962 us | 85.144 us | 97.956 us | 98.268 us | 90.271 us |
| 100 | 259.45 us | 72.590 us | 116.59 us | 181.22 us | 183.10 us | 145.40 us |
| 500 | 500.33 us | 82.897 us | 279.05 us | 548.44 us | 574.79 us | 377.51 us |

Interpretation: target visual-only remained favorable. Some adjacent 500-row retained paths looked worse in this broad run, even though most should not exercise the new branch. Focused retest required.

### Focused 500-row retained retest

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- panel_500_rows --noplot
```

| Slice | Broad run | Focused retest | Decision |
|---|---:|---:|---|
| cold | 500.33 us | 495.30 us | Better |
| no_change_update | 82.897 us | 72.120 us | Better; broad regression did not reproduce |
| resize_only | 279.05 us | 273.91 us | Better |
| warm | 548.44 us | 527.12 us | Better; broad regression did not reproduce |
| color_change_full_rebuild | 574.79 us | 531.68 us | Better; broad regression did not reproduce |
| visual_only_rebuild | 377.51 us | 377.14 us | Stable and still much better than Attempt 13 |

Decision: accept Attempt 14. The focused retest cleared the broad-run concern. Integration still requires text-edit correctness coverage, especially same-frame tree visual-only plus text edit aggregation.

## Attempt 15: lazy cached-measurer construction in panel layout system

Purpose: improve no-change retained frames by not building the cache-backed text measurement closure until a panel actually needs layout/reuse work.

Change: replace eager `build_cached_measure(&cache, &measurer)` with an `Option<MeasureTextFn>` initialized only after a panel passes the changed/pending gate.

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- no_change_update --noplot
```

| Rows | Accepted baseline context | Attempt 15 | Decision |
|---:|---:|---:|---|
| 5 | Attempt 14 broad: 71.591 us | 91.767 us | Worse |
| 20 | Attempt 14 broad: 78.962 us | 72.743 us | Better |
| 100 | Attempt 14 broad: 72.590 us | 72.857 us | Neutral |
| 500 | Attempt 14 focused retest: 72.120 us | 81.599 us | Worse |

Decision: reject and revert. A fixed-overhead optimization should be consistently neutral or better. This was mixed and regressed the key 500-row no-change baseline.

## Attempt 16: skip editable-field collection for trees with no editable fields

Purpose: reduce full layout commit work by avoiding editable-field record collection when a tree contains no editable fields.

Change:

- Added private `editable_element_count` to `LayoutTree`.
- Incremented it when elements with editable metadata are added.
- Preserved it through scaling via clone.
- Added `LayoutTree::has_editable_fields()`.
- Returned empty field records/conflicts immediately when there are no editable fields.

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- panel_500_rows --noplot
```

Manual baseline: Attempt 14 focused 500-row retest.

| Slice | Attempt 14 | Attempt 16 | Decision |
|---|---:|---:|---|
| cold | 495.30 us | 492.56 us | Slightly better/noise |
| no_change_update | 72.120 us | 72.736 us | Neutral/slightly worse |
| resize_only | 273.91 us | 265.48 us | Better |
| warm | 527.12 us | 543.54 us | Worse/noise |
| color_change_full_rebuild | 531.68 us | 532.43 us | Neutral |
| visual_only_rebuild | 377.14 us | 376.75 us | Neutral |

Decision: reject and revert. The target warm/color full rebuild paths did not improve, and the cached count adds a maintenance invariant.

## Attempt 17: default preallocation for `LayoutBuilder`

Purpose: reduce early reallocations when warm/color-full retained benches rebuild `LayoutTree`s through `LayoutBuilder::with_root`.

Change: add private `DEFAULT_BUILDER_CAPACITY = 64` for `LayoutBuilder::new` and `LayoutBuilder::with_root`. Explicit `LayoutBuilder::with_capacity` behavior remains unchanged.

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- panel_500_rows --noplot
```

| Slice | Attempt 14/16 neighborhood | Attempt 17 | Decision |
|---|---:|---:|---|
| cold | ~492-495 us | 499.40 us | Worse |
| no_change_update | ~72 us | 74.585 us | Worse |
| resize_only | ~265-274 us | 266.46 us | Neutral |
| warm | ~527 us | 528.84 us | Neutral |
| color_change_full_rebuild | ~531 us | 530.11 us | Neutral |
| visual_only_rebuild | ~377 us | 374.75 us | Neutral/noise |

Decision: reject and revert. The target paths did not improve, and the fixed capacity regressed cold/no-change paths.

## Attempt 18: compute draw-order text anchor during enumeration

Purpose: remove a scan in `DrawOrder::from_commands` by recording the first text ordinal while commands are already being enumerated.

Change:

- `enumerate_draw_commands` returns both enumerated commands and first text ordinal.
- `DrawOrder::from_commands` uses that value instead of scanning commands again.
- Test-only ordinal enumeration adapts to the new return shape.

Command:

```sh
cargo bench -p bevy_diegetic --bench panel_perf --features bench_support -- panel_500_rows --noplot
```

| Slice | Accepted neighborhood | Attempt 18 | Decision |
|---|---:|---:|---|
| cold | ~495 us | 496.93 us | Neutral |
| no_change_update | ~72-76 us | 76.430 us | Neutral/noise |
| resize_only | ~266-274 us | 266.46 us | Neutral |
| warm | ~527 us | 534.39 us | Neutral/slightly worse |
| color_change_full_rebuild | ~531 us | 536.50 us | Neutral/slightly worse |
| visual_only_rebuild | ~377 us | 375.49 us | Neutral |

Decision: reject and revert. Removing the text-anchor scan did not measurably improve retained rebuild paths.

## Attempt 19: classification hotspot diagnostic

Purpose: decide whether the final attempt should optimize `LayoutTree::classify_change`.

Change: no production-code change. Ran the existing raw tree-diff benchmark group after an initial empty-match filter attempt (`classify`) produced no Criterion measurements.

Command:

```sh
cargo bench -p bevy_diegetic --bench layout_engine_raw --features bench_support -- layout_tree_diff --noplot
```

| slice | measured time |
| --- | ---: |
| `layout_tree_diff_5_rows/compare_text_color_only_tree` | 503.01 ns |
| `layout_tree_diff_5_rows/compare_background_color_only_tree` | 530.11 ns |
| `layout_tree_diff_20_rows/compare_text_color_only_tree` | 1.7285 us |
| `layout_tree_diff_20_rows/compare_background_color_only_tree` | 1.7510 us |
| `layout_tree_diff_100_rows/compare_text_color_only_tree` | 8.6072 us |
| `layout_tree_diff_100_rows/compare_background_color_only_tree` | 9.2577 us |
| `layout_tree_diff_500_rows/compare_identical_tree` | 44.922 us |
| `layout_tree_diff_500_rows/compare_text_color_only_tree` | 41.827 us |
| `layout_tree_diff_500_rows/compare_background_color_only_tree` | 45.106 us |
| `layout_tree_diff_500_rows/compare_layout_change_early_exit` | 10.238 ns |
| `layout_tree_diff_500_rows/compare_layout_change_late_exit` | 44.927 us |

Interpretation: classification is row-scaled for full-tree identical/visual comparisons, but the 500-row visual-only classifier cost is only about 42-45 us versus the accepted Attempt 14 `visual_only_rebuild` path around 377 us. Even a perfect classifier elimination would cap total-path gain around 11-12%, and a realistic safe change would recover less.

Panel decision: stop implementation attempts. The remaining retained-update cost is likely downstream of classification: command regeneration, draw-order/projection, ECS/reconciliation, component writes, or benchmark harness work. Attempts 15-18 already rejected several cheap guesses in that area.

Decision: no Attempt 20 implementation. Leave Attempt 20 unused unless the attempt budget is explicitly reopened with a profiler-backed or bench-isolated target.

## Final plain-English summary

The main result is not "Diegetic is always faster than Clay" or "Clay is always faster than Diegetic." They are optimized for different situations.

Clay is faster when both systems rebuild the whole panel every frame. Diegetic can be much faster when it is used as a retained system, where unchanged frames keep prior layout state and skip most layout work.

Definitions:

- **Immediate full rebuild**: build the UI tree, compute layout, and generate draw commands every frame.
- **Retained mode**: keep the previous tree/layout result and only redo work when something changes.
- **Visual-only change**: a change like color or paint style that should not change element sizes or positions.

Simple benchmark framing:

| Scenario | What happens | Result |
|---|---|---|
| Full rebuild every frame | Clay and Diegetic both rebuild everything | Clay is still faster |
| No changes in retained mode | Diegetic skips layout work | Diegetic is much faster |
| Visual-only retained update | Diegetic reuses geometry and updates commands | Diegetic improved a lot, but still has retained update overhead |

Approximate current comparison:

| 500-row case | Time |
|---|---:|
| Clay immediate full rebuild | ~163 us |
| Diegetic immediate build + compute | ~194 us |
| Diegetic retained no-change update | ~72 us |

Practical takeaway: Clay wins the full-rebuild benchmark. Diegetic wins when the app can keep the panel around and most frames do not require rebuilding layout.

Accepted improvements:

- Text wrapping now avoids building a temporary parent lookup table. The traversal carries parent width directly.
- Layout traversal now skips second visits for elements that cannot produce after-children draw commands.
- Text command generation now avoids no-op scaling when `font_scale == 1.0`.
- The first X and Y size-propagation passes were merged into one traversal.
- Tree-classified visual-only updates now skip defensive text remeasurement when the tree comparison already proves layout geometry is unchanged.

Measured outcome:

| Area | Result |
|---|---|
| Direct Clay comparison | Diegetic gap improved from about `1.33x-1.52x` slower to about `1.19x-1.36x` slower |
| 500-row retained visual-only update | Improved from about `453 us` to about `377 us` |
| 500-row retained no-change update | Around `72 us`, much faster than Clay's full rebuild path |

What did not work:

- Storing parent links in the tree made build/cache behavior worse.
- A single-grow-child layout shortcut was too small and inconsistent.
- Broad draw-order refresh skipping was unsafe.
- A safer draw-order classifier cost more than it saved.
- Leaf fast paths did not hold up across benchmarks.
- One-pass tree scaling regressed important cases.
- Retained render-command buffer reuse did not help enough.
- Lazy cached-measurer setup hurt or mixed no-change results.
- Editable-field count caching added state without improving the target paths.
- Builder preallocation was too blunt and caused regressions.
- Draw-order text-anchor enumeration cleanup was neutral.
- Classifier optimization was not pursued because the diagnostic showed it was not the dominant remaining cost.

Next steps before integration:

- Add or run correctness tests for border, divider, and clipping behavior after the traversal skip.
- Add layout parity coverage for the fused X/Y size-propagation pass.
- Add text-edit safety coverage for visual-only geometry reuse, especially same-frame visual tree updates plus text edits.
- Keep using `layout_comparison` for Clay-facing full-rebuild performance.
- Keep using `panel_perf` for retained-mode performance, especially `no_change_update` and `visual_only_rebuild`.
- Investigate remaining retained warm/color rebuild cost in downstream reconciliation, text child updates, command application, or Bevy component writes rather than more layout math guessing.

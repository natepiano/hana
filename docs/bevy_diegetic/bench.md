# 2026-05-11 Bench Review

Decisions from the ad hoc review of next steps after clarifying `bevy_diegetic` benchmark labels and docs.

## Decisions

1. Split benchmark matrix: use a non-default `bench_support` feature for raw internal engine benchmarks, so normal public API stays unchanged while Criterion can measure internals cleanly. Expose internals through a `#[doc(hidden)]` `bench_support` module.
2. API boundary: keep raw engine benchmarking behind `bench_support` rather than making `LayoutEngine` part of the normal supported public API.
3. Public-path optimization: keep scaled-tree caching as the first optimization candidate, but defer it until the split benchmark matrix confirms there is a real public-path issue worth fixing.

## Next-Step Review

1. Keep: add `bench_support = []` and a `#[doc(hidden)]` `bench_support` module that re-exports only the raw internals needed by Criterion, without expanding the normal supported public API.
2. Keep: add a raw `layout_engine_raw` Criterion target gated by `required-features = ["bench_support"]`; benchmark a prebuilt `LayoutTree` through `LayoutEngine::compute` so raw engine cost is separated from tree building, Bevy scheduling, change detection, and unit scaling.
3. Keep: refactor shared fixtures into `crates/bevy_diegetic/benches/common/`, with `mod.rs`, `measurement.rs`, `rows.rs`, `panels.rs`, and `app.rs`. `layout_comparison.rs`, `panel_perf.rs`, and `layout_engine_raw.rs` will each use `mod common;`; keep the shared module nested so Cargo does not treat it as a standalone bench target.
4. Keep: expand `panel_perf` with public-path scenarios that isolate separate questions: `no_change_update` for unchanged Bevy frames, `resize_only` for reflowing an unchanged tree at a new panel size, `warm` for replacing the panel with a freshly rebuilt but logically identical tree, and `color_change_rebuild` as the baseline for future visual-only fast paths.
5. Keep: add diagnostic micro-bench slices for `build_tree_only`, `scale_tree_only`, and `raw_compute_prebuilt_tree` so the benchmark matrix can show whether cost comes from public tree construction, unit conversion/scaled-tree cloning, or the raw layout algorithm. Keep the matrix full featured, but preserve a direct Clay-vs-diegetic comparison as an overall goal.
6. Keep: once the benchmark matrix exists, run baseline benches before choosing optimization work. No premature optimization; scaled-tree caching and/or layout-affecting hashes become candidates only if the new data shows `scale_tree_only`, `color_change_rebuild`, or related public-path results are material.

## Implemented Matrix

1. `layout_comparison`: direct Clay-vs-diegetic layout comparison with shared fixtures, gated by `bench_support`; no Bevy ECS path.
2. `panel_perf`: covers `cold`, `no_change_update`, `resize_only`, `warm`, and `color_change_rebuild`.
3. `layout_engine_raw`: gated behind `bench_support`; covers `build_tree_only`, `scale_tree_only`, and `raw_compute_prebuilt_tree`.
4. CI benchmark smoke command: `cargo bench -p bevy_diegetic --benches --features bench_support`.

## Scaled-Tree Cache Pass

Implemented `ScaledLayoutTreeCache` as a required component on `DiegeticPanel`.
The cache stores the point-scaled `LayoutTree` per panel and invalidates only
when `DiegeticPanel::set_tree` advances `tree_revision`, or when layout/font
unit scale factors change. Width and height changes reuse the cached scaled
tree and still run layout against the new bounds.

System coverage now includes a screen-space percent panel resized through the
primary window. The test confirms the first layout is a cache miss and the
window resize is a cache hit.

`panel_perf` comparison after the cache:

| Scenario | Earlier ad hoc mean | After mean | Manual delta | Criterion report |
| --- | ---: | ---: | ---: | --- |
| 5 rows / resize_only | 50.078 us | 51.428 us | +2.7% | -4.1873%, noise threshold |
| 20 rows / resize_only | 64.649 us | 62.752 us | -2.9% | -5.1121%, improved |
| 100 rows / resize_only | 113.50 us | 96.560 us | -14.9% | -17.758%, improved |
| 500 rows / resize_only | 365.87 us | 233.05 us | -36.3% | -36.179%, improved |

Interpretation: the cache helps where scaled-tree cloning was large enough to
matter. The 500-row resize path is the clearest win; no-change frames remain
dominated by the fixed Bevy update baseline, and color-change rebuild still
replaces the tree so it does not use this cache.

## Layout-Tree Diff Cost Pass

Added `LayoutTree::classify_change` behind `bench_support` and benchmarked it
in `layout_engine_raw`. The classifier exits immediately on layout-affecting
differences, but must walk the full tree to prove a change is visual-only.

`layout_tree_diff` means:

- `Identical`: no inspected fields differ.
- `VisualOnly`: differences are limited to render-only fields such as text
  color, background color, border color, or image tint.
- `LayoutAffecting`: structure, sizing, text content, measurement fields, or
  placement fields changed.

Raw comparison costs:

| Scenario | 5 rows | 20 rows | 100 rows | 500 rows |
| --- | ---: | ---: | ---: | ---: |
| identical | 589.96 ns | 1.989 us | 9.935 us | 48.373 us |
| text color only | 576.44 ns | 1.910 us | 9.220 us | 44.866 us |
| background color only | 592.08 ns | 1.982 us | 9.899 us | 48.195 us |
| layout change, early exit | 12.695 ns | 12.672 ns | 12.668 ns | 12.644 ns |
| layout change, late exit | 580.97 ns | 1.977 us | 10.067 us | 48.659 us |

Compared with the same run's raw slices, a 500-row text-color-only comparison
costs ~44.9 us versus ~117.5 us to scale the tree and ~104.3 us to compute
layout. That supports trying a color-only fast path, as long as render-command
patching can reuse the existing layout result without changing bounds.

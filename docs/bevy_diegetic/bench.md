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

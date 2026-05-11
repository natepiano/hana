# 2026-05-11 Bench Review

Decisions from the ad hoc review of next steps after clarifying `bevy_diegetic` benchmark labels and docs.

## Decisions

1. Split benchmark matrix: use a non-default `bench_support` feature for raw internal engine benchmarks, so normal public API stays unchanged while Criterion can measure internals cleanly. Expose internals through a `#[doc(hidden)]` `bench_support` module.
2. API boundary: keep raw engine benchmarking behind `bench_support` rather than making `LayoutEngine` part of the normal supported public API.
3. Public-path optimization: keep scaled-tree caching as the first optimization candidate, but defer it until the split benchmark matrix confirms there is a real public-path issue worth fixing.

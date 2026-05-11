#![allow(
    clippy::expect_used,
    reason = "benchmarks expect valid fixtures and use black_box for output"
)]

//! Raw `LayoutEngine` and diagnostic micro-benchmarks.
//!
//! This target is gated behind `bench_support` because it intentionally
//! touches crate internals that are not part of the normal public API.
//!
//! Slices per row count:
//! - **`build_tree_only`**: public `LayoutBuilder` tree construction.
//! - **`scale_tree_only`**: unit conversion via `LayoutTree::scaled`.
//! - **`raw_compute_prebuilt_tree`**: `LayoutEngine::compute` on an already built and scaled tree.
//!
//! Run with `cargo bench --bench layout_engine_raw --features bench_support`.

mod common;

use std::hint::black_box;

use bevy_diegetic::bench_support::LayoutEngine;
use common::measurement::monospace_measure_text_fn;
use common::panels::PANEL_SIZE;
use common::panels::build_diegetic_status_tree;
use common::panels::layout_to_points;
use common::rows::ROW_COUNTS;
use common::rows::generate_status_rows;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

fn bench_raw_layout(c: &mut Criterion) {
    let scale = layout_to_points(PANEL_SIZE);
    let viewport_size = PANEL_SIZE * scale;

    for row_count in ROW_COUNTS {
        let rows = generate_status_rows(row_count);
        let group_name = format!("raw_status_panel_{row_count}_rows");
        let mut group = c.benchmark_group(&group_name);

        group.bench_function("build_tree_only", |b| {
            b.iter(|| {
                let tree = build_diegetic_status_tree(black_box(&rows));
                black_box(tree);
            });
        });

        group.bench_function("scale_tree_only", |b| {
            let tree = build_diegetic_status_tree(&rows);

            b.iter(|| {
                let scaled_tree = tree.scaled(black_box(scale), black_box(scale));
                black_box(scaled_tree);
            });
        });

        group.bench_function("raw_compute_prebuilt_tree", |b| {
            let tree = build_diegetic_status_tree(&rows).scaled(scale, scale);
            let engine = LayoutEngine::new(monospace_measure_text_fn());

            b.iter(|| {
                let result = engine.compute(
                    black_box(&tree),
                    black_box(viewport_size),
                    black_box(viewport_size),
                    black_box(1.0),
                );
                black_box(result);
            });
        });

        group.finish();
    }
}

criterion_group!(benches, bench_raw_layout);
criterion_main!(benches);

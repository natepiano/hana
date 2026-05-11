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
//! - **`layout_tree_diff_*`**: field-by-field tree change classification cost.
//!
//! Run with `cargo bench --bench layout_engine_raw --features bench_support`.

mod common;

use std::hint::black_box;

use bevy::color::Color;
use bevy_diegetic::bench_support::LayoutEngine;
use bevy_diegetic::bench_support::LayoutTreeChange;
use common::measurement::monospace_measure_text_fn;
use common::panels::DiegeticStatusTreeStyle;
use common::panels::PANEL_SIZE;
use common::panels::build_diegetic_status_tree;
use common::panels::build_diegetic_status_tree_with_style;
use common::panels::layout_to_points;
use common::rows::ROW_COUNTS;
use common::rows::StatusRow;
use common::rows::generate_status_rows;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

fn bench_raw_layout(c: &mut Criterion) {
    let scale = layout_to_points(PANEL_SIZE);
    let viewport_size = PANEL_SIZE * scale;

    for row_count in ROW_COUNTS {
        let rows = generate_status_rows(row_count);
        bench_raw_status_panel(c, row_count, &rows, scale, viewport_size);
        bench_layout_tree_diff(c, row_count, &rows);
    }
}

fn bench_raw_status_panel(
    c: &mut Criterion,
    row_count: usize,
    rows: &[StatusRow],
    scale: f32,
    viewport_size: f32,
) {
    let group_name = format!("raw_status_panel_{row_count}_rows");
    let mut group = c.benchmark_group(&group_name);

    group.bench_function("build_tree_only", |b| {
        b.iter(|| {
            let tree = build_diegetic_status_tree(black_box(rows));
            black_box(tree);
        });
    });

    group.bench_function("scale_tree_only", |b| {
        let tree = build_diegetic_status_tree(rows);

        b.iter(|| {
            let scaled_tree = tree.scaled(black_box(scale), black_box(scale));
            black_box(scaled_tree);
        });
    });

    group.bench_function("raw_compute_prebuilt_tree", |b| {
        let tree = build_diegetic_status_tree(rows).scaled(scale, scale);
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

fn bench_layout_tree_diff(c: &mut Criterion, row_count: usize, rows: &[StatusRow]) {
    let base_tree = build_diegetic_status_tree(rows);
    let identical_tree = base_tree.clone();
    let text_color_tree = build_diegetic_status_tree_with_style(
        rows,
        DiegeticStatusTreeStyle {
            text_color: Color::srgb(0.2, 0.8, 1.0),
            ..Default::default()
        },
    );
    let background_color_tree = build_diegetic_status_tree_with_style(
        rows,
        DiegeticStatusTreeStyle {
            root_background: Color::srgb_u8(96, 122, 180),
            ..Default::default()
        },
    );
    let layout_change_early_tree = build_diegetic_status_tree_with_style(
        rows,
        DiegeticStatusTreeStyle {
            root_child_gap: 6.0,
            ..Default::default()
        },
    );
    let mut late_rows = rows.to_vec();
    if let Some((_, value)) = late_rows.last_mut() {
        *value = "changed";
    }
    let layout_change_late_tree = build_diegetic_status_tree(&late_rows);

    assert_tree_diff_fixtures(
        &base_tree,
        &identical_tree,
        &text_color_tree,
        &background_color_tree,
        &layout_change_early_tree,
        &layout_change_late_tree,
    );

    let diff_group_name = format!("layout_tree_diff_{row_count}_rows");
    let mut group = c.benchmark_group(&diff_group_name);

    group.bench_function("compare_identical_tree", |b| {
        b.iter(|| {
            let change = black_box(&base_tree).classify_change(black_box(&identical_tree));
            black_box(change);
        });
    });

    group.bench_function("compare_text_color_only_tree", |b| {
        b.iter(|| {
            let change = black_box(&base_tree).classify_change(black_box(&text_color_tree));
            black_box(change);
        });
    });

    group.bench_function("compare_background_color_only_tree", |b| {
        b.iter(|| {
            let change = black_box(&base_tree).classify_change(black_box(&background_color_tree));
            black_box(change);
        });
    });

    group.bench_function("compare_layout_change_early_exit", |b| {
        b.iter(|| {
            let change =
                black_box(&base_tree).classify_change(black_box(&layout_change_early_tree));
            black_box(change);
        });
    });

    group.bench_function("compare_layout_change_late_exit", |b| {
        b.iter(|| {
            let change = black_box(&base_tree).classify_change(black_box(&layout_change_late_tree));
            black_box(change);
        });
    });

    group.finish();
}

fn assert_tree_diff_fixtures(
    base_tree: &bevy_diegetic::LayoutTree,
    identical_tree: &bevy_diegetic::LayoutTree,
    text_color_tree: &bevy_diegetic::LayoutTree,
    background_color_tree: &bevy_diegetic::LayoutTree,
    layout_change_early_tree: &bevy_diegetic::LayoutTree,
    layout_change_late_tree: &bevy_diegetic::LayoutTree,
) {
    assert_eq!(
        base_tree.classify_change(identical_tree),
        LayoutTreeChange::Identical
    );
    assert_eq!(
        base_tree.classify_change(text_color_tree),
        LayoutTreeChange::VisualOnly
    );
    assert_eq!(
        base_tree.classify_change(background_color_tree),
        LayoutTreeChange::VisualOnly
    );
    assert_eq!(
        base_tree.classify_change(layout_change_early_tree),
        LayoutTreeChange::LayoutAffecting
    );
    assert_eq!(
        base_tree.classify_change(layout_change_late_tree),
        LayoutTreeChange::LayoutAffecting
    );
}

criterion_group!(benches, bench_raw_layout);
criterion_main!(benches);

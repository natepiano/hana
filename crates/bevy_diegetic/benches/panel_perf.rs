#![allow(
    clippy::expect_used,
    reason = "benchmarks expect on just-spawned entities where None is a test bug"
)]

//! Benchmark for public `DiegeticPanel` layout performance at various sizes.
//!
//! Measures the public `DiegeticPanel` update path: build or reuse a
//! `LayoutTree`, mutate a panel when the scenario calls for it, and run
//! `app.update()` so `compute_panel_layouts` has a chance to execute. This
//! includes retained-mode API-boundary work and Bevy scheduling; it is not a
//! raw `LayoutEngine` benchmark.
//!
//! Scenarios per row count:
//! - **`cold`**: first layout for a fresh panel.
//! - **`no_change_update`**: unchanged panel frame; layout should be skipped.
//! - **`resize_only`**: panel dimensions change while the tree is reused.
//! - **`warm`**: same logical tree rebuilt and assigned every iteration.
//! - **`color_change_rebuild`**: same layout structure rebuilt with visual-only text color changes.
//!
//! Run with `cargo bench --bench panel_perf`.

mod common;

use std::hint::black_box;

use bevy::prelude::*;
use bevy_diegetic::ComputedDiegeticPanel;
use bevy_diegetic::DiegeticPanel;
use common::app::create_bench_app;
use common::panels::PANEL_SIZE;
use common::panels::RESIZED_PANEL_SIZE;
use common::panels::bench_panel;
use common::panels::build_diegetic_status_tree;
use common::panels::build_diegetic_status_tree_with_text_color;
use common::rows::ROW_COUNTS;
use common::rows::generate_status_rows;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

fn bench_panel_layout(c: &mut Criterion) {
    for row_count in ROW_COUNTS {
        let rows = generate_status_rows(row_count);
        let group_name = format!("panel_{row_count}_rows");
        let mut group = c.benchmark_group(&group_name);

        group.bench_function("cold", |b| {
            b.iter_with_setup(
                || {
                    let mut app = create_bench_app();
                    let tree = build_diegetic_status_tree(&rows);
                    let entity = app.world_mut().spawn(bench_panel(tree, PANEL_SIZE)).id();
                    (app, entity)
                },
                |(mut app, entity)| {
                    app.update();
                    black_box(app.world().get::<ComputedDiegeticPanel>(entity));
                },
            );
        });

        group.bench_function("no_change_update", |b| {
            let mut app = create_bench_app();
            let tree = build_diegetic_status_tree(&rows);
            let entity = app.world_mut().spawn(bench_panel(tree, PANEL_SIZE)).id();
            app.update();

            b.iter(|| {
                app.update();
                black_box(app.world().get::<ComputedDiegeticPanel>(entity));
            });
        });

        group.bench_function("resize_only", |b| {
            let mut app = create_bench_app();
            let tree = build_diegetic_status_tree(&rows);
            let entity = app.world_mut().spawn(bench_panel(tree, PANEL_SIZE)).id();
            app.update();

            let mut expanded = false;
            b.iter(|| {
                expanded = !expanded;
                let size = if expanded {
                    RESIZED_PANEL_SIZE
                } else {
                    PANEL_SIZE
                };
                let mut panel = app
                    .world_mut()
                    .get_mut::<DiegeticPanel>(entity)
                    .expect("entity must exist");
                panel.set_width(size);
                panel.set_height(size);
                app.update();
                black_box(app.world().get::<ComputedDiegeticPanel>(entity));
            });
        });

        group.bench_function("warm", |b| {
            let mut app = create_bench_app();
            let tree = build_diegetic_status_tree(&rows);
            let entity = app.world_mut().spawn(bench_panel(tree, PANEL_SIZE)).id();
            app.update();

            b.iter(|| {
                let tree = build_diegetic_status_tree(&rows);
                app.world_mut()
                    .get_mut::<DiegeticPanel>(entity)
                    .expect("entity must exist")
                    .set_tree(tree);
                app.update();
                black_box(app.world().get::<ComputedDiegeticPanel>(entity));
            });
        });

        group.bench_function("color_change_rebuild", |b| {
            let mut app = create_bench_app();
            let tree = build_diegetic_status_tree(&rows);
            let entity = app.world_mut().spawn(bench_panel(tree, PANEL_SIZE)).id();
            app.update();

            let mut toggle = false;
            b.iter(|| {
                toggle = !toggle;
                let color = if toggle {
                    Color::srgb(1.0, 0.0, 0.0)
                } else {
                    Color::srgb(0.0, 0.0, 1.0)
                };
                let tree = build_diegetic_status_tree_with_text_color(&rows, color);
                app.world_mut()
                    .get_mut::<DiegeticPanel>(entity)
                    .expect("entity must exist")
                    .set_tree(tree);
                app.update();
                black_box(app.world().get::<ComputedDiegeticPanel>(entity));
            });
        });

        group.finish();
    }
}

criterion_group!(benches, bench_panel_layout);
criterion_main!(benches);

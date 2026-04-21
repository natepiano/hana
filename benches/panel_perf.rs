#![allow(
    clippy::expect_used,
    reason = "benchmarks expect on just-spawned entities where None is a test bug"
)]

//! Benchmark for `DiegeticPanel` layout performance at various sizes.
//!
//! Measures the real user-facing cost: build a `LayoutTree`, assign it to a
//! `DiegeticPanel`, and run `app.update()` so `compute_panel_layouts` executes.
//!
//! Three scenarios per row count:
//! - **cold**: First layout — full engine computation.
//! - **warm**: Same tree reassigned — full engine computation (change detected).
//! - **`color_only`**: Tree rebuilt with different colors, same structure — layout hash matches,
//!   only render command colors are patched.
//!
//! Run with `cargo bench --bench panel_perf`.

use std::hint::black_box;
use std::sync::Arc;

use bevy::app::App;
use bevy::prelude::*;
use bevy_diegetic::Border;
use bevy_diegetic::ComputedDiegeticPanel;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticTextMeasurer;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::HeadlessLayoutPlugin;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextDimensions;
use bevy_diegetic::TextMeasure;
use bevy_diegetic::Unit;
use bevy_diegetic::UnitConfig;
use bevy_kana::ToF32;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

// ── Shared measurement ──────────────────────────────────────────────────

const FONT_SIZE: f32 = 7.0;
const LAYOUT_SIZE: f32 = 160.0;
const LAYOUT_MPU: f32 = 1.0 / LAYOUT_SIZE;

fn monospace_measurer() -> DiegeticTextMeasurer {
    DiegeticTextMeasurer {
        measure_fn: Arc::new(|text: &str, measure: &TextMeasure| {
            let char_width = measure.size * 0.6;
            let mut max_line_width: f32 = 0.0;
            let mut line_count = 0_u32;
            for line in text.lines() {
                line_count += 1;
                let width = line.chars().count().to_f32() * char_width;
                max_line_width = max_line_width.max(width);
            }
            if line_count == 0 {
                line_count = 1;
            }
            TextDimensions {
                width:       max_line_width,
                height:      measure.size * line_count.to_f32(),
                line_height: measure.size,
            }
        }),
    }
}

// ── Row data ────────────────────────────────────────────────────────────

const WORDS: &[&str] = &[
    "bevy",
    "diegetic",
    "layout",
    "engine",
    "text",
    "rendering",
    "msdf",
    "atlas",
    "glyph",
    "quad",
    "mesh",
    "shader",
    "pipeline",
    "parley",
    "shaping",
    "font",
    "registry",
    "plugin",
    "system",
    "resource",
];

fn generate_rows(count: usize) -> Vec<(String, &'static str)> {
    (0..count)
        .map(|i| (format!("item {i}:"), WORDS[i % WORDS.len()]))
        .collect()
}

// ── Tree builder ────────────────────────────────────────────────────────

fn build_panel_tree(rows: &[(String, &str)], text_color: Color) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(LAYOUT_SIZE, LAYOUT_SIZE);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(5.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .background(Color::srgb_u8(40, 44, 52))
            .border(Border::all(1.0, Color::srgb_u8(120, 130, 140))),
        |b| {
            for (label, value) in rows {
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::FIT)
                        .direction(Direction::LeftToRight)
                        .child_gap(5.0),
                    |b| {
                        b.text(
                            label,
                            LayoutTextStyle::new(FONT_SIZE).with_color(text_color),
                        );
                        b.with(
                            El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                            |_| {},
                        );
                        b.text(
                            *value,
                            LayoutTextStyle::new(FONT_SIZE).with_color(text_color),
                        );
                    },
                );
            }
        },
    );
    builder.build()
}

// ── Headless app ────────────────────────────────────────────────────────

fn create_bench_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(monospace_measurer());
    app.insert_resource(UnitConfig::default());
    app.add_plugins(HeadlessLayoutPlugin);
    app.update();
    app
}

fn bench_panel(tree: bevy_diegetic::LayoutTree) -> DiegeticPanel {
    let dim = bevy_diegetic::Dimension {
        value: LAYOUT_SIZE,
        unit:  Some(Unit::Custom(LAYOUT_MPU)),
    };
    DiegeticPanel::world()
        .size(
            bevy_diegetic::Sizing::Fixed(dim),
            bevy_diegetic::Sizing::Fixed(dim),
        )
        .font_unit(Unit::Custom(LAYOUT_MPU))
        .with_tree(tree)
        .build()
        .expect("bench panel dimensions must be valid")
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_panel_layout(c: &mut Criterion) {
    for row_count in [5, 20, 100, 500] {
        let rows = generate_rows(row_count);
        let group_name = format!("panel_{row_count}_rows");
        let mut group = c.benchmark_group(&group_name);

        // Cold: first layout computation for a fresh panel.
        group.bench_function("cold", |b| {
            b.iter_with_setup(
                || {
                    let mut app = create_bench_app();
                    let tree = build_panel_tree(&rows, Color::WHITE);
                    let entity = app.world_mut().spawn(bench_panel(tree)).id();
                    (app, entity)
                },
                |(mut app, entity)| {
                    app.update();
                    black_box(app.world().get::<ComputedDiegeticPanel>(entity));
                },
            );
        });

        // Warm: tree mutation triggers full layout recomputation.
        group.bench_function("warm", |b| {
            let mut app = create_bench_app();
            let tree = build_panel_tree(&rows, Color::WHITE);
            let entity = app.world_mut().spawn(bench_panel(tree)).id();
            app.update(); // Initial layout.

            b.iter(|| {
                // Rebuild tree with same content — triggers Changed<DiegeticPanel>.
                let tree = build_panel_tree(&rows, Color::WHITE);
                app.world_mut()
                    .get_mut::<DiegeticPanel>(entity)
                    .expect("entity must exist")
                    .set_tree(tree);
                app.update();
                black_box(app.world().get::<ComputedDiegeticPanel>(entity));
            });
        });

        // Color-only: same layout structure, different colors — hash matches,
        // skips engine.compute(), only patches render command colors.
        group.bench_function("color_only", |b| {
            let mut app = create_bench_app();
            let tree = build_panel_tree(&rows, Color::WHITE);
            let entity = app.world_mut().spawn(bench_panel(tree)).id();
            app.update(); // Initial layout.

            let mut toggle = false;
            b.iter(|| {
                // Alternate colors to ensure Changed<DiegeticPanel> fires.
                toggle = !toggle;
                let color = if toggle {
                    Color::srgb(1.0, 0.0, 0.0)
                } else {
                    Color::srgb(0.0, 0.0, 1.0)
                };
                let tree = build_panel_tree(&rows, color);
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

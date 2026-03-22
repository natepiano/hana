#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::expect_used)]

//! Benchmark comparing Clay (FFI) and `bevy_diegetic` layout performance on
//! identical panel layouts.
//!
//! - **Clay**: immediate-mode full cycle (allocate + build + layout + collect).
//! - **Diegetic**: ECS-based — spawn a `DiegeticPanel`, run `app.update()`, read
//!   `ComputedDiegeticPanel`. Uses [`LayoutPlugin`] for headless operation.
//!
//! # Methodology notes
//!
//! The diegetic side pays ~30µs of Bevy ECS scheduling overhead per
//! `app.update()` that the clay side does not. This overhead is constant
//! regardless of layout size — it's the scheduler dispatching systems and
//! running change detection.
//!
//! In a real Bevy application, clay's output would also be processed through
//! the ECS (spawning entities, building meshes), so both sides pay this cost
//! in practice. The bench intentionally includes it for the diegetic side to
//! measure the true user-facing cost of using `DiegeticPanel`.
//!
//! Subtracting the ECS baseline (~30µs), the raw layout engine is faster than
//! clay at all tested sizes.
//!
//! Run with `cargo bench --bench layout_comparison`.

use std::hint::black_box;
use std::sync::Arc;

use bevy::app::App;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::ComputedDiegeticPanel;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticTextMeasurer;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutPlugin;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;
use bevy_diegetic::TextDimensions;
use bevy_diegetic::TextMeasure;
use clay_layout::Clay;
use clay_layout::ClayLayoutScope;
use clay_layout::Declaration;
use clay_layout::fit;
use clay_layout::fixed;
use clay_layout::grow;
use clay_layout::layout::Alignment;
use clay_layout::layout::LayoutAlignmentX;
use clay_layout::layout::LayoutAlignmentY;
use clay_layout::layout::LayoutDirection;
use clay_layout::math::Dimensions;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

// ── Shared measurement ──────────────────────────────────────────────────

const FONT_SIZE: f32 = 10.0;
const CLAY_FONT_SIZE: u16 = 10;
const CHAR_WIDTH_FACTOR: f32 = 0.6;

fn monospace_measurer() -> DiegeticTextMeasurer {
    DiegeticTextMeasurer {
        measure_fn: Arc::new(|text: &str, measure: &TextMeasure| {
            let line_height = measure.effective_line_height();
            let char_width = measure.size * CHAR_WIDTH_FACTOR;
            let mut max_line_width: f32 = 0.0;
            let mut line_count = 0_u32;
            for line in text.lines() {
                line_count += 1;
                let width = line.chars().count() as f32 * char_width;
                max_line_width = max_line_width.max(width);
            }
            if line_count == 0 {
                line_count = 1;
            }
            TextDimensions {
                width:  max_line_width,
                height: line_height * line_count as f32,
            }
        }),
    }
}

fn clay_monospace_measure(
    text: &str,
    config: &clay_layout::text::TextConfig,
    _: &mut (),
) -> Dimensions {
    let font_size = f32::from(config.font_size);
    let char_width = font_size * CHAR_WIDTH_FACTOR;
    let line_height = if config.line_height == 0 {
        font_size
    } else {
        f32::from(config.line_height)
    };
    let mut max_line_width: f32 = 0.0;
    let mut line_count = 0_u32;
    for line in text.lines() {
        line_count += 1;
        let width = line.chars().count() as f32 * char_width;
        max_line_width = max_line_width.max(width);
    }
    if line_count == 0 {
        line_count = 1;
    }
    Dimensions {
        width:  max_line_width,
        height: line_height * line_count as f32,
    }
}

// ── Row data ────────────────────────────────────────────────────────────

fn generate_rows(count: usize) -> Vec<(&'static str, &'static str)> {
    const LABELS: &[&str] = &[
        "fps:",
        "frame ms:",
        "radius:",
        "entities:",
        "triangles:",
        "draw calls:",
        "memory:",
        "cpu:",
        "gpu:",
        "batches:",
        "lights:",
        "shadows:",
        "textures:",
        "meshes:",
        "shaders:",
        "cameras:",
        "viewports:",
        "particles:",
        "bones:",
        "clips:",
    ];
    const VALUES: &[&str] = &[
        "60", "16.7", "0.3", "1024", "128000", "42", "512MB", "23%", "45%", "18", "4", "8", "256",
        "64", "32", "2", "1", "10000", "128", "3",
    ];
    (0..count)
        .map(|i| (LABELS[i % LABELS.len()], VALUES[i % VALUES.len()]))
        .collect()
}

// ── Layout builders ─────────────────────────────────────────────────────

fn build_clay_panel<'a>(
    layout: &mut ClayLayoutScope<'a, 'a, (), ()>,
    rows: &[(&str, &str)],
    size: f32,
) {
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(size))
            .padding(clay_layout::layout::Padding::all(8))
            .direction(LayoutDirection::TopToBottom)
            .child_gap(5)
            .end()
            .background_color((180, 96, 122).into()),
        |clay| {
            // Header.
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!(FONT_SIZE, 20.0))
                    .padding(clay_layout::layout::Padding::new(5, 5, 4, 4))
                    .child_alignment(Alignment::new(
                        LayoutAlignmentX::Left,
                        LayoutAlignmentY::Center,
                    ))
                    .end()
                    .background_color((52, 98, 90).into()),
                |clay| {
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(grow!())
                            .height(fit!())
                            .direction(LayoutDirection::LeftToRight)
                            .end(),
                        |clay| {
                            clay.with(
                                Declaration::new()
                                    .layout()
                                    .width(fit!())
                                    .height(grow!())
                                    .end(),
                                |clay| {
                                    clay.text(
                                        "STATUS",
                                        clay_layout::text::TextConfig::new()
                                            .font_size(CLAY_FONT_SIZE)
                                            .end(),
                                    );
                                },
                            );
                            clay.with(
                                Declaration::new()
                                    .layout()
                                    .width(grow!())
                                    .height(fixed!(1.0))
                                    .end(),
                                |_| {},
                            );
                            clay.with(
                                Declaration::new()
                                    .layout()
                                    .width(fit!())
                                    .height(grow!())
                                    .end(),
                                |clay| {
                                    clay.text(
                                        "BENCH",
                                        clay_layout::text::TextConfig::new()
                                            .font_size(CLAY_FONT_SIZE)
                                            .end(),
                                    );
                                },
                            );
                        },
                    );
                },
            );
            // Divider.
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fixed!(4.0))
                    .end()
                    .background_color((74, 196, 172).into()),
                |_| {},
            );
            // Body.
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!())
                    .end()
                    .background_color((22, 28, 34).into()),
                |clay| {
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(grow!())
                            .padding(clay_layout::layout::Padding::all(5))
                            .direction(LayoutDirection::TopToBottom)
                            .child_gap(2)
                            .end(),
                        |clay| {
                            for (label, value) in rows {
                                clay.with(
                                    Declaration::new()
                                        .layout()
                                        .width(grow!())
                                        .height(fit!())
                                        .direction(LayoutDirection::LeftToRight)
                                        .end(),
                                    |clay| {
                                        clay.text(
                                            label,
                                            clay_layout::text::TextConfig::new()
                                                .font_size(CLAY_FONT_SIZE)
                                                .end(),
                                        );
                                        clay.with(
                                            Declaration::new().layout().width(grow!()).end(),
                                            |_| {},
                                        );
                                        clay.text(
                                            value,
                                            clay_layout::text::TextConfig::new()
                                                .font_size(CLAY_FONT_SIZE)
                                                .end(),
                                        );
                                    },
                                );
                            }
                        },
                    );
                },
            );
        },
    );
}

fn build_diegetic_tree(rows: &[(&str, &str)], size: f32) -> bevy_diegetic::LayoutTree {
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(size))
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .child_gap(5.0)
            .background(bevy::color::Color::srgb_u8(180, 96, 122)),
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::grow_range(FONT_SIZE, 20.0))
            .padding(Padding::new(5.0, 5.0, 4.0, 4.0))
            .child_align_y(AlignY::Center)
            .background(bevy::color::Color::srgb_u8(52, 98, 90)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight),
                |b| {
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("STATUS", TextConfig::new(FONT_SIZE));
                    });
                    b.with(
                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                        |_| {},
                    );
                    b.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::GROW)
                            .child_align_x(AlignX::Right),
                        |b| {
                            b.text("BENCH", TextConfig::new(FONT_SIZE));
                        },
                    );
                },
            );
        },
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(4.0))
            .background(bevy::color::Color::srgb_u8(74, 196, 172)),
        |_| {},
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb_u8(22, 28, 34)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .padding(Padding::all(5.0))
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0),
                |b| {
                    for (label, value) in rows {
                        b.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::FIT)
                                .direction(Direction::LeftToRight),
                            |b| {
                                b.text(*label, TextConfig::new(FONT_SIZE));
                                b.with(
                                    El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                    |_| {},
                                );
                                b.text(*value, TextConfig::new(FONT_SIZE));
                            },
                        );
                    }
                },
            );
        },
    );
    b.build()
}

// ── Headless Bevy app for diegetic benchmarking ─────────────────────────

/// Creates a minimal headless Bevy app with `LayoutPlugin`.
///
/// No rendering, no window, no GPU — only the ECS scheduler and
/// layout computation with a monospace measurer.
fn create_bench_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(monospace_measurer());
    app.add_plugins(LayoutPlugin);
    app.update();
    app
}

// ── Benchmark runners ───────────────────────────────────────────────────

fn run_clay_layout(rows: &[(&str, &str)], size: f32) {
    let mut clay = Clay::new((size, size).into());
    clay.set_measure_text_function_user_data((), clay_monospace_measure);
    let mut layout = clay.begin::<(), ()>();
    build_clay_panel(&mut layout, rows, size);
    let cmds: Vec<_> = layout.end().collect();
    black_box(&cmds);
}

fn run_diegetic_layout(app: &mut App, entity: Entity, rows: &[(&str, &str)], size: f32) {
    let tree = build_diegetic_tree(rows, size);
    app.world_mut()
        .get_mut::<DiegeticPanel>(entity)
        .expect("panel entity must exist")
        .tree = tree;
    app.update();
    let computed = app.world().get::<ComputedDiegeticPanel>(entity);
    black_box(&computed);
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_status_panel(c: &mut Criterion) {
    let size = 160.0;

    for row_count in [5, 20, 100, 500] {
        let rows = generate_rows(row_count);

        let group_name = format!("status_panel_{row_count}_rows");
        let mut group = c.benchmark_group(&group_name);

        group.bench_function("clay", |b| {
            b.iter(|| run_clay_layout(&rows, size));
        });

        group.bench_function("diegetic", |b| {
            let mut app = create_bench_app();
            let tree = build_diegetic_tree(&rows, size);
            let entity = app
                .world_mut()
                .spawn(DiegeticPanel {
                    tree,
                    layout_width: size,
                    layout_height: size,
                    world_width: 1.0,
                    world_height: 1.0,
                })
                .id();
            // First update to compute initial layout.
            app.update();

            b.iter(|| run_diegetic_layout(&mut app, entity, &rows, size));
        });

        group.finish();
    }
}

criterion_group!(benches, bench_status_panel);
criterion_main!(benches);

#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]
#![allow(clippy::too_many_lines)]

//! Benchmark comparing Clay (FFI) and `bevy_diegetic` layout engines on identical
//! complex layouts. Run with `cargo bench`.

use std::hint::black_box;
use std::sync::Arc;

use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutEngine;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::MeasureTextFn;
use bevy_diegetic::Padding;
use bevy_diegetic::RenderCommandKind;
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
use clay_layout::render_commands::RenderCommandConfig;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

// ── Shared measurement ──────────────────────────────────────────────────

const FONT_SIZE: f32 = 10.0;
const CLAY_FONT_SIZE: u16 = 10;
const CHAR_WIDTH_FACTOR: f32 = 0.6;

fn monospace_measure() -> MeasureTextFn {
    Arc::new(|text: &str, measure: &TextMeasure| {
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
    })
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

// ── Parity verification ─────────────────────────────────────────────────
//
// Before timing, we run both engines once on the same layout and assert that
// every rectangle and text bounding box matches within 0.5 units. This ensures
// the benchmark is comparing two implementations that produce *identical* output.

#[derive(Debug, Clone, Copy)]
struct Bbox {
    x:    f32,
    y:    f32,
    w:    f32,
    h:    f32,
    kind: BboxKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BboxKind {
    Rectangle,
    Text,
}

fn approx_eq(a: f32, b: f32) -> bool { (a - b).abs() < 0.5 }

fn collect_clay_bboxes<'a>(
    commands: impl IntoIterator<Item = clay_layout::render_commands::RenderCommand<'a, (), ()>>,
) -> Vec<Bbox> {
    let mut out = Vec::new();
    for cmd in commands {
        let kind = match cmd.config {
            RenderCommandConfig::Rectangle(_) => BboxKind::Rectangle,
            RenderCommandConfig::Text(_) => BboxKind::Text,
            _ => continue,
        };
        out.push(Bbox {
            x: cmd.bounding_box.x,
            y: cmd.bounding_box.y,
            w: cmd.bounding_box.width,
            h: cmd.bounding_box.height,
            kind,
        });
    }
    out
}

fn collect_diegetic_bboxes(result: &bevy_diegetic::LayoutResult) -> Vec<Bbox> {
    let mut out = Vec::new();
    for cmd in &result.commands {
        let kind = match &cmd.kind {
            RenderCommandKind::Rectangle { .. } => BboxKind::Rectangle,
            RenderCommandKind::Text { .. } => BboxKind::Text,
            _ => continue,
        };
        out.push(Bbox {
            x: cmd.bounds.x,
            y: cmd.bounds.y,
            w: cmd.bounds.width,
            h: cmd.bounds.height,
            kind,
        });
    }
    out
}

fn assert_bboxes_match(clay_boxes: &[Bbox], diegetic_boxes: &[Bbox], kind: BboxKind) {
    let clay_filtered: Vec<_> = clay_boxes.iter().filter(|b| b.kind == kind).collect();
    let diegetic_filtered: Vec<_> = diegetic_boxes.iter().filter(|b| b.kind == kind).collect();
    assert_eq!(
        clay_filtered.len(),
        diegetic_filtered.len(),
        "{kind:?} count mismatch: Clay={}, Diegetic={}",
        clay_filtered.len(),
        diegetic_filtered.len(),
    );
    for (i, (c, d)) in clay_filtered
        .iter()
        .zip(diegetic_filtered.iter())
        .enumerate()
    {
        assert!(
            approx_eq(c.x, d.x)
                && approx_eq(c.y, d.y)
                && approx_eq(c.w, d.w)
                && approx_eq(c.h, d.h),
            "{kind:?}[{i}] mismatch:\n  Clay:     x={:.1} y={:.1} w={:.1} h={:.1}\n  Diegetic: x={:.1} y={:.1} w={:.1} h={:.1}",
            c.x,
            c.y,
            c.w,
            c.h,
            d.x,
            d.y,
            d.w,
            d.h,
        );
    }
}

/// Runs both engines on the same layout and panics if bounding boxes diverge.
///
/// Both engines cull off-screen elements by default, so command counts and
/// positions should match directly.
fn verify_parity(rows: &[(&str, &str)], size: f32, measure: &MeasureTextFn) {
    // Clay
    let mut clay = Clay::new((size, size).into());
    clay.set_measure_text_function_user_data((), clay_monospace_measure);
    let mut layout = clay.begin::<(), ()>();
    build_clay_panel(&mut layout, rows, size);
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic (culling enabled by default — matches Clay)
    let engine = LayoutEngine::new(measure.clone());
    let tree = build_diegetic_panel(rows, size);
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Text);
}

// ── Shared layout builders ──────────────────────────────────────────────
//
// Both the parity check and the timed benchmark share the same layout
// construction. Extracted here so there is exactly one definition of the
// status-panel layout per engine.

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
            // Header
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
            // Divider
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fixed!(4.0))
                    .end()
                    .background_color((74, 196, 172).into()),
                |_| {},
            );
            // Body
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

fn build_diegetic_panel(rows: &[(&str, &str)], size: f32) -> LayoutTree {
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

// ── Benchmark runners ───────────────────────────────────────────────────

fn run_clay_layout(rows: &[(&str, &str)], size: f32) {
    let mut clay = Clay::new((size, size).into());
    clay.set_measure_text_function_user_data((), clay_monospace_measure);
    let mut layout = clay.begin::<(), ()>();
    build_clay_panel(&mut layout, rows, size);
    let cmds: Vec<_> = layout.end().collect();
    black_box(&cmds);
}

fn run_diegetic_layout(rows: &[(&str, &str)], size: f32, measure: &MeasureTextFn) {
    let engine = LayoutEngine::new(measure.clone());
    let tree = build_diegetic_panel(rows, size);
    let result = engine.compute(&tree, size, size);
    black_box(&result);
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_status_panel(c: &mut Criterion) {
    let size = 160.0;
    let measure = monospace_measure();

    for row_count in [5, 20, 100, 500] {
        let rows = generate_rows(row_count);

        // Verify both engines produce identical output before timing.
        verify_parity(&rows, size, &measure);

        let group_name = format!("status_panel_{row_count}_rows");
        let mut group = c.benchmark_group(&group_name);

        group.bench_function("clay", |b| {
            b.iter(|| run_clay_layout(&rows, size));
        });

        group.bench_function("diegetic", |b| {
            b.iter(|| run_diegetic_layout(&rows, size, &measure));
        });

        group.finish();
    }
}

criterion_group!(benches, bench_status_panel);
criterion_main!(benches);

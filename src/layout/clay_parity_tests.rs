//! Clay parity tests.
//!
//! Each test builds the same layout in both Clay (via `clay-layout` FFI) and
//! `bevy_diegetic`, then asserts that every rectangle/text bounding box matches.
//! This is the source of truth — if Clay produces it, we should too.

use std::sync::Arc;

use clay_layout::Clay;
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

use super::AlignX;
use super::AlignY;
use super::Direction;
use super::El;
use super::LayoutBuilder;
use super::LayoutEngine;
use super::LayoutResult;
use super::MeasureTextFn;
use super::Padding;
use super::RenderCommandKind;
use super::Sizing;
use super::TextConfig;
use super::TextDimensions;
use super::TextMeasure;

// ── Shared measurement ────────────────────────────────────────────────────

const FONT_SIZE: f32 = 10.0;
const CLAY_FONT_SIZE: u16 = 10;
const CHAR_WIDTH_FACTOR: f32 = 0.6;

/// Monospace measurement: each char = `font_size * 0.6` wide, one line = `font_size` tall.
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

/// Same measurement logic for Clay's callback.
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

// ── Bounding box comparison ───────────────────────────────────────────────

/// A simplified bounding box for comparison between the two engines.
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

// ── Clay helper ───────────────────────────────────────────────────────────

fn new_clay(size: f32) -> Clay {
    let mut clay = Clay::new((size, size).into());
    clay.set_measure_text_function_user_data((), clay_monospace_measure);
    clay
}

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

// ── Diegetic helper ───────────────────────────────────────────────────────

fn collect_diegetic_bboxes(result: &LayoutResult) -> Vec<Bbox> {
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

// ── Tests ─────────────────────────────────────────────────────────────────

#[test]
fn parity_fixed_root_with_grow_child() {
    let size = 160.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(size))
            .end()
            .background_color((255, 0, 0).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!())
                    .end()
                    .background_color((0, 255, 0).into()),
                |_| {},
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(size))
            .background(bevy::color::Color::srgb(1.0, 0.0, 0.0)),
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb(0.0, 1.0, 0.0)),
        |_| {},
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
fn parity_header_body_divider() {
    let size = 160.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
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
                    .height(fixed!(20.0))
                    .end()
                    .background_color((52, 98, 90).into()),
                |_| {},
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
                |_| {},
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
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
            .height(Sizing::fixed(20.0))
            .background(bevy::color::Color::srgb_u8(52, 98, 90)),
        |_| {},
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
        |_| {},
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
fn parity_key_value_row_with_spacer() {
    let size = 160.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(size))
            .direction(LayoutDirection::LeftToRight)
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.text(
                "fps:",
                clay_layout::text::TextConfig::new()
                    .font_size(CLAY_FONT_SIZE)
                    .end(),
            );
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fixed!(1.0))
                    .end(),
                |_| {},
            );
            clay.text(
                "60",
                clay_layout::text::TextConfig::new()
                    .font_size(CLAY_FONT_SIZE)
                    .end(),
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(size))
            .direction(Direction::LeftToRight)
            .background(bevy::color::Color::srgb_u8(22, 28, 34)),
    );
    b.text("fps:", TextConfig::new(FONT_SIZE));
    b.with(
        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
        |_| {},
    );
    b.text("60", TextConfig::new(FONT_SIZE));
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Text);
}

#[test]
fn parity_vertical_center_alignment() {
    let size = 160.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(size))
            .direction(LayoutDirection::LeftToRight)
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
                    .width(fixed!(40.0))
                    .height(fixed!(20.0))
                    .end()
                    .background_color((255, 0, 0).into()),
                |_| {},
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(size))
            .direction(Direction::LeftToRight)
            .child_align_y(AlignY::Center)
            .background(bevy::color::Color::srgb_u8(52, 98, 90)),
    );
    b.with(
        El::new()
            .width(Sizing::fixed(40.0))
            .height(Sizing::fixed(20.0))
            .background(bevy::color::Color::srgb(1.0, 0.0, 0.0)),
        |_| {},
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
#[allow(clippy::too_many_lines)]
fn parity_fit_parent_with_grow_children_centering() {
    // The header vertical-centering bug: Fit-height parent, Grow-height
    // children containing text, centered vertically in a fixed container.
    let size = 160.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(size))
            .end(),
        |clay| {
            // Header container: fixed height, centers child vertically.
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fixed!(30.0))
                    .padding(clay_layout::layout::Padding::new(0, 0, 4, 4))
                    .child_alignment(Alignment::new(
                        LayoutAlignmentX::Left,
                        LayoutAlignmentY::Center,
                    ))
                    .end()
                    .background_color((52, 98, 90).into()),
                |clay| {
                    // Text row: Fit height, LeftToRight.
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(grow!())
                            .height(fit!())
                            .direction(LayoutDirection::LeftToRight)
                            .end()
                            .background_color((22, 28, 34).into()),
                        |clay| {
                            // Title slot: Grow height.
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
                            // Spacer.
                            clay.with(
                                Declaration::new()
                                    .layout()
                                    .width(grow!())
                                    .height(fixed!(1.0))
                                    .end(),
                                |_| {},
                            );
                            // Subtitle slot: Grow height.
                            clay.with(
                                Declaration::new()
                                    .layout()
                                    .width(fit!())
                                    .height(grow!())
                                    .end(),
                                |clay| {
                                    clay.text(
                                        "SUB",
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
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(size)),
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(30.0))
            .padding(Padding::new(0.0, 0.0, 4.0, 4.0))
            .child_align_y(AlignY::Center)
            .background(bevy::color::Color::srgb_u8(52, 98, 90)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .background(bevy::color::Color::srgb_u8(22, 28, 34)),
                |b| {
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("STATUS", TextConfig::new(FONT_SIZE));
                    });
                    b.with(
                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                        |_| {},
                    );
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("SUB", TextConfig::new(FONT_SIZE));
                    });
                },
            );
        },
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Text);
}

#[test]
fn parity_compression_with_content_minimum() {
    // Two Fit siblings each with Fixed(50) content in an 80-wide parent.
    // Tests whether compression respects propagated content minimums.
    let size = 80.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(100.0))
            .direction(LayoutDirection::LeftToRight)
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(fit!())
                    .height(grow!())
                    .end()
                    .background_color((255, 0, 0).into()),
                |clay| {
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(fixed!(50.0))
                            .height(fixed!(10.0))
                            .end()
                            .background_color((0, 255, 0).into()),
                        |_| {},
                    );
                },
            );
            clay.with(
                Declaration::new()
                    .layout()
                    .width(fit!())
                    .height(grow!())
                    .end()
                    .background_color((0, 0, 255).into()),
                |clay| {
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(fixed!(50.0))
                            .height(fixed!(10.0))
                            .end()
                            .background_color((255, 255, 0).into()),
                        |_| {},
                    );
                },
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(100.0))
            .direction(Direction::LeftToRight)
            .background(bevy::color::Color::srgb_u8(22, 28, 34)),
    );
    b.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb(1.0, 0.0, 0.0)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(10.0))
                    .background(bevy::color::Color::srgb(0.0, 1.0, 0.0)),
                |_| {},
            );
        },
    );
    b.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb(0.0, 0.0, 1.0)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(10.0))
                    .background(bevy::color::Color::srgb(1.0, 1.0, 0.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, 100.0);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
fn parity_cross_axis_grow_with_large_content() {
    // TopToBottom parent (30 wide), Grow child with Fixed(50) inner content.
    // Tests cross-axis minDimensions floor.
    let width = 30.0;
    let height = 100.0;

    // Clay
    let mut clay = Clay::new((width, height).into());
    clay.set_measure_text_function_user_data((), clay_monospace_measure);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(width))
            .height(fixed!(height))
            .direction(LayoutDirection::TopToBottom)
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!())
                    .end()
                    .background_color((255, 0, 0).into()),
                |clay| {
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(fixed!(50.0))
                            .height(fixed!(10.0))
                            .end()
                            .background_color((0, 255, 0).into()),
                        |_| {},
                    );
                },
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(width))
            .height(Sizing::fixed(height))
            .direction(Direction::TopToBottom)
            .background(bevy::color::Color::srgb_u8(22, 28, 34)),
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb(1.0, 0.0, 0.0)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(10.0))
                    .background(bevy::color::Color::srgb(0.0, 1.0, 0.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, width, height);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
fn parity_two_grow_children_horizontal() {
    let size = 200.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(100.0))
            .direction(LayoutDirection::LeftToRight)
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!())
                    .end()
                    .background_color((255, 0, 0).into()),
                |_| {},
            );
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!())
                    .end()
                    .background_color((0, 0, 255).into()),
                |_| {},
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(100.0))
            .direction(Direction::LeftToRight)
            .background(bevy::color::Color::srgb_u8(22, 28, 34)),
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb(1.0, 0.0, 0.0)),
        |_| {},
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb(0.0, 0.0, 1.0)),
        |_| {},
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, 100.0);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
fn parity_padding_and_child_gap() {
    let size = 160.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(size))
            .padding(clay_layout::layout::Padding::new(10, 20, 5, 15))
            .direction(LayoutDirection::TopToBottom)
            .child_gap(8)
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fixed!(30.0))
                    .end()
                    .background_color((255, 0, 0).into()),
                |_| {},
            );
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!())
                    .end()
                    .background_color((0, 255, 0).into()),
                |_| {},
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(size))
            .padding(Padding::new(10.0, 20.0, 5.0, 15.0))
            .direction(Direction::TopToBottom)
            .child_gap(8.0)
            .background(bevy::color::Color::srgb_u8(22, 28, 34)),
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(30.0))
            .background(bevy::color::Color::srgb(1.0, 0.0, 0.0)),
        |_| {},
    );
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(bevy::color::Color::srgb(0.0, 1.0, 0.0)),
        |_| {},
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
fn parity_right_alignment() {
    let size = 200.0;

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
    layout.with(
        Declaration::new()
            .layout()
            .width(fixed!(size))
            .height(fixed!(100.0))
            .direction(LayoutDirection::LeftToRight)
            .child_alignment(Alignment::new(
                LayoutAlignmentX::Right,
                LayoutAlignmentY::Top,
            ))
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(fixed!(50.0))
                    .height(fixed!(30.0))
                    .end()
                    .background_color((255, 0, 0).into()),
                |_| {},
            );
        },
    );
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(100.0))
            .direction(Direction::LeftToRight)
            .child_align_x(AlignX::Right)
            .background(bevy::color::Color::srgb_u8(22, 28, 34)),
    );
    b.with(
        El::new()
            .width(Sizing::fixed(50.0))
            .height(Sizing::fixed(30.0))
            .background(bevy::color::Color::srgb(1.0, 0.0, 0.0)),
        |_| {},
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, 100.0);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
}

#[test]
#[allow(clippy::too_many_lines)]
fn parity_status_panel_full_layout() {
    // The full status panel layout from the actual application.
    let size = 160.0;

    let labels = [("fps:", "14"), ("frame ms:", "68"), ("radius:", "0.3")];

    // Clay
    let mut clay = new_clay(size);
    let mut layout = clay.begin::<(), ()>();
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
                                    .child_alignment(Alignment::new(
                                        LayoutAlignmentX::Right,
                                        LayoutAlignmentY::Top,
                                    ))
                                    .end(),
                                |clay| {
                                    clay.text(
                                        "DIEGETIC",
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
                            for (label, value) in &labels {
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
    let clay_bboxes = collect_clay_bboxes(layout.end());

    // Diegetic
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
                            b.text("DIEGETIC", TextConfig::new(FONT_SIZE));
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
                    for (label, value) in &labels {
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
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, size, size);
    let diegetic_bboxes = collect_diegetic_bboxes(&result);

    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Rectangle);
    assert_bboxes_match(&clay_bboxes, &diegetic_bboxes, BboxKind::Text);
}

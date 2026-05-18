//! Layout engine integration tests.
//!
//! Each test constructs a layout tree, runs the engine, and verifies the
//! computed bounding boxes match expectations. A simple monospace text
//! measurement function is used throughout.

#![allow(
    clippy::float_cmp,
    reason = "tests compare exact expected layout values"
)]
#![allow(
    clippy::needless_collect,
    reason = "tests collect into named variables for readable assertions and index access"
)]
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests use panic/unwrap for clearer failure messages"
)]

use std::sync::Arc;

use bevy::color::Color;
use bevy_kana::ToF32;

use crate::constants::MONOSPACE_WIDTH_RATIO;
use crate::layout::AlignX;
use crate::layout::AlignY;
use crate::layout::Border;
use crate::layout::Direction;
use crate::layout::El;
use crate::layout::LayoutBuilder;
use crate::layout::LayoutEngine;
use crate::layout::LayoutTextStyle;
use crate::layout::LayoutTree;
use crate::layout::MeasureTextFn;
use crate::layout::Padding;
use crate::layout::RectangleSource;
use crate::layout::RenderCommandKind;
use crate::layout::Sizing;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;
use crate::layout::TextWrap;
use crate::layout::element::Element;
use crate::layout::element::ElementContent;

const VIEWPORT: f32 = 200.0;

fn monospace_measure() -> MeasureTextFn {
    Arc::new(|text: &str, measure: &TextMeasure| {
        let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
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
    })
}

fn approx_eq(a: f32, b: f32) -> bool { (a - b).abs() < 0.01 }

fn text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count().to_f32() * font_size * MONOSPACE_WIDTH_RATIO
}

fn line_height(font_size: f32) -> f32 { font_size }

fn text_height(line_count: u32, font_size: f32) -> f32 {
    line_count.to_f32() * line_height(font_size)
}

fn add_aligned_table_row(
    builder: &mut LayoutBuilder,
    bindings: &[&str],
    action: &str,
    style: &LayoutTextStyle,
    action_min: f32,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(4.0)
            .child_align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0),
                |builder| {
                    for binding in bindings {
                        builder.text(*binding, style.clone());
                    }
                },
            );
            builder.text("->", style.clone());
            builder.with(
                El::new()
                    .width(Sizing::fit_min(action_min))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(action, style.clone());
                },
            );
        },
    );
}

// ── Fixed sizing ─────────────────────────────────────────────────────────────

#[test]
fn fixed_root_dimensions() {
    let tree = LayoutBuilder::new(100.0, 50.0).build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert_eq!(result.computed[0].width, 100.0);
    assert_eq!(result.computed[0].height, 50.0);
}

#[test]
fn fixed_child_dimensions() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::fixed(80.0))
            .height(Sizing::fixed(40.0)),
        |_| {},
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Child is index 1.
    assert_eq!(result.computed[1].width, 80.0);
    assert_eq!(result.computed[1].height, 40.0);
}

// ── Grow sizing ──────────────────────────────────────────────────────────────

#[test]
fn single_grow_child_fills_parent() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert_eq!(result.computed[1].width, 200.0);
    assert_eq!(result.computed[1].height, 100.0);
}

#[test]
fn two_grow_children_split_evenly_horizontal() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Parent is index 1, children are 2 and 3.
    assert!(approx_eq(result.computed[2].width, 100.0));
    assert!(approx_eq(result.computed[3].width, 100.0));
}

#[test]
fn two_grow_children_split_evenly_vertical() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(approx_eq(result.computed[2].height, 50.0));
    assert!(approx_eq(result.computed[3].height, 50.0));
}

#[test]
fn grow_with_min_max() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            // This child wants to grow but is capped at 60.
            b.with(
                El::new()
                    .width(Sizing::grow_range(0.0, 60.0))
                    .height(Sizing::GROW),
                |_| {},
            );
            // This child fills the rest.
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(approx_eq(result.computed[2].width, 60.0));
    assert!(approx_eq(result.computed[3].width, 140.0));
}

// ── Fit sizing ───────────────────────────────────────────────────────────────

#[test]
fn fit_wraps_text_content() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    let font_size = 16.0;
    let text = "Hello";
    // Expected text bounds come from the same monospace metrics as production.
    b.text(text, LayoutTextStyle::new(font_size));
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(approx_eq(
        result.computed[1].width,
        text_width(text, font_size)
    ));
    assert!(approx_eq(result.computed[1].height, line_height(font_size)));
}

#[test]
fn fit_with_min_respects_minimum() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::fit_min(100.0))
            .height(Sizing::fixed(20.0)),
        |b| {
            // Content is only 48px wide but min is 100.
            b.text("Hello", LayoutTextStyle::new(16.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Parent element (index 1) should be at least 100 wide.
    assert!(result.computed[1].width >= 100.0);
}

// ── Engine sanity: Fit root with Grow children (panel-size plan step 1) ──────

#[test]
fn fit_root_clamps_grow_children_content_under_max() {
    // Fit root (max 400) with two horizontal GROW children carrying text.
    // Expect the root to resolve to the combined text width, not the max.
    let font_size = 16.0;
    let text = "Hello";
    let expected_width = text_width(text, font_size) * 2.0;
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fit_range(0.0, 400.0))
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight),
    );
    b.with(El::new().width(Sizing::GROW).height(Sizing::FIT), |b| {
        b.text(text, LayoutTextStyle::new(font_size));
    });
    b.with(El::new().width(Sizing::GROW).height(Sizing::FIT), |b| {
        b.text(text, LayoutTextStyle::new(font_size));
    });
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(
        approx_eq(result.computed[0].width, expected_width),
        "root width = {}, expected {expected_width}",
        result.computed[0].width
    );
}

#[test]
fn fit_root_caps_grow_children_content_at_max() {
    // Same tree, but combined text content is wider than max.
    let wide = "HelloHelloHelloHelloHelloHello";
    let font_size = 16.0;
    let expected_content_width = text_width(wide, font_size) * 2.0;
    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fit_range(0.0, 400.0))
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight),
    );
    b.with(El::new().width(Sizing::GROW).height(Sizing::FIT), |b| {
        b.text(wide, LayoutTextStyle::new(font_size));
    });
    b.with(El::new().width(Sizing::GROW).height(Sizing::FIT), |b| {
        b.text(wide, LayoutTextStyle::new(font_size));
    });
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(
        approx_eq(result.computed[0].width, 400.0),
        "root width = {}, expected 400 capped from content width {expected_content_width}",
        result.computed[0].width,
    );
}

// ── Percent sizing ───────────────────────────────────────────────────────────

#[test]
fn percent_sizing() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.with(
                El::new().width(Sizing::percent(0.3)).height(Sizing::GROW),
                |_| {},
            );
            b.with(
                El::new().width(Sizing::percent(0.7)).height(Sizing::GROW),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(approx_eq(result.computed[2].width, 60.0));
    assert!(approx_eq(result.computed[3].width, 140.0));
}

// ── Padding ──────────────────────────────────────────────────────────────────

#[test]
fn padding_reduces_child_space() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(10.0)),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Parent is 200x100 with 10px padding on each side.
    // Child should be 180x80.
    assert!(approx_eq(result.computed[2].width, 180.0));
    assert!(approx_eq(result.computed[2].height, 80.0));
}

#[test]
fn asymmetric_padding() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::new(5.0, 15.0, 10.0, 20.0)),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Horizontal padding: 5 + 15 = 20, vertical: 10 + 20 = 30.
    assert!(approx_eq(result.computed[2].width, 180.0));
    assert!(approx_eq(result.computed[2].height, 70.0));
}

// ── Child gap ────────────────────────────────────────────────────────────────

#[test]
fn child_gap_horizontal() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_gap(10.0),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // 200 - 2*10 gap = 180 / 3 = 60 each.
    assert!(approx_eq(result.computed[2].width, 60.0));
    assert!(approx_eq(result.computed[3].width, 60.0));
    assert!(approx_eq(result.computed[4].width, 60.0));
}

#[test]
fn child_gap_vertical() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .child_gap(5.0),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // 100 - 5 gap = 95 / 2 = 47.5 each.
    assert!(approx_eq(result.computed[2].height, 47.5));
    assert!(approx_eq(result.computed[3].height, 47.5));
}

// ── Alignment ────────────────────────────────────────────────────────────────

#[test]
fn center_alignment_horizontal() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_align_x(AlignX::Center),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(30.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Find the child's render command to check position.
    // Extra space = 200 - 50 = 150. Center offset = 75.
    let child_bounds = result.computed[2].bounds;
    assert!(approx_eq(child_bounds.x, 75.0));
}

#[test]
fn right_alignment_horizontal() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_align_x(AlignX::Right),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(30.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let child_bounds = result.computed[2].bounds;
    assert!(approx_eq(child_bounds.x, 150.0));
}

#[test]
fn center_alignment_vertical() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_align_y(AlignY::Center),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(30.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Cross-axis centering: (100 - 30) / 2 = 35.
    let child_bounds = result.computed[2].bounds;
    assert!(approx_eq(child_bounds.y, 35.0));
}

#[test]
fn bottom_alignment_vertical() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_align_y(AlignY::Bottom),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(30.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Bottom: (100 - 30) = 70.
    let child_bounds = result.computed[2].bounds;
    assert!(approx_eq(child_bounds.y, 70.0));
}

// ── Direction ────────────────────────────────────────────────────────────────

#[test]
fn left_to_right_positioning() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(60.0))
                    .height(Sizing::fixed(40.0)),
                |_| {},
            );
            b.with(
                El::new()
                    .width(Sizing::fixed(80.0))
                    .height(Sizing::fixed(40.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let first = result.computed[2].bounds;
    let second = result.computed[3].bounds;
    assert!(approx_eq(first.x, 0.0));
    assert!(approx_eq(second.x, 60.0));
}

#[test]
fn top_to_bottom_positioning() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(60.0))
                    .height(Sizing::fixed(30.0)),
                |_| {},
            );
            b.with(
                El::new()
                    .width(Sizing::fixed(60.0))
                    .height(Sizing::fixed(50.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let first = result.computed[2].bounds;
    let second = result.computed[3].bounds;
    assert!(approx_eq(first.y, 0.0));
    assert!(approx_eq(second.y, 30.0));
}

// ── Overflow compression ─────────────────────────────────────────────────────

#[test]
fn overflow_compression_largest_first() {
    let mut b = LayoutBuilder::new(100.0, 50.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            // Total: 60 + 60 = 120, but parent is 100 wide.
            // Both are Fit with Fixed(60) content. Clay's `minDimensions`
            // propagates 60 from each child, so neither can compress below 60.
            // Result: both stay at 60, overflowing the parent by 20.
            b.with(
                El::new()
                    .width(Sizing::fit_range(0.0, 60.0))
                    .height(Sizing::GROW),
                |b| {
                    b.with(
                        El::new()
                            .width(Sizing::fixed(60.0))
                            .height(Sizing::fixed(10.0)),
                        |_| {},
                    );
                },
            );
            b.with(
                El::new()
                    .width(Sizing::fit_range(0.0, 60.0))
                    .height(Sizing::GROW),
                |b| {
                    b.with(
                        El::new()
                            .width(Sizing::fixed(60.0))
                            .height(Sizing::fixed(10.0)),
                        |_| {},
                    );
                },
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // `minDimensions` floor prevents compression below content size.
    assert!(approx_eq(result.computed[2].width, 60.0));
    assert!(approx_eq(result.computed[4].width, 60.0));
}

// ── Render commands ──────────────────────────────────────────────────────────

#[test]
fn render_commands_include_rectangles() {
    let mut b = LayoutBuilder::new(100.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(Color::srgb_u8(255, 0, 0)),
        |_| {},
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let rect_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Rectangle { .. }))
        .collect();

    assert!(
        !rect_commands.is_empty(),
        "Should have at least one rectangle render command"
    );
}

#[test]
fn render_commands_include_text() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.text("Hello", LayoutTextStyle::new(16.0));
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();

    assert_eq!(text_commands.len(), 1);
    if let RenderCommandKind::Text { ref text, .. } = text_commands[0].kind {
        assert_eq!(text, "Hello");
    }
}

#[test]
fn render_commands_include_borders() {
    let mut b = LayoutBuilder::new(100.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .border(Border::all(2.0, Color::srgb_u8(255, 255, 255))),
        |_| {},
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let border_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Border { .. }))
        .collect();

    assert!(
        !border_commands.is_empty(),
        "Should have at least one border render command"
    );
}

// ── Nested layout ────────────────────────────────────────────────────────────

#[test]
fn nested_layout_header_body() {
    // Mimics the status cube layout: header + divider + body.
    let mut b = LayoutBuilder::new(160.0, 160.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .child_gap(5.0),
        |b| {
            // Header: fixed height.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(20.0))
                    .background(Color::srgb_u8(52, 98, 90)),
                |b| {
                    b.text("STATUS", LayoutTextStyle::new(7.0));
                },
            );
            // Divider.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(4.0))
                    .background(Color::srgb_u8(74, 196, 172)),
                |_| {},
            );
            // Body: grows to fill remaining space.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .background(Color::srgb_u8(22, 28, 34)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Root (0): 160x160.
    // Container (1): 160x160 (Grow fills root, which has no padding).
    // Container's children get 160 - 2*8 padding = 144 available.
    let container = &result.computed[1];
    assert!(approx_eq(container.width, 160.0));
    assert!(approx_eq(container.height, 160.0));

    // Header (2): 144 wide (grows within container's padding), 20 tall (fixed).
    let header = &result.computed[2];
    assert!(approx_eq(header.width, 144.0));
    assert!(approx_eq(header.height, 20.0));

    // Divider (4): 144 wide, 4 tall.
    let divider = &result.computed[4];
    assert!(approx_eq(divider.width, 144.0));
    assert!(approx_eq(divider.height, 4.0));

    // Body (5): 144 wide, fills rest. 144 - 20 - 4 - 2*5 gap = 110.
    let body = &result.computed[5];
    assert!(approx_eq(body.width, 144.0));
    assert!(approx_eq(body.height, 110.0));
}

// ── Positioning with padding ─────────────────────────────────────────────────

#[test]
fn children_positioned_after_padding() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::new(20.0, 10.0, 30.0, 5.0))
            .direction(Direction::LeftToRight),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(50.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Child should start at (20, 30) — left padding, top padding.
    let child_bounds = result.computed[2].bounds;
    assert!(approx_eq(child_bounds.x, 20.0));
    assert!(approx_eq(child_bounds.y, 30.0));
}

// ── Mixed fixed and grow ─────────────────────────────────────────────────────

#[test]
fn fixed_and_grow_siblings() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.with(
                El::new().width(Sizing::fixed(50.0)).height(Sizing::GROW),
                |_| {},
            );
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(approx_eq(result.computed[2].width, 50.0));
    assert!(approx_eq(result.computed[3].width, 150.0));
}

// ── Text positioning ─────────────────────────────────────────────────────────

#[test]
fn text_positioned_correctly() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(10.0))
            .direction(Direction::TopToBottom),
        |b| {
            b.text("Hello", LayoutTextStyle::new(16.0));
            b.text("World", LayoutTextStyle::new(16.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // "Hello" at (10, 10), "World" at (10, 26).
    let hello = result.computed[2].bounds;
    let world = result.computed[3].bounds;
    assert!(approx_eq(hello.x, 10.0));
    assert!(approx_eq(hello.y, 10.0));
    assert!(approx_eq(world.x, 10.0));
    assert!(approx_eq(world.y, 26.0));
}

// ── Key-value row layout ─────────────────────────────────────────────────────

#[test]
fn key_value_row_layout() {
    // Label on left, value pushed to right by grow spacer.
    let label_text = "fps:";
    let value_text = "60";
    let font_size = 7.0;
    let label_width = text_width(label_text, font_size);
    let value_width = text_width(value_text, font_size);
    let spacer_width = VIEWPORT - label_width - value_width;
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.text(label_text, LayoutTextStyle::new(font_size));
            b.with(
                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                |_| {},
            );
            b.text(value_text, LayoutTextStyle::new(font_size));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Spacer fills the space left after the label and value text widths.
    let label = result.computed[2].bounds;
    let value = result.computed[4].bounds;
    assert!(approx_eq(label.x, 0.0));
    assert!(approx_eq(result.computed[3].bounds.width, spacer_width));
    assert!(approx_eq(value.x, VIEWPORT - value_width));
}

#[test]
fn fit_table_with_grow_rows_aligns_middle_column() {
    let font_size = 10.0;
    let style = LayoutTextStyle::new(font_size).no_wrap();
    let action_min = text_width("Orbit", font_size);
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(5.0)
            .border(Border::new().between_children(1.0).color(Color::WHITE)),
        |builder| {
            add_aligned_table_row(
                builder,
                &["MMB drag", "Trackpad"],
                "Orbit",
                &style,
                action_min,
            );
            add_aligned_table_row(
                builder,
                &["Shift+MMB drag", "Shift+trackpad"],
                "Pan",
                &style,
                action_min,
            );
            add_aligned_table_row(
                builder,
                &["Wheel", "Ctrl+trackpad", "Pinch"],
                "Zoom",
                &style,
                action_min,
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let arrow_xs = result
        .commands
        .iter()
        .filter_map(|command| match &command.kind {
            RenderCommandKind::Text { text, .. } if text == "->" => Some(command.bounds.x),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(arrow_xs.len(), 3);
    assert!(approx_eq(arrow_xs[0], arrow_xs[1]));
    assert!(approx_eq(arrow_xs[1], arrow_xs[2]));

    let divider_count = result
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command.kind,
                RenderCommandKind::Rectangle {
                    source: RectangleSource::BetweenChildrenBorder,
                    ..
                }
            )
        })
        .count();
    assert_eq!(divider_count, 2);
}

// ── Scissor/clip ─────────────────────────────────────────────────────────────

#[test]
fn clip_emits_scissor_commands() {
    let mut b = LayoutBuilder::new(100.0, 100.0);
    b.with(
        El::new().width(Sizing::GROW).height(Sizing::GROW).clip(),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(50.0))
                    .height(Sizing::fixed(50.0)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    let scissor_starts: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::ScissorStart))
        .collect();
    let scissor_ends: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::ScissorEnd))
        .collect();

    assert_eq!(scissor_starts.len(), 1);
    assert_eq!(scissor_ends.len(), 1);
}

// ── Empty tree ───────────────────────────────────────────────────────────────

#[test]
fn empty_tree_produces_no_commands() {
    let tree = LayoutTree::new();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(result.commands.is_empty());
    assert!(result.computed.is_empty());
}

// ── Between-children borders ─────────────────────────────────────────────────

#[test]
fn between_children_borders_emitted() {
    let mut b = LayoutBuilder::new(200.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .border(
                Border::new()
                    .color(Color::srgb_u8(255, 255, 255))
                    .between_children(2.0),
            ),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // Should have 2 between-children rectangles (3 children = 2 gaps).
    let rect_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| {
            matches!(cmd.kind, RenderCommandKind::Rectangle { .. }) && cmd.element_idx == 1
        })
        .collect();

    assert_eq!(
        rect_commands.len(),
        2,
        "Should have 2 between-children border rectangles"
    );
}

// ── Text wrapping ─────────────────────────────────────────────────────────────

#[test]
fn text_wraps_at_word_boundaries() {
    let font_size = 16.0;
    let hello_width = text_width("Hello", font_size);
    let world_width = text_width("World", font_size);
    let test_width = text_width("Test", font_size);
    let space_width = text_width(" ", font_size);
    let first_line_width = hello_width + space_width + world_width;
    // "Hello World Test" is wider than the container before wrapping.
    // Container is 80 wide — should force word wrapping.
    // "Hello World" is wider than the container, so the three words split
    // across separate lines.
    assert!(first_line_width > 80.0);
    assert!(test_width < 80.0);
    let mut b = LayoutBuilder::new(80.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("Hello World Test", LayoutTextStyle::new(font_size));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 200.0, 1.0);

    // Text element is index 2. Height should be three measured lines.
    assert!(approx_eq(
        result.computed[2].height,
        text_height(3, font_size)
    ));

    // Should emit 3 text render commands for the wrapped lines.
    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 3, "Should have 3 wrapped text lines");
}

#[test]
fn text_no_wrap_overflows() {
    // "Hello World" in a narrow container with TextWrap::None.
    let mut b = LayoutBuilder::new(40.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("Hello World", LayoutTextStyle::new(16.0).no_wrap());
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 40.0, 200.0, 1.0);

    // Height should remain single-line (16.0).
    assert!(approx_eq(result.computed[2].height, 16.0));

    // Single text command emitted (no wrapping).
    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 1);
}

#[test]
fn text_wraps_at_newlines_only() {
    // "Line1\nLine2\nLine3" with TextWrap::Newlines in a wide container.
    let font_size = 16.0;
    let mut b = LayoutBuilder::new(500.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text(
                "Line1\nLine2\nLine3",
                LayoutTextStyle::new(font_size).wrap(TextWrap::Newlines),
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 500.0, 200.0, 1.0);

    assert!(approx_eq(
        result.computed[2].height,
        text_height(3, font_size)
    ));

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 3);
}

#[test]
fn word_wrap_long_word_does_not_break() {
    let font_size = 16.0;
    let word = "Supercalifragilistic";
    let word_width = text_width(word, font_size);
    // Container is only 80 wide. The word should NOT be broken — stays on one line.
    assert!(word_width > 80.0);
    let mut b = LayoutBuilder::new(80.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text(word, LayoutTextStyle::new(font_size));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 200.0, 1.0);

    // Should be a single line (word never broken mid-word).
    assert!(approx_eq(result.computed[2].height, line_height(font_size)));

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 1);
}

#[test]
fn word_wrap_preserves_explicit_newlines() {
    let font_size = 16.0;
    let first_paragraph_width = text_width("AA BB", font_size);
    let second_paragraph_width = text_width("CC DD", font_size);
    // "AA BB\nCC DD" with Words wrap in a container that fits "AA BB" but not all four.
    // Container is 50 wide — "AA BB" fits on one line, "CC DD" fits on one line.
    assert!(first_paragraph_width < 50.0);
    assert!(second_paragraph_width < 50.0);
    let mut b = LayoutBuilder::new(50.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("AA BB\nCC DD", LayoutTextStyle::new(font_size));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 50.0, 200.0, 1.0);

    // Two paragraphs, each fits on one line.
    assert!(approx_eq(
        result.computed[2].height,
        text_height(2, font_size)
    ));

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 2);
}

#[test]
fn word_wrap_empty_string() {
    let font_size = 16.0;
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("", LayoutTextStyle::new(font_size));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 200.0, 200.0, 1.0);

    // Empty text produces one empty line.
    assert!(approx_eq(result.computed[2].height, line_height(font_size)));
}

#[test]
fn word_wrap_updates_parent_fit_height() {
    let font_size = 16.0;
    // Parent is Fit-height, child text wraps to 3 lines.
    // Parent height should grow to accommodate.
    let mut b = LayoutBuilder::new(80.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("Hello World Test", LayoutTextStyle::new(font_size));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 200.0, 1.0);

    // Text wraps to 3 lines (see text_wraps_at_word_boundaries test).
    // Parent Fit height should follow the measured wrapped text height.
    assert!(approx_eq(
        result.computed[1].height,
        text_height(3, font_size)
    ));
}

#[test]
fn word_wrap_render_commands_per_line() {
    // Verify each wrapped line has correct bounds positioning.
    let font_size = 16.0;
    let first_line_width = text_width("AA BB", font_size);
    let second_line_width = text_width("CC", font_size);
    let mut b = LayoutBuilder::new(50.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            // The first wrapped line fits in the container; the second starts
            // one measured line height lower.
            assert!(first_line_width < 50.0);
            assert!(second_line_width < 50.0);
            b.text("AA BB CC", LayoutTextStyle::new(font_size));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 50.0, 200.0, 1.0);

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();

    assert_eq!(text_commands.len(), 2);

    // First line starts at y=0 (relative to parent).
    assert!(approx_eq(text_commands[0].bounds.y, 0.0));

    // Second line starts one measured line height down.
    assert!(approx_eq(text_commands[1].bounds.y, line_height(font_size)));
}

// ── Fit parent with Grow children propagation ─────────────────────────────

#[test]
fn fit_parent_sees_grow_children_content_height() {
    let title_font_size = 7.0;
    let subtitle_font_size = 4.0;
    let text_row_height = line_height(title_font_size);
    // Reproduces the header vertical-centering bug: a Fit-height parent
    // with Grow-height children that contain text. The Fit parent must
    // propagate the children's content size upward so it gets a real
    // height, not collapse to the spacer's 1.0.
    //
    // Layout (mirrors the status panel header):
    //   header_container (Grow height, padding 4/4, align_y=Center)
    //     text_row (Fit height, LeftToRight)
    //       title_slot (Fit width, Grow height) → text "STATUS" (font 7)
    //       spacer (Grow width, Fixed height=1)
    //       subtitle_slot (Fit width, Grow height) → text "SUB" (font 4)
    //
    // text_row Fit height should be the title line height, NOT the spacer.
    let mut b = LayoutBuilder::new(160.0, 160.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(20.0))
            .padding(Padding::new(0.0, 0.0, 4.0, 4.0))
            .child_align_y(AlignY::Center),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight),
                |b| {
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("STATUS", LayoutTextStyle::new(title_font_size));
                    });
                    b.with(
                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                        |_| {},
                    );
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("SUB", LayoutTextStyle::new(subtitle_font_size));
                    });
                },
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 160.0, 160.0, 1.0);

    // text_row is index 2.
    let computed_text_row_height = result.computed[2].height;
    assert!(
        computed_text_row_height >= text_row_height - 0.01,
        "Fit text_row height should be >= {text_row_height} (text content), got {computed_text_row_height}"
    );

    // text_row should be vertically centered in header_container.
    // header_container is 20.0 tall with 4+4=8 vertical padding → 12 content area.
    // Center offset = (12 - text_row_height) / 2 + 4 (top padding).
    let text_row_bounds = result.computed[2].bounds;
    let expected_y = (12.0 - computed_text_row_height).mul_add(0.5, 4.0);
    assert!(
        approx_eq(text_row_bounds.y, expected_y),
        "text_row should be centered: expected y={expected_y}, got y={}",
        text_row_bounds.y
    );
}

// ── Clay parity: minDimensions propagation ────────────────────────────────
//
// Clay tracks a propagated `minDimensions` field on every element — the
// recursive minimum size derived from nested content. Our engine does not
// track this yet. These tests encode Clay's correct behavior and will fail
// until we implement `minDimensions`.

#[test]
fn compression_respects_content_minimum_symmetric() {
    // Two Fit siblings each containing a Fixed(50) child in an 80-wide parent.
    // Total content = 100, overflow = 20.
    //
    // Clay: `minDimensions` = 50 for each Fit wrapper (propagated from the
    // Fixed child). Compression cannot reduce either below 50. Both stay at
    // 50 — the parent overflows by 20.
    //
    // Our engine: `min_size()` = 0 (default Fit), so compression squashes
    // both to 40.
    let mut b = LayoutBuilder::new(80.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                b.with(
                    El::new()
                        .width(Sizing::fixed(50.0))
                        .height(Sizing::fixed(10.0)),
                    |_| {},
                );
            });
            b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                b.with(
                    El::new()
                        .width(Sizing::fixed(50.0))
                        .height(Sizing::fixed(10.0)),
                    |_| {},
                );
            });
        },
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 100.0, 1.0);

    // Indices: 0=root, 1=container, 2=fit_a, 3=fixed_a, 4=fit_b, 5=fixed_b
    let child_a = result.computed[2].width;
    let child_b = result.computed[4].width;
    assert!(
        child_a >= 50.0 - 0.01,
        "Fit child A should not compress below content minimum 50.0, got {child_a}"
    );
    assert!(
        child_b >= 50.0 - 0.01,
        "Fit child B should not compress below content minimum 50.0, got {child_b}"
    );
}

#[test]
fn compression_respects_content_minimum_asymmetric() {
    // Fit child A has Fixed(60) content, Fit child B has Fixed(30). Parent is 80.
    // Total = 90, overflow = 10.
    //
    // Clay: A has `minDimensions` = 60, B has `minDimensions` = 30. Compression
    // targets the largest (A at 60) but `minDimensions` prevents any reduction.
    // Both stay at their content size.
    //
    // Our engine: compresses A from 60 to 50 (largest-first, 10px distributed).
    let mut b = LayoutBuilder::new(80.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                b.with(
                    El::new()
                        .width(Sizing::fixed(60.0))
                        .height(Sizing::fixed(10.0)),
                    |_| {},
                );
            });
            b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                b.with(
                    El::new()
                        .width(Sizing::fixed(30.0))
                        .height(Sizing::fixed(10.0)),
                    |_| {},
                );
            });
        },
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 100.0, 1.0);

    let child_a = result.computed[2].width;
    assert!(
        child_a >= 60.0 - 0.01,
        "Fit child A should not compress below content minimum 60.0, got {child_a}"
    );
}

#[test]
fn cross_axis_grow_respects_content_minimum() {
    // Clay's cross-axis sizing enforces `minDimensions` as a floor.
    // A Grow child whose nested content is wider than the parent should
    // not be squished below its content minimum.
    //
    // Layout: TopToBottom parent (30 wide), Grow-width child containing
    // a Fixed(50) inner element.
    //
    // Clay: cross-axis sets Grow to `MIN(parent-padding, max)` = 30, then
    // applies `MAX(minDimensions=50, 30)` = 50. Content minimum wins.
    //
    // Our engine: Grow fills parent = 30. No content floor.
    let mut b = LayoutBuilder::new(30.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |b| {
                b.with(
                    El::new()
                        .width(Sizing::fixed(50.0))
                        .height(Sizing::fixed(10.0)),
                    |_| {},
                );
            });
        },
    );
    let tree = b.build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 30.0, 100.0, 1.0);

    // Index 2 is the Grow child.
    let child_width = result.computed[2].width;
    assert!(
        child_width >= 50.0 - 0.01,
        "Cross-axis Grow child should not shrink below content minimum 50.0, got {child_width}"
    );
}

#[test]
fn grow_body_compression_20_rows() {
    // Reproduces the benchmark parity failure: root 160x160, TopToBottom,
    // with header(Grow 10..20), divider(Fixed 4), body(Grow).
    // 20 rows of text overflow the available space. Clay keeps body at its
    // content height (248), header at content height (18). We should match.
    let size = 160.0_f32;
    let measure = monospace_measure();

    let rows: Vec<(&str, &str)> = (0..20)
        .map(|i| {
            let labels = ["fps:", "frame ms:", "radius:", "entities:", "triangles:"];
            let values = ["60", "16.7", "0.3", "1024", "128000"];
            (labels[i % 5], values[i % 5])
        })
        .collect();

    let mut b = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(size))
            .height(Sizing::fixed(size))
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .child_gap(5.0),
    );
    // Header: Grow height 10..20
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::grow_range(10.0, 20.0))
            .padding(Padding::new(5.0, 5.0, 4.0, 4.0))
            .child_align_y(AlignY::Center),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight),
                |b| {
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("STATUS", LayoutTextStyle::new(10.0));
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
                            b.text("BENCH", LayoutTextStyle::new(10.0));
                        },
                    );
                },
            );
        },
    );
    // Divider: Fixed 4
    b.with(
        El::new().width(Sizing::GROW).height(Sizing::fixed(4.0)),
        |_| {},
    );
    // Body: Grow
    b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |b| {
        b.with(
            El::new()
                .width(Sizing::GROW)
                .padding(Padding::all(5.0))
                .direction(Direction::TopToBottom)
                .child_gap(2.0),
            |b| {
                for (label, value) in &rows {
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .direction(Direction::LeftToRight),
                        |b| {
                            b.text(*label, LayoutTextStyle::new(10.0));
                            b.with(
                                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                |_| {},
                            );
                            b.text(*value, LayoutTextStyle::new(10.0));
                        },
                    );
                }
            },
        );
    });
    let tree = b.build();
    let engine = LayoutEngine::new(measure);
    let result = engine.compute(&tree, size, size, 1.0);

    // Root children by builder order: header (1), divider (8), body (9).
    // Available height = 160 - 16 (padding) - 10 (2 gaps) = 134.
    // Header: GROW(10..20), content = 18 (4+10+4 padding + 10px text) → 18.
    // Divider: Fixed(4) → 4.
    // Body: GROW, 20 rows of text → content height 248.
    // Content overflows the panel (18 + 4 + 248 = 270 > 134). The layout
    // engine keeps the content height rather than compressing the body;
    // render-side systems decide what is visible in the current viewport.
    let header = &result.computed[1];
    let divider = &result.computed[8];
    let body = &result.computed[9];

    assert_eq!(
        header.bounds.height, 18.0,
        "header height (content within [10, 20])"
    );
    assert_eq!(divider.bounds.height, 4.0, "divider height (Fixed)");
    assert_eq!(
        body.bounds.height, 248.0,
        "body height (full content, overflows panel)"
    );
}

#[test]
#[ignore = "manual perf benchmark — run with --ignored"]
fn perf_element_sizes() {
    println!("Element: {} bytes", std::mem::size_of::<Element>());
    println!(
        "ElementContent: {} bytes",
        std::mem::size_of::<ElementContent>()
    );
    println!(
        "TextConfig: {} bytes",
        std::mem::size_of::<LayoutTextStyle>()
    );
    println!("Border: {} bytes", std::mem::size_of::<Border>());
    println!("Sizing: {} bytes", std::mem::size_of::<Sizing>());
    println!("Padding: {} bytes", std::mem::size_of::<Padding>());
    println!("String: {} bytes", std::mem::size_of::<String>());
    println!("Vec<usize>: {} bytes", std::mem::size_of::<Vec<usize>>());
    println!(
        "Option<Color>: {} bytes",
        std::mem::size_of::<Option<Color>>()
    );
    println!(
        "Option<Border>: {} bytes",
        std::mem::size_of::<Option<Border>>()
    );
}

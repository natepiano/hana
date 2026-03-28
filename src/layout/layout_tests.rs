//! Layout engine integration tests.
//!
//! Each test constructs a layout tree, runs the engine, and verifies the computed
//! bounding boxes match expectations. A simple monospace text measurement function
//! is used throughout: each character is `font_size * 0.6` wide, one line tall.

#![allow(
    clippy::float_cmp,
    clippy::needless_collect,
    clippy::cast_precision_loss
)]

use std::sync::Arc;

use bevy::color::Color;

use super::AlignX;
use super::AlignY;
use super::Border;
use super::Direction;
use super::El;
use super::LayoutBuilder;
use super::LayoutEngine;
use super::LayoutTextStyle;
use super::LayoutTree;
use super::MeasureTextFn;
use super::Padding;
use super::RenderCommandKind;
use super::Sizing;
use super::TextDimensions;
use super::TextMeasure;
use super::TextWrap;

const VIEWPORT: f32 = 200.0;

fn monospace_measure() -> MeasureTextFn {
    Arc::new(|text: &str, measure: &TextMeasure| {
        let char_width = measure.size * 0.6;
        let mut max_line_width: f32 = 0.0;
        let mut line_count = 0_u32;
        for line in text.lines() {
            line_count += 1;
            #[allow(clippy::cast_precision_loss)]
            let width = line.chars().count() as f32 * char_width;
            max_line_width = max_line_width.max(width);
        }
        if line_count == 0 {
            line_count = 1;
        }
        TextDimensions {
            width:                                        max_line_width,
            #[allow(clippy::cast_precision_loss)]
            height:                                       measure.size * line_count as f32,
            line_height:                                  measure.size,
        }
    })
}

fn approx_eq(a: f32, b: f32) -> bool { (a - b).abs() < 0.01 }

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
    // "Hello" = 5 chars * 16 * 0.6 = 48.0 wide, 16.0 tall.
    b.text("Hello", LayoutTextStyle::new(16.0));
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    assert!(approx_eq(result.computed[1].width, 48.0));
    assert!(approx_eq(result.computed[1].height, 16.0));
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
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.text("fps:", LayoutTextStyle::new(7.0));
            b.with(
                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                |_| {},
            );
            b.text("60", LayoutTextStyle::new(7.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT, 1.0);

    // "fps:" is 4 chars * 7 * 0.6 = 16.8 wide.
    // "60" is 2 chars * 7 * 0.6 = 8.4 wide.
    // Spacer fills 200 - 16.8 - 8.4 = 174.8.
    let label = result.computed[2].bounds;
    let value = result.computed[4].bounds;
    assert!(approx_eq(label.x, 0.0));
    assert!(approx_eq(value.x, 200.0 - 8.4));
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
    // "Hello World Test" = 16 chars * 16 * 0.6 = 153.6 unwrapped width.
    // Container is 80 wide — should force word wrapping.
    // "Hello" = 48.0, "World" = 48.0, "Test" = 38.4
    // Line 1: "Hello World" = 48 + 9.6(space) + 48 = 105.6 > 80, so:
    // Line 1: "Hello" (48.0), Line 2: "World" (48.0), Line 3: "Test" (38.4)
    let mut b = LayoutBuilder::new(80.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("Hello World Test", LayoutTextStyle::new(16.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 200.0, 1.0);

    // Text element is index 2. Height should be 3 lines * 16 = 48.
    assert!(approx_eq(result.computed[2].height, 48.0));

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
    let mut b = LayoutBuilder::new(500.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text(
                "Line1\nLine2\nLine3",
                LayoutTextStyle::new(16.0).wrap(TextWrap::Newlines),
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 500.0, 200.0, 1.0);

    // 3 lines * 16 = 48.
    assert!(approx_eq(result.computed[2].height, 48.0));

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 3);
}

#[test]
fn word_wrap_long_word_does_not_break() {
    // "Supercalifragilistic" = 20 chars * 16 * 0.6 = 192.0 wide.
    // Container is only 80 wide. The word should NOT be broken — stays on one line.
    let mut b = LayoutBuilder::new(80.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("Supercalifragilistic", LayoutTextStyle::new(16.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 200.0, 1.0);

    // Should be a single line (word never broken mid-word).
    assert!(approx_eq(result.computed[2].height, 16.0));

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 1);
}

#[test]
fn word_wrap_preserves_explicit_newlines() {
    // "AA BB\nCC DD" with Words wrap in a container that fits "AA BB" but not all four.
    // "AA" = 19.2, "BB" = 19.2, space = 9.6. "AA BB" = 48.0.
    // Container is 50 wide — "AA BB" fits on one line, "CC DD" fits on one line.
    let mut b = LayoutBuilder::new(50.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("AA BB\nCC DD", LayoutTextStyle::new(16.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 50.0, 200.0, 1.0);

    // 2 paragraphs, each fits on one line = 2 lines * 16 = 32.
    assert!(approx_eq(result.computed[2].height, 32.0));

    let text_commands: Vec<_> = result
        .commands
        .iter()
        .filter(|cmd| matches!(cmd.kind, RenderCommandKind::Text { .. }))
        .collect();
    assert_eq!(text_commands.len(), 2);
}

#[test]
fn word_wrap_empty_string() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("", LayoutTextStyle::new(16.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 200.0, 200.0, 1.0);

    // Empty text produces one empty line — height = 1 * line_height = 16.
    assert!(approx_eq(result.computed[2].height, 16.0));
}

#[test]
fn word_wrap_updates_parent_fit_height() {
    // Parent is Fit-height, child text wraps to 3 lines.
    // Parent height should grow to accommodate.
    let mut b = LayoutBuilder::new(80.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom),
        |b| {
            b.text("Hello World Test", LayoutTextStyle::new(16.0));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 80.0, 200.0, 1.0);

    // Text wraps to 3 lines (see text_wraps_at_word_boundaries test).
    // Parent Fit height should be 48 (3 * 16).
    assert!(approx_eq(result.computed[1].height, 48.0));
}

#[test]
fn word_wrap_render_commands_per_line() {
    // Verify each wrapped line has correct bounds positioning.
    let mut b = LayoutBuilder::new(50.0, 200.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            // "AA BB CC" — each word ~19.2 wide, space ~9.6.
            // "AA BB" = 48.0 < 50, "CC" = 19.2 < 50.
            // Line 1: "AA BB" at y=0, Line 2: "CC" at y=16.
            b.text("AA BB CC", LayoutTextStyle::new(16.0));
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

    // Second line starts at y=16 (one line height down).
    assert!(approx_eq(text_commands[1].bounds.y, 16.0));
}

// ── Fit parent with Grow children propagation ─────────────────────────────

#[test]
fn fit_parent_sees_grow_children_content_height() {
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
    // "STATUS" = 6 chars * 7 * 0.6 = 25.2 wide, 7.0 tall.
    // "SUB" = 3 chars * 4 * 0.6 = 7.2 wide, 4.0 tall.
    // text_row Fit height should be max(7.0, 1.0, 4.0) = 7.0, NOT 1.0.
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
                        b.text("STATUS", LayoutTextStyle::new(7.0));
                    });
                    b.with(
                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                        |_| {},
                    );
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("SUB", LayoutTextStyle::new(4.0));
                    });
                },
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, 160.0, 160.0, 1.0);

    // text_row is index 2.
    let text_row_height = result.computed[2].height;
    assert!(
        text_row_height >= 7.0 - 0.01,
        "Fit text_row height should be >= 7.0 (text content), got {text_row_height}"
    );

    // text_row should be vertically centered in header_container.
    // header_container is 20.0 tall with 4+4=8 vertical padding → 12 content area.
    // Center offset = (12 - text_row_height) / 2 + 4 (top padding).
    let text_row_bounds = result.computed[2].bounds;
    let expected_y = (12.0 - text_row_height).mul_add(0.5, 4.0);
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
    // Content overflows the panel (18 + 4 + 248 = 270 > 134). Clay handles this
    // by keeping the content height and culling off-screen render commands rather
    // than compressing the body. This matches Clay's `CloseElement` behavior where
    // GROW elements are initialized from their children's content size.
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
    use super::element::Element;
    use super::element::ElementContent;
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

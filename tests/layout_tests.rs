#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::needless_collect)]

//! Layout engine integration tests.
//!
//! Each test constructs a layout tree, runs the engine, and verifies the computed
//! bounding boxes match expectations. A simple monospace text measurement function
//! is used throughout: each character is `font_size * 0.6` wide, one line tall.

use bevy_diegetic::layout::AlignX;
use bevy_diegetic::layout::AlignY;
use bevy_diegetic::layout::BackgroundColor;
use bevy_diegetic::layout::Border;
use bevy_diegetic::layout::Direction;
use bevy_diegetic::layout::El;
use bevy_diegetic::layout::LayoutBuilder;
use bevy_diegetic::layout::LayoutEngine;
use bevy_diegetic::layout::MeasureTextFn;
use bevy_diegetic::layout::Padding;
use bevy_diegetic::layout::RenderCommandKind;
use bevy_diegetic::layout::Sizing;
use bevy_diegetic::layout::TextConfig;
use bevy_diegetic::layout::TextDimensions;

const VIEWPORT: f32 = 200.0;

fn monospace_measure() -> MeasureTextFn {
    Box::new(|text: &str, config: &TextConfig| {
        let line_height = config.effective_line_height();
        let char_width = f32::from(config.font_size) * 0.6;
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
            width: max_line_width,
            height: line_height * line_count as f32,
        }
    })
}

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

// ── Fixed sizing ─────────────────────────────────────────────────────────────

#[test]
fn fixed_root_dimensions() {
    let tree = LayoutBuilder::new(100.0, 50.0).build();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

    assert!(approx_eq(result.computed[2].width, 60.0));
    assert!(approx_eq(result.computed[3].width, 140.0));
}

// ── Fit sizing ───────────────────────────────────────────────────────────────

#[test]
fn fit_wraps_text_content() {
    let mut b = LayoutBuilder::new(200.0, 200.0);
    // "Hello" = 5 chars * 16 * 0.6 = 48.0 wide, 16.0 tall.
    b.text("Hello", TextConfig::new(16));
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            b.text("Hello", TextConfig::new(16));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            .align_x(AlignX::Center),
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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            .align_x(AlignX::Right),
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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            .align_y(AlignY::Center),
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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            .align_y(AlignY::Bottom),
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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            // Both are Fit, so both get compressed.
            // Both at same size (60), so compressed evenly: 50 each.
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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

    // Children should be compressed to 50 each (indices 2 and 4, not 3 which is a grandchild).
    assert!(approx_eq(result.computed[2].width, 50.0));
    assert!(approx_eq(result.computed[4].width, 50.0));
}

// ── Render commands ──────────────────────────────────────────────────────────

#[test]
fn render_commands_include_rectangles() {
    let mut b = LayoutBuilder::new(100.0, 100.0);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(BackgroundColor::rgb(255, 0, 0)),
        |_| {},
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    b.text("Hello", TextConfig::new(16));
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            .border(Border::all(2.0, BackgroundColor::rgb(255, 255, 255))),
        |_| {},
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
                    .background(BackgroundColor::rgb(52, 98, 90)),
                |b| {
                    b.text("STATUS", TextConfig::new(7));
                },
            );
            // Divider.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(4.0))
                    .background(BackgroundColor::rgb(74, 196, 172)),
                |_| {},
            );
            // Body: grows to fill remaining space.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .background(BackgroundColor::rgb(22, 28, 34)),
                |_| {},
            );
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            b.text("Hello", TextConfig::new(16));
            b.text("World", TextConfig::new(16));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            b.text("fps:", TextConfig::new(7));
            b.with(
                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                |_| {},
            );
            b.text("60", TextConfig::new(7));
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
    let tree = bevy_diegetic::layout::LayoutTree::new();
    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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
            .border(Border {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
                color: BackgroundColor::rgb(255, 255, 255),
                between_children: 2.0,
            }),
        |b| {
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
    let tree = b.build();

    let engine = LayoutEngine::new(monospace_measure());
    let result = engine.compute(&tree, VIEWPORT, VIEWPORT);

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

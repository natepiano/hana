use bevy::math::Vec2;
use bevy_kana::ToF32;

use super::layout_engine::ComputedLayout;
use super::sizing;
use super::sizing::Axis;
use super::wrapping::WrappedText;
use crate::layout::AlignX;
use crate::layout::AlignY;
use crate::layout::Border;
use crate::layout::BoundingBox;
use crate::layout::Direction;
use crate::layout::TextAlign;
use crate::layout::TextStyle;
use crate::layout::element::ChildOverflow;
use crate::layout::element::Element;
use crate::layout::element::ElementContent;
use crate::layout::element::LayoutTree;
use crate::layout::element::ScrollAnchor;
use crate::layout::render::RectangleSource;
use crate::layout::render::RenderCommand;
use crate::layout::render::RenderCommandKind;

/// Emits border and scissor-end commands during the up-traversal (second visit)
/// of the DFS positioning pass.
fn emit_up_traversal_commands(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    commands: &mut Vec<RenderCommand>,
    element: &Element,
    bounds: BoundingBox,
    index: usize,
) {
    if let Some(ref border) = element.border {
        commands.push(RenderCommand {
            bounds,
            kind: RenderCommandKind::Border { border: *border },
            element_idx: index,
        });

        // Between-children borders.
        if border.between_children.value > 0.0 {
            emit_between_borders(tree, computed, commands, index, border);
        }
    }

    if matches!(element.overflow, ChildOverflow::Clipped) {
        commands.push(RenderCommand {
            bounds,
            kind: RenderCommandKind::ScissorEnd,
            element_idx: index,
        });
    }
}

/// Emits background, scissor-start, and text render commands during the
/// down-traversal (first visit) of the DFS positioning pass.
fn emit_down_traversal_commands(
    commands: &mut Vec<RenderCommand>,
    element: &Element,
    wrapped: Option<&WrappedText>,
    bounds: BoundingBox,
    index: usize,
    font_scale: f32,
) {
    // Emit rectangle if background is set.
    if let Some(color) = element.background {
        commands.push(RenderCommand {
            bounds,
            kind: RenderCommandKind::Rectangle {
                color,
                source: RectangleSource::Background,
            },
            element_idx: index,
        });
    }

    // Emit scissor start if clipping (always emit — scissor regions
    // must be balanced even when the parent is off-screen).
    // Clip to the border's inner edge — content can fill up to (but
    // not into) the border. Padding is inside this region.
    if matches!(element.overflow, ChildOverflow::Clipped) {
        let bt = element.border.as_ref().map_or(0.0, |b| b.top.value);
        let br = element.border.as_ref().map_or(0.0, |b| b.right.value);
        let bb = element.border.as_ref().map_or(0.0, |b| b.bottom.value);
        let bl = element.border.as_ref().map_or(0.0, |b| b.left.value);
        let clip_bounds = BoundingBox {
            x:      bounds.x + bl,
            y:      bounds.y + bt,
            width:  (bounds.width - bl - br).max(0.0),
            height: (bounds.height - bt - bb).max(0.0),
        };
        commands.push(RenderCommand {
            bounds:      clip_bounds,
            kind:        RenderCommandKind::ScissorStart,
            element_idx: index,
        });
    }

    // Emit text render commands.
    if let ElementContent::Text {
        ref config,
        ref text,
        ..
    } = element.content
    {
        emit_text_commands(commands, wrapped, config, text, bounds, index, font_scale);
    }

    // Emit image render commands.
    if let ElementContent::Image { ref handle, tint } = element.content {
        commands.push(RenderCommand {
            bounds,
            kind: RenderCommandKind::Image {
                handle: handle.clone(),
                tint,
            },
            element_idx: index,
        });
    }
}

/// Emits render commands for text content (both wrapped and unwrapped).
fn emit_text_commands(
    commands: &mut Vec<RenderCommand>,
    wrapped: Option<&WrappedText>,
    config: &TextStyle,
    text: &str,
    bounds: BoundingBox,
    index: usize,
    font_scale: f32,
) {
    // Render commands store font sizes in layout units so downstream
    // renderers don't need to know about the font unit conversion.
    let scaled_config = config.scaled(font_scale);

    if let Some(wrap_result) = wrapped {
        // Wrapped text: emit one command per line.
        for (line_idx, line) in wrap_result.lines.iter().enumerate() {
            let line_y = wrap_result.line_height.mul_add(line_idx.to_f32(), bounds.y);
            let line_x = line_x_for_alignment(config.text_align(), bounds, line.width);
            commands.push(RenderCommand {
                bounds:      BoundingBox {
                    x:      line_x,
                    y:      line_y,
                    width:  line.width,
                    height: wrap_result.line_height,
                },
                kind:        RenderCommandKind::Text {
                    text:   line.text.clone(),
                    config: scaled_config.clone(),
                },
                element_idx: index,
            });
        }
    } else {
        // Unwrapped text (`TextWrap::None`): single command.
        commands.push(RenderCommand {
            bounds,
            kind: RenderCommandKind::Text {
                text:   text.to_owned(),
                config: scaled_config,
            },
            element_idx: index,
        });
    }
}

fn line_x_for_alignment(align: TextAlign, bounds: BoundingBox, line_width: f32) -> f32 {
    match align {
        TextAlign::Left => bounds.x,
        TextAlign::Center => (bounds.width - line_width).mul_add(0.5, bounds.x),
        TextAlign::Right => bounds.x + bounds.width - line_width,
    }
}

/// Resolves the clamped per-axis scroll offset for a scrolling parent, returning
/// `(0, 0)` for the common non-scrolling case. Each axis clamps to
/// `[0, content - viewport]`; `End`-anchored axes measure the offset from the
/// far edge so `0` pins to the bottom/right.
fn resolve_scroll_offset(
    parent_el: &Element,
    computed: &[ComputedLayout],
    children: &[usize],
    is_horizontal: bool,
    main_available: f32,
    cross_available: f32,
    content_main: f32,
) -> (f32, f32) {
    // A zero `Start` offset is a no-op; a zero `End` offset still resolves
    // (scrollback 0 pins to the end), so don't short-circuit it.
    if parent_el.scroll_offset == Vec2::ZERO && parent_el.scroll_anchor == ScrollAnchor::Start {
        return (0.0, 0.0);
    }

    let mut content_cross: f32 = 0.0;
    for &idx in children {
        let cross = if is_horizontal {
            computed[idx].height
        } else {
            computed[idx].width
        };
        content_cross = content_cross.max(cross);
    }

    let max_main = (content_main - main_available).max(0.0);
    let max_cross = (content_cross - cross_available).max(0.0);
    let (max_x, max_y) = if is_horizontal {
        (max_main, max_cross)
    } else {
        (max_cross, max_main)
    };
    let resolve = |offset: f32, max: f32| match parent_el.scroll_anchor {
        ScrollAnchor::Start => offset.clamp(0.0, max),
        ScrollAnchor::End => (max - offset).clamp(0.0, max),
    };
    (
        resolve(parent_el.scroll_offset.x, max_x),
        resolve(parent_el.scroll_offset.y, max_y),
    )
}

/// Pushes children onto the DFS stack in reverse order with computed positions.
///
/// Children are pushed in reverse so the first child is processed first during
/// iteration. Uses a reverse cursor to compute positions without allocation.
fn push_children_to_stack(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    stack: &mut Vec<(usize, f32, f32, bool)>,
    index: usize,
    x: f32,
    y: f32,
) {
    let children = tree.children_of(index);
    if children.is_empty() {
        return;
    }

    let parent_el = &tree.elements[index];
    let parent_width = computed[index].width;
    let parent_height = computed[index].height;
    let is_horizontal = parent_el.direction == Direction::LeftToRight;

    let mut children_main_size: f32 = 0.0;
    for &idx in children {
        children_main_size += if is_horizontal {
            computed[idx].width
        } else {
            computed[idx].height
        };
    }

    let gap_total = if children.len() > 1 {
        parent_el.child_gap.value * (children.len() - 1).to_f32()
    } else {
        0.0
    };

    let border_x = sizing::border_inset(parent_el, Axis::X);
    let border_y = sizing::border_inset(parent_el, Axis::Y);
    let border_left = sizing::border_leading(parent_el, Axis::X);
    let border_top = sizing::border_leading(parent_el, Axis::Y);

    let main_available = if is_horizontal {
        parent_width - parent_el.padding.horizontal() - border_x
    } else {
        parent_height - parent_el.padding.vertical() - border_y
    };

    let content_main = children_main_size + gap_total;
    let extra_main = (main_available - content_main).max(0.0);

    let cross_available = if is_horizontal {
        parent_height - parent_el.padding.vertical() - border_y
    } else {
        parent_width - parent_el.padding.horizontal() - border_x
    };
    // Offset subtracted from child positions so a clipping parent scrolls its
    // children; the element's scissor rect stays fixed, so shifted-out content
    // clips.
    let (scroll_x, scroll_y) = resolve_scroll_offset(
        parent_el,
        computed,
        children,
        is_horizontal,
        main_available,
        cross_available,
        content_main,
    );

    let main_offset = if is_horizontal {
        match parent_el.child_align_x {
            AlignX::Left => 0.0,
            AlignX::Center => extra_main * 0.5,
            AlignX::Right => extra_main,
        }
    } else {
        match parent_el.child_align_y {
            AlignY::Top => 0.0,
            AlignY::Center => extra_main * 0.5,
            AlignY::Bottom => extra_main,
        }
    };

    // Walk children in reverse, subtracting each child's main size
    // from the cursor to produce positions in stack-push order.
    let mut reverse_cursor = main_offset + children_main_size + gap_total;
    for &child_idx in children.iter().rev() {
        let child_width = computed[child_idx].width;
        let child_height = computed[child_idx].height;
        let child_main = if is_horizontal {
            child_width
        } else {
            child_height
        };

        reverse_cursor -= child_main;

        let (cx, cy) = if is_horizontal {
            let cross_available = parent_height - parent_el.padding.vertical() - border_y;
            let cross_offset = match parent_el.child_align_y {
                AlignY::Top => 0.0,
                AlignY::Center => (cross_available - child_height).max(0.0) * 0.5,
                AlignY::Bottom => (cross_available - child_height).max(0.0),
            };
            (
                x + parent_el.padding.left.value + border_left + reverse_cursor - scroll_x,
                y + parent_el.padding.top.value + border_top + cross_offset - scroll_y,
            )
        } else {
            let cross_available = parent_width - parent_el.padding.horizontal() - border_x;
            let cross_offset = match parent_el.child_align_x {
                AlignX::Left => 0.0,
                AlignX::Center => (cross_available - child_width).max(0.0) * 0.5,
                AlignX::Right => (cross_available - child_width).max(0.0),
            };
            (
                x + parent_el.padding.left.value + border_left + cross_offset - scroll_x,
                y + parent_el.padding.top.value + border_top + reverse_cursor - scroll_y,
            )
        };

        stack.push((child_idx, cx, cy, false));
        reverse_cursor -= parent_el.child_gap.value;
    }
}

/// DFS positioning pass. Computes final bounding boxes and emits render commands.
pub(super) fn position_and_render(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    root: usize,
    wrapped: &[Option<WrappedText>],
    _viewport_width: f32,
    _viewport_height: f32,
    font_scale: f32,
) -> Vec<RenderCommand> {
    let mut commands = Vec::with_capacity(tree.len() * 2);

    // Stack entries: (element_index, x, y, is_second_visit)
    let mut stack: Vec<(usize, f32, f32, bool)> = Vec::with_capacity(tree.len());
    stack.push((root, 0.0, 0.0, false));

    while let Some(&mut (index, x, y, ref mut visited)) = stack.last_mut() {
        let element = &tree.elements[index];
        let bounds = BoundingBox {
            x,
            y,
            width: computed[index].width,
            height: computed[index].height,
        };

        if *visited {
            emit_up_traversal_commands(tree, computed, &mut commands, element, bounds, index);
            stack.pop();
        } else {
            *visited = true;

            // Store the final bounding box for render-side culling and clipping.
            computed[index].bounds = bounds;

            emit_down_traversal_commands(
                &mut commands,
                element,
                wrapped[index].as_ref(),
                bounds,
                index,
                font_scale,
            );

            push_children_to_stack(tree, computed, &mut stack, index, x, y);
        }
    }
    commands
}

/// Generates render commands from already-computed element bounds.
pub(super) fn render_commands_from_geometry(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    root: usize,
    wrapped: &[Option<WrappedText>],
    _viewport_width: f32,
    _viewport_height: f32,
    font_scale: f32,
) -> Vec<RenderCommand> {
    let mut commands = Vec::with_capacity(tree.len() * 2);

    // Stack entries: (element_index, is_second_visit)
    let mut stack: Vec<(usize, bool)> = Vec::with_capacity(tree.len());
    stack.push((root, false));

    while let Some((index, visited)) = stack.pop() {
        let element = &tree.elements[index];
        let bounds = computed[index].bounds;

        if visited {
            emit_up_traversal_commands(tree, computed, &mut commands, element, bounds, index);
            continue;
        }

        emit_down_traversal_commands(
            &mut commands,
            element,
            wrapped[index].as_ref(),
            bounds,
            index,
            font_scale,
        );

        stack.push((index, true));
        for &child_idx in tree.children_of(index).iter().rev() {
            stack.push((child_idx, false));
        }
    }

    commands
}

/// Emit border-between-children rectangles.
///
/// Uses children's already-computed bounds (set during DFS first visit)
/// to avoid re-computing positions.
fn emit_between_borders(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    commands: &mut Vec<RenderCommand>,
    parent_idx: usize,
    border: &Border,
) {
    let parent = &tree.elements[parent_idx];
    let parent_bounds = computed[parent_idx].bounds;
    let children = tree.children_of(parent_idx);

    if children.len() < 2 {
        return;
    }

    let is_horizontal = parent.direction == Direction::LeftToRight;

    // Draw a line between each pair of adjacent children.
    for pair in children.windows(2) {
        let a_bounds = computed[pair[0]].bounds;
        let b_bounds = computed[pair[1]].bounds;

        if is_horizontal {
            let midpoint = (b_bounds.x - (a_bounds.x + a_bounds.width))
                .mul_add(0.5, a_bounds.x + a_bounds.width);
            let line_x = border.between_children.value.mul_add(-0.5, midpoint);
            commands.push(RenderCommand {
                bounds:      BoundingBox {
                    x:      line_x,
                    y:      parent_bounds.y + parent.padding.top.value,
                    width:  border.between_children.value,
                    height: parent_bounds.height - parent.padding.vertical(),
                },
                kind:        RenderCommandKind::Rectangle {
                    color:  border.color,
                    source: RectangleSource::BetweenChildrenBorder,
                },
                element_idx: parent_idx,
            });
        } else {
            let midpoint = (b_bounds.y - (a_bounds.y + a_bounds.height))
                .mul_add(0.5, a_bounds.y + a_bounds.height);
            let line_y = border.between_children.value.mul_add(-0.5, midpoint);
            commands.push(RenderCommand {
                bounds:      BoundingBox {
                    x:      parent_bounds.x + parent.padding.left.value,
                    y:      line_y,
                    width:  parent_bounds.width - parent.padding.horizontal(),
                    height: border.between_children.value,
                },
                kind:        RenderCommandKind::Rectangle {
                    color:  border.color,
                    source: RectangleSource::BetweenChildrenBorder,
                },
                element_idx: parent_idx,
            });
        }
    }
}

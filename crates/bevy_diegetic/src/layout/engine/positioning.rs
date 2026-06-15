use bevy::math::Vec2;
use bevy_kana::ToF32;

use super::layout_engine::ComputedLayout;
use super::sizing;
use super::sizing::Axis;
use super::wrapping::WrappedText;
use crate::layout::AlignX;
use crate::layout::AlignY;
use crate::layout::BoundingBox;
use crate::layout::ChildDivider;
use crate::layout::DrawOverflow;
use crate::layout::DrawZIndex;
use crate::layout::PanelLineSourceKey;
use crate::layout::ResolvedPanelLine;
use crate::layout::TextAlign;
use crate::layout::TextStyle;
use crate::layout::element::ChildOverflow;
use crate::layout::element::Element;
use crate::layout::element::ElementContent;
use crate::layout::element::LayoutTree;
use crate::layout::element::ScrollAnchor;
use crate::layout::line;
use crate::layout::line::PanelLineClipPolicy;
use crate::layout::line::PanelLineResolveContext;
use crate::layout::render::RectangleSource;
use crate::layout::render::RenderCommand;
use crate::layout::render::RenderCommandKind;

/// Pushes one command with its [`RenderCommand::z_index`].
fn push_command(
    commands: &mut Vec<RenderCommand>,
    bounds: BoundingBox,
    kind: RenderCommandKind,
    element_idx: usize,
    z_index: DrawZIndex,
) {
    commands.push(RenderCommand {
        bounds,
        kind,
        element_idx,
        z_index,
    });
}

#[derive(Clone, Copy)]
struct PositionStackEntry {
    index:        usize,
    x:            f32,
    y:            f32,
    visited:      bool,
    clip_context: ClipContext,
}

#[derive(Clone, Copy)]
struct GeometryStackEntry {
    index:        usize,
    visited:      bool,
    clip_context: ClipContext,
}

/// Clip state threaded down the DFS. `inherited` starts at the viewport and
/// narrows at every [`ChildOverflow::Clipped`] ancestor — it clips owner-bound
/// content. `scissor` carries only the [`ChildOverflow::Clipped`] ancestor
/// intersections (no viewport seed), so overflow-visible panel lines escape
/// the panel viewport but still respect explicit scissor regions.
#[derive(Clone, Copy)]
struct ClipContext {
    inherited: BoundingBox,
    scissor:   Option<BoundingBox>,
}

impl ClipContext {
    const fn root(viewport: BoundingBox) -> Self {
        Self {
            inherited: viewport,
            scissor:   None,
        }
    }

    fn child(self, element: &Element, bounds: BoundingBox) -> Self {
        if matches!(element.overflow, ChildOverflow::Clipped) {
            let scissor_bounds = element_scissor_bounds(element, bounds);
            Self {
                inherited: self
                    .inherited
                    .intersect(&scissor_bounds)
                    .unwrap_or_else(empty_clip),
                scissor:   Some(match self.scissor {
                    Some(scissor) => scissor
                        .intersect(&scissor_bounds)
                        .unwrap_or_else(empty_clip),
                    None => scissor_bounds,
                }),
            }
        } else {
            self
        }
    }
}

#[derive(Clone, Copy)]
struct ChildStackContext<'a> {
    parent:               &'a Element,
    parent_size:          Vec2,
    is_horizontal:        bool,
    border_x:             f32,
    border_y:             f32,
    border_left:          f32,
    border_top:           f32,
    scroll_x:             f32,
    scroll_y:             f32,
    reverse_cursor_start: f32,
    clip_context:         ClipContext,
}

impl ChildStackContext<'_> {
    const fn child_main_size(&self, child_size: Vec2) -> f32 {
        if self.is_horizontal {
            child_size.x
        } else {
            child_size.y
        }
    }

    fn child_position(&self, origin: Vec2, reverse_cursor: f32, child_size: Vec2) -> Vec2 {
        let parent = self.parent;
        let base_x = origin.x + parent.padding.left.value + self.border_left - self.scroll_x;
        let base_y = origin.y + parent.padding.top.value + self.border_top - self.scroll_y;
        if self.is_horizontal {
            let cross_available = self.parent_size.y - parent.padding.vertical() - self.border_y;
            let cross_offset = match parent.child_layout.align_y() {
                AlignY::Top => 0.0,
                AlignY::Center => (cross_available - child_size.y).max(0.0) * 0.5,
                AlignY::Bottom => (cross_available - child_size.y).max(0.0),
            };
            Vec2::new(base_x + reverse_cursor, base_y + cross_offset)
        } else {
            let cross_available = self.parent_size.x - parent.padding.horizontal() - self.border_x;
            let cross_offset = match parent.child_layout.align_x() {
                AlignX::Left => 0.0,
                AlignX::Center => (cross_available - child_size.x).max(0.0) * 0.5,
                AlignX::Right => (cross_available - child_size.x).max(0.0),
            };
            Vec2::new(base_x + cross_offset, base_y + reverse_cursor)
        }
    }
}

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
        push_command(
            commands,
            bounds,
            RenderCommandKind::Border { border: *border },
            index,
            element.z_index,
        );
    }

    if let Some(divider) = element.child_layout.divider()
        && divider.width().value > 0.0
    {
        emit_child_dividers(tree, computed, commands, index, divider);
    }

    if matches!(element.overflow, ChildOverflow::Clipped) {
        push_command(
            commands,
            bounds,
            RenderCommandKind::ScissorEnd,
            index,
            element.z_index,
        );
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
    clip_context: ClipContext,
) {
    // Emit rectangle if background is set.
    if let Some(color) = element.background {
        push_command(
            commands,
            bounds,
            RenderCommandKind::Rectangle {
                color,
                source: RectangleSource::Background,
            },
            index,
            element.z_index,
        );
    }

    // Emit scissor start if clipping (always emit — scissor regions
    // must be balanced even when the parent is off-screen).
    // Clip to the border's inner edge — content can fill up to (but
    // not into) the border. Padding is inside this region.
    if matches!(element.overflow, ChildOverflow::Clipped) {
        let clip_bounds = element_scissor_bounds(element, bounds);
        push_command(
            commands,
            clip_bounds,
            RenderCommandKind::ScissorStart,
            index,
            element.z_index,
        );
    }

    emit_line_commands(commands, element, bounds, index, clip_context);

    // Emit text render commands.
    if let ElementContent::Text {
        ref config,
        ref text,
        ..
    } = element.content
    {
        emit_text_commands(
            commands,
            wrapped,
            config,
            text,
            bounds,
            index,
            font_scale,
            element.z_index,
        );
    }

    // Emit image render commands.
    if let ElementContent::Image { ref handle, tint } = element.content {
        push_command(
            commands,
            bounds,
            RenderCommandKind::Image {
                handle: handle.clone(),
                tint,
            },
            index,
            element.z_index,
        );
    }
}

fn emit_line_commands(
    commands: &mut Vec<RenderCommand>,
    element: &Element,
    bounds: BoundingBox,
    index: usize,
    clip_context: ClipContext,
) {
    let Some(panel_draw) = element.draw.as_ref() else {
        return;
    };
    if panel_draw.lines_ref().is_empty() {
        return;
    }

    let source_command_index = commands.len();
    let (clip, clip_policy) = match panel_draw.overflow_policy() {
        DrawOverflow::Clipped => (
            Some(clip_context.inherited),
            PanelLineClipPolicy::OwnerBounds,
        ),
        DrawOverflow::Visible => (clip_context.scissor, PanelLineClipPolicy::Inherited),
    };

    let mut lines = Vec::new();
    for (line_ordinal, line) in panel_draw.lines_ref().iter().enumerate() {
        let source_key = PanelLineSourceKey::element(index, 0, line_ordinal);
        let context = PanelLineResolveContext::new(
            bounds,
            clip,
            clip_policy,
            source_command_index,
            source_key,
        );
        if let Some(resolved_line) = line::resolve_panel_line(line, context) {
            lines.push(resolved_line);
        }
    }

    let Some(command_bounds) = line_command_bounds(&lines) else {
        return;
    };
    push_command(
        commands,
        command_bounds,
        RenderCommandKind::Lines { lines },
        index,
        element.z_index,
    );
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
    z_index: DrawZIndex,
) {
    // Render commands store font sizes in layout units so downstream
    // renderers don't need to know about the font unit conversion.
    let scaled_config = config.scaled(font_scale);

    if let Some(wrap_result) = wrapped {
        // Wrapped text: emit one command per line.
        for (line_idx, line) in wrap_result.lines.iter().enumerate() {
            let line_y = wrap_result.line_height.mul_add(line_idx.to_f32(), bounds.y);
            let line_x = line_x_for_alignment(config.text_align(), bounds, line.width);
            push_command(
                commands,
                BoundingBox {
                    x:      line_x,
                    y:      line_y,
                    width:  line.width,
                    height: wrap_result.line_height,
                },
                RenderCommandKind::Text {
                    text:   line.text.clone(),
                    config: scaled_config.clone(),
                },
                index,
                z_index,
            );
        }
    } else {
        // Unwrapped text (`TextWrap::None`): single command.
        push_command(
            commands,
            bounds,
            RenderCommandKind::Text {
                text:   text.to_owned(),
                config: scaled_config,
            },
            index,
            z_index,
        );
    }
}

fn line_x_for_alignment(align: TextAlign, bounds: BoundingBox, line_width: f32) -> f32 {
    match align {
        TextAlign::Left => bounds.x,
        TextAlign::Center => (bounds.width - line_width).mul_add(0.5, bounds.x),
        TextAlign::Right => bounds.x + bounds.width - line_width,
    }
}

fn line_command_bounds(lines: &[ResolvedPanelLine]) -> Option<BoundingBox> {
    let mut iter = lines.iter();
    let first = iter.next()?.visual_bounds;
    Some(iter.fold(first, |bounds, line| {
        union_bounds(bounds, line.visual_bounds)
    }))
}

fn union_bounds(a: BoundingBox, b: BoundingBox) -> BoundingBox {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    BoundingBox {
        x:      x0,
        y:      y0,
        width:  x1 - x0,
        height: y1 - y0,
    }
}

fn element_scissor_bounds(element: &Element, bounds: BoundingBox) -> BoundingBox {
    let top = element
        .border
        .as_ref()
        .map_or(0.0, |border| border.top.value);
    let right = element
        .border
        .as_ref()
        .map_or(0.0, |border| border.right.value);
    let bottom = element
        .border
        .as_ref()
        .map_or(0.0, |border| border.bottom.value);
    let left = element
        .border
        .as_ref()
        .map_or(0.0, |border| border.left.value);
    BoundingBox {
        x:      bounds.x + left,
        y:      bounds.y + top,
        width:  (bounds.width - left - right).max(0.0),
        height: (bounds.height - top - bottom).max(0.0),
    }
}

const fn empty_clip() -> BoundingBox {
    BoundingBox {
        x:      0.0,
        y:      0.0,
        width:  0.0,
        height: 0.0,
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

fn child_stack_context<'a>(
    parent: &'a Element,
    computed: &[ComputedLayout],
    children: &[usize],
    parent_size: Vec2,
    is_horizontal: bool,
    clip_context: ClipContext,
) -> ChildStackContext<'a> {
    let children_main_size = children.iter().fold(0.0, |main_size, &idx| {
        main_size
            + if is_horizontal {
                computed[idx].width
            } else {
                computed[idx].height
            }
    });
    let gap_total = if children.len() > 1 {
        parent.child_layout.gap().value * (children.len() - 1).to_f32()
    } else {
        0.0
    };
    let border_x = sizing::border_inset(parent, Axis::X);
    let border_y = sizing::border_inset(parent, Axis::Y);
    let border_left = sizing::border_leading(parent, Axis::X);
    let border_top = sizing::border_leading(parent, Axis::Y);
    let main_available = if is_horizontal {
        parent_size.x - parent.padding.horizontal() - border_x
    } else {
        parent_size.y - parent.padding.vertical() - border_y
    };
    let content_main = children_main_size + gap_total;
    let extra_main = (main_available - content_main).max(0.0);
    let cross_available = if is_horizontal {
        parent_size.y - parent.padding.vertical() - border_y
    } else {
        parent_size.x - parent.padding.horizontal() - border_x
    };
    let (scroll_x, scroll_y) = resolve_scroll_offset(
        parent,
        computed,
        children,
        is_horizontal,
        main_available,
        cross_available,
        content_main,
    );
    let main_offset = if is_horizontal {
        match parent.child_layout.align_x() {
            AlignX::Left => 0.0,
            AlignX::Center => extra_main * 0.5,
            AlignX::Right => extra_main,
        }
    } else {
        match parent.child_layout.align_y() {
            AlignY::Top => 0.0,
            AlignY::Center => extra_main * 0.5,
            AlignY::Bottom => extra_main,
        }
    };
    ChildStackContext {
        parent,
        parent_size,
        is_horizontal,
        border_x,
        border_y,
        border_left,
        border_top,
        scroll_x,
        scroll_y,
        reverse_cursor_start: main_offset + children_main_size + gap_total,
        clip_context,
    }
}

/// Pushes children onto the DFS stack in reverse order with computed positions.
///
/// Children are pushed in reverse so the first child is processed first during
/// iteration. Uses a reverse cursor to compute positions without allocation.
fn push_children_to_stack(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    stack: &mut Vec<PositionStackEntry>,
    index: usize,
    x: f32,
    y: f32,
    clip_context: ClipContext,
) {
    let children = tree.children_of(index);
    if children.is_empty() {
        return;
    }

    let parent_el = &tree.elements[index];
    let is_horizontal = parent_el.child_layout.is_row();
    let child_context = child_stack_context(
        parent_el,
        computed,
        children,
        Vec2::new(computed[index].width, computed[index].height),
        is_horizontal,
        clip_context,
    );

    // Walk children in reverse, subtracting each child's main size
    // from the cursor to produce positions in stack-push order.
    let origin = Vec2::new(x, y);
    let mut reverse_cursor = child_context.reverse_cursor_start;
    for &child_idx in children.iter().rev() {
        let child_size = Vec2::new(computed[child_idx].width, computed[child_idx].height);
        let child_main = child_context.child_main_size(child_size);
        reverse_cursor -= child_main;
        let child_position = child_context.child_position(origin, reverse_cursor, child_size);

        stack.push(PositionStackEntry {
            index:        child_idx,
            x:            child_position.x,
            y:            child_position.y,
            visited:      false,
            clip_context: child_context.clip_context,
        });
        reverse_cursor -= parent_el.child_layout.gap().value;
    }
}

/// DFS positioning pass. Computes final bounding boxes and emits render commands.
pub(super) fn position_and_render(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    root: usize,
    wrapped: &[Option<WrappedText>],
    viewport_width: f32,
    viewport_height: f32,
    font_scale: f32,
) -> Vec<RenderCommand> {
    let mut commands = Vec::with_capacity(tree.len() * 2);
    let viewport_clip = BoundingBox {
        x:      0.0,
        y:      0.0,
        width:  viewport_width,
        height: viewport_height,
    };

    let mut stack = Vec::with_capacity(tree.len());
    stack.push(PositionStackEntry {
        index:        root,
        x:            0.0,
        y:            0.0,
        visited:      false,
        clip_context: ClipContext::root(viewport_clip),
    });

    while let Some(entry) = stack.pop() {
        let index = entry.index;
        let element = &tree.elements[index];
        let bounds = BoundingBox {
            x:      entry.x,
            y:      entry.y,
            width:  computed[index].width,
            height: computed[index].height,
        };

        if entry.visited {
            emit_up_traversal_commands(tree, computed, &mut commands, element, bounds, index);
        } else {
            // Store the final bounding box for render-side culling and clipping.
            computed[index].bounds = bounds;

            emit_down_traversal_commands(
                &mut commands,
                element,
                wrapped[index].as_ref(),
                bounds,
                index,
                font_scale,
                entry.clip_context,
            );

            let child_clip = entry.clip_context.child(element, bounds);
            stack.push(PositionStackEntry {
                visited: true,
                ..entry
            });
            push_children_to_stack(
                tree, computed, &mut stack, index, entry.x, entry.y, child_clip,
            );
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
    viewport_width: f32,
    viewport_height: f32,
    font_scale: f32,
) -> Vec<RenderCommand> {
    let mut commands = Vec::with_capacity(tree.len() * 2);
    let viewport_clip = BoundingBox {
        x:      0.0,
        y:      0.0,
        width:  viewport_width,
        height: viewport_height,
    };

    let mut stack = Vec::with_capacity(tree.len());
    stack.push(GeometryStackEntry {
        index:        root,
        visited:      false,
        clip_context: ClipContext::root(viewport_clip),
    });

    while let Some(entry) = stack.pop() {
        let index = entry.index;
        let element = &tree.elements[index];
        let bounds = computed[index].bounds;

        if entry.visited {
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
            entry.clip_context,
        );

        let child_clip = entry.clip_context.child(element, bounds);
        stack.push(GeometryStackEntry {
            visited: true,
            ..entry
        });
        for &child_idx in tree.children_of(index).iter().rev() {
            stack.push(GeometryStackEntry {
                index:        child_idx,
                visited:      false,
                clip_context: child_clip,
            });
        }
    }

    commands
}

/// Emits child-divider rectangles.
///
/// Uses children's already-computed bounds (set during DFS first visit)
/// to avoid re-computing positions.
fn emit_child_dividers(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    commands: &mut Vec<RenderCommand>,
    parent_idx: usize,
    divider: ChildDivider,
) {
    let parent = &tree.elements[parent_idx];
    let parent_bounds = computed[parent_idx].bounds;
    let children = tree.children_of(parent_idx);

    if children.len() < 2 {
        return;
    }

    let is_horizontal = parent.child_layout.is_row();
    let width = divider.width().value;
    let color = divider.color();

    // Draw a line between each pair of adjacent children.
    for pair in children.windows(2) {
        let a_bounds = computed[pair[0]].bounds;
        let b_bounds = computed[pair[1]].bounds;

        if is_horizontal {
            let midpoint = (b_bounds.x - (a_bounds.x + a_bounds.width))
                .mul_add(0.5, a_bounds.x + a_bounds.width);
            let line_x = width.mul_add(-0.5, midpoint);
            push_command(
                commands,
                BoundingBox {
                    x: line_x,
                    y: parent_bounds.y + parent.padding.top.value,
                    width,
                    height: parent_bounds.height - parent.padding.vertical(),
                },
                RenderCommandKind::Rectangle {
                    color,
                    source: RectangleSource::ChildDivider,
                },
                parent_idx,
                parent.z_index,
            );
        } else {
            let midpoint = (b_bounds.y - (a_bounds.y + a_bounds.height))
                .mul_add(0.5, a_bounds.y + a_bounds.height);
            let line_y = width.mul_add(-0.5, midpoint);
            push_command(
                commands,
                BoundingBox {
                    x:      parent_bounds.x + parent.padding.left.value,
                    y:      line_y,
                    width:  parent_bounds.width - parent.padding.horizontal(),
                    height: width,
                },
                RenderCommandKind::Rectangle {
                    color,
                    source: RectangleSource::ChildDivider,
                },
                parent_idx,
                parent.z_index,
            );
        }
    }
}

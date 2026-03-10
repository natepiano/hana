//! Core layout computation engine.
//!
//! Implements a Clay-inspired two-pass layout algorithm:
//! 1. **Sizing pass** — BFS traversal determines element dimensions (called twice: X then Y).
//! 2. **Positioning pass** — DFS traversal computes final positions and emits render commands.
//!
//! The engine is fully self-contained with no global state. Multiple instances can run
//! concurrently on different threads without interference.

use super::element::Element;
use super::element::ElementContent;
use super::element::LayoutTree;
use super::render::RenderCommand;
use super::render::RenderCommandKind;
use super::types::AlignX;
use super::types::AlignY;
use super::types::Border;
use super::types::BoundingBox;
use super::types::Direction;
use super::types::Sizing;
use super::types::TextConfig;
use super::types::TextDimensions;

/// Callback type for measuring text dimensions.
///
/// Given a text string and its configuration, returns the measured dimensions
/// in layout units. The layout engine calls this during sizing to determine
/// how much space text elements need.
pub type MeasureTextFn = Box<dyn Fn(&str, &TextConfig) -> TextDimensions>;

/// Computed layout data for a single element.
#[derive(Clone, Copy, Debug, Default)]
pub struct ComputedLayout {
    /// Final bounding box in layout coordinates.
    pub bounds: BoundingBox,
    /// Resolved width before positioning.
    pub width: f32,
    /// Resolved height before positioning.
    pub height: f32,
}

/// The layout engine. Thread-safe, no global state.
///
/// # Usage
///
/// ```ignore
/// let mut engine = LayoutEngine::new(measure_fn);
/// let commands = engine.compute(&tree, 800.0, 600.0);
/// ```
pub struct LayoutEngine {
    measure_text: MeasureTextFn,
}

impl LayoutEngine {
    /// Creates a new layout engine with the given text measurement callback.
    #[must_use]
    pub fn new(measure_text: MeasureTextFn) -> Self {
        Self { measure_text }
    }

    /// Computes layout for the given tree within the specified viewport dimensions.
    ///
    /// Returns a list of render commands in draw order, and the computed layout
    /// for each element (indexed by element index).
    #[must_use]
    pub fn compute(
        &self,
        tree: &LayoutTree,
        viewport_width: f32,
        viewport_height: f32,
    ) -> LayoutResult {
        let Some(root) = tree.root else {
            return LayoutResult::default();
        };

        let element_count = tree.len();
        let mut computed = vec![ComputedLayout::default(); element_count];

        // Initialize leaf sizes (text measurement, fixed values).
        self.initialize_leaf_sizes(tree, &mut computed);

        // Propagate Fit container sizes bottom-up from their children.
        propagate_fit_sizes(tree, &mut computed, root, true);
        propagate_fit_sizes(tree, &mut computed, root, false);

        // Phase 1: Size along X axis (BFS top-down).
        size_along_axis(tree, &mut computed, root, true, viewport_width);

        // Phase 2: Size along Y axis (BFS top-down).
        size_along_axis(tree, &mut computed, root, false, viewport_height);

        // Phase 3: Position elements and generate render commands (DFS).
        let commands = position_and_render(tree, &mut computed, root);

        LayoutResult { computed, commands }
    }

    /// Initialize leaf element dimensions from text measurement and fixed sizing rules.
    fn initialize_leaf_sizes(&self, tree: &LayoutTree, computed: &mut [ComputedLayout]) {
        for (index, element) in tree.elements.iter().enumerate() {
            // Set initial size from Fixed rules.
            computed[index].width = match element.width {
                Sizing::Fixed(size) => size,
                _ => 0.0,
            };
            computed[index].height = match element.height {
                Sizing::Fixed(size) => size,
                _ => 0.0,
            };

            // Measure text content.
            if let ElementContent::Text {
                ref text,
                ref config,
            } = element.content
            {
                let dims = (self.measure_text)(text, config);
                if element.width.is_fit() {
                    computed[index].width = dims
                        .width
                        .clamp(element.width.min_size(), element.width.max_size());
                }
                if element.height.is_fit() {
                    computed[index].height = dims
                        .height
                        .clamp(element.height.min_size(), element.height.max_size());
                }
            }
        }
    }
}

/// Result of a layout computation.
#[derive(Clone, Debug, Default)]
pub struct LayoutResult {
    /// Computed layout for each element, indexed by element index.
    pub computed: Vec<ComputedLayout>,
    /// Render commands in draw order.
    pub commands: Vec<RenderCommand>,
}

// ── Layout passes (free functions) ────────────────────────────────────────────

/// Bottom-up pass: set Fit container sizes from their children's accumulated sizes.
///
/// This runs before the BFS so that when a parent processes its children,
/// Fit containers already have a content-based initial size.
fn propagate_fit_sizes(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    index: usize,
    x_axis: bool,
) -> f32 {
    let element = &tree.elements[index];
    let children = tree.children_of(index);

    // Leaf node: return current size.
    if children.is_empty() {
        return get_size(computed[index], x_axis);
    }

    // Recurse into children first (post-order).
    let child_sizes: Vec<f32> = children
        .iter()
        .map(|&child_idx| propagate_fit_sizes(tree, computed, child_idx, x_axis))
        .collect();

    let sizing = get_sizing(element, x_axis);
    if !sizing.is_fit() {
        return get_size(computed[index], x_axis);
    }

    let padding = if x_axis {
        element.padding.horizontal()
    } else {
        element.padding.vertical()
    };

    let is_along = is_layout_axis(element.direction, x_axis);
    let content_size = if is_along {
        let gap_total = if children.len() > 1 {
            #[allow(clippy::cast_precision_loss)]
            let gap = element.child_gap * (children.len() - 1) as f32;
            gap
        } else {
            0.0
        };
        let sum: f32 = child_sizes.iter().sum();
        sum + padding + gap_total
    } else {
        let max: f32 = child_sizes.iter().copied().fold(0.0_f32, f32::max);
        max + padding
    };

    let clamped = content_size.clamp(sizing.min_size(), sizing.max_size());
    set_size(&mut computed[index], x_axis, clamped);
    clamped
}

/// BFS sizing pass along one axis.
///
/// When `x_axis` is true, sizes widths; otherwise sizes heights.
fn size_along_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    root: usize,
    x_axis: bool,
    viewport_size: f32,
) {
    // Set root size if it hasn't been set.
    let root_element = &tree.elements[root];
    let root_size = get_size(computed[root], x_axis);
    if root_size <= 0.0 {
        let new_size = match get_sizing(root_element, x_axis) {
            Sizing::Grow { min, max } | Sizing::Fit { min, max } => viewport_size.clamp(min, max),
            Sizing::Fixed(size) => size,
            Sizing::Percent(frac) => viewport_size * frac,
        };
        set_size(&mut computed[root], x_axis, new_size);
    }

    // BFS queue.
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root);

    while let Some(parent_idx) = queue.pop_front() {
        let children = tree.children_of(parent_idx);
        if children.is_empty() {
            continue;
        }

        let parent_element = &tree.elements[parent_idx];
        let parent_size = get_size(computed[parent_idx], x_axis);
        let is_along = is_layout_axis(parent_element.direction, x_axis);

        let padding = if x_axis {
            parent_element.padding.horizontal()
        } else {
            parent_element.padding.vertical()
        };

        let gap_total = if is_along && children.len() > 1 {
            #[allow(clippy::cast_precision_loss)]
            let gap = parent_element.child_gap * (children.len() - 1) as f32;
            gap
        } else {
            0.0
        };

        // Resolve Percent children first.
        let available_for_percent = parent_size - padding - gap_total;
        for &child_idx in children {
            let child_sizing = get_sizing(&tree.elements[child_idx], x_axis);
            if let Sizing::Percent(frac) = child_sizing {
                let size = (available_for_percent * frac).max(0.0);
                set_size(&mut computed[child_idx], x_axis, size);
            }
        }

        if is_along {
            size_children_along_axis(
                tree,
                computed,
                parent_idx,
                children,
                x_axis,
                parent_size,
                padding,
                gap_total,
            );
        } else {
            size_children_cross_axis(tree, computed, children, x_axis, parent_size, padding);
        }

        // Enqueue children that have their own children.
        for &child_idx in children {
            if !tree.children_of(child_idx).is_empty() {
                queue.push_back(child_idx);
            }
        }
    }
}

/// Size children that are laid out ALONG the parent's layout axis.
#[allow(clippy::too_many_arguments)]
fn size_children_along_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    parent_idx: usize,
    children: &[usize],
    x_axis: bool,
    parent_size: f32,
    padding: f32,
    gap_total: f32,
) {
    let parent_element = &tree.elements[parent_idx];

    // Sum current child sizes, count grow children.
    let mut content_size: f32 = 0.0;
    let mut grow_count = 0_u32;
    for &child_idx in children {
        let child_sizing = get_sizing(&tree.elements[child_idx], x_axis);
        let child_size = get_size(computed[child_idx], x_axis);
        content_size += child_size;
        if child_sizing.is_grow() {
            grow_count += 1;
        }
    }

    let available = parent_size - padding - gap_total;
    let mut to_distribute = available - content_size;

    // Overflow compression: largest-first heuristic.
    if to_distribute < 0.0 && !parent_element.clip {
        compress_children(tree, computed, children, x_axis, &mut to_distribute);
    }

    // Growth expansion: smallest-first heuristic.
    if to_distribute > 0.0 && grow_count > 0 {
        expand_children(tree, computed, children, x_axis, &mut to_distribute);
    }
}

/// Size children that are laid out ACROSS (perpendicular to) the parent's layout axis.
fn size_children_cross_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    children: &[usize],
    x_axis: bool,
    parent_size: f32,
    padding: f32,
) {
    let max_size = parent_size - padding;

    for &child_idx in children {
        let child_element = &tree.elements[child_idx];
        let child_sizing = get_sizing(child_element, x_axis);
        let current = get_size(computed[child_idx], x_axis);

        let new_size = match child_sizing {
            Sizing::Grow { min, max } => max_size.clamp(min, max),
            Sizing::Fit { min, max } => {
                // Fit elements keep their propagated content size.
                if current > f32::EPSILON {
                    current.clamp(min, max)
                } else {
                    min
                }
            }
            Sizing::Fixed(size) => size,
            Sizing::Percent(frac) => (parent_size * frac).max(0.0),
        };

        set_size(&mut computed[child_idx], x_axis, new_size);
    }
}

/// DFS positioning pass. Computes final bounding boxes and emits render commands.
fn position_and_render(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    root: usize,
) -> Vec<RenderCommand> {
    let mut commands = Vec::new();

    // Stack entries: (element_index, x, y, is_second_visit)
    let mut stack: Vec<(usize, f32, f32, bool)> = vec![(root, 0.0, 0.0, false)];

    while let Some(&mut (index, x, y, ref mut visited)) = stack.last_mut() {
        let element = &tree.elements[index];
        let width = computed[index].width;
        let height = computed[index].height;

        if *visited {
            // Second visit (up-traversal): emit borders and scissor end.
            if let Some(ref border) = element.border {
                commands.push(RenderCommand {
                    bounds: BoundingBox {
                        x,
                        y,
                        width,
                        height,
                    },
                    kind: RenderCommandKind::Border { border: *border },
                    element_idx: index,
                });

                // Between-children borders.
                if border.between_children > 0.0 {
                    emit_between_borders(tree, computed, &mut commands, index, x, y, border);
                }
            }

            if element.clip {
                commands.push(RenderCommand {
                    bounds: BoundingBox {
                        x,
                        y,
                        width,
                        height,
                    },
                    kind: RenderCommandKind::ScissorEnd,
                    element_idx: index,
                });
            }

            stack.pop();
        } else {
            *visited = true;

            // Store the final bounding box.
            computed[index].bounds = BoundingBox {
                x,
                y,
                width,
                height,
            };

            // Emit rectangle if background is set.
            if let Some(color) = element.background {
                commands.push(RenderCommand {
                    bounds: BoundingBox {
                        x,
                        y,
                        width,
                        height,
                    },
                    kind: RenderCommandKind::Rectangle { color },
                    element_idx: index,
                });
            }

            // Emit scissor start if clipping.
            if element.clip {
                commands.push(RenderCommand {
                    bounds: BoundingBox {
                        x,
                        y,
                        width,
                        height,
                    },
                    kind: RenderCommandKind::ScissorStart,
                    element_idx: index,
                });
            }

            // Emit text if this is a text element.
            if let ElementContent::Text {
                ref text,
                ref config,
            } = element.content
            {
                commands.push(RenderCommand {
                    bounds: BoundingBox {
                        x,
                        y,
                        width,
                        height,
                    },
                    kind: RenderCommandKind::Text {
                        text: text.clone(),
                        config: config.clone(),
                    },
                    element_idx: index,
                });
            }

            // Push children in reverse order (so first child is processed first).
            let children = tree.children_of(index);
            if !children.is_empty() {
                let child_positions = compute_child_positions(tree, computed, index, x, y);
                for &(child_idx, cx, cy) in child_positions.iter().rev() {
                    stack.push((child_idx, cx, cy, false));
                }
            }
        }
    }

    commands
}

/// Compute the (x, y) position of each child within its parent.
fn compute_child_positions(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    parent_idx: usize,
    parent_x: f32,
    parent_y: f32,
) -> Vec<(usize, f32, f32)> {
    let parent = &tree.elements[parent_idx];
    let parent_w = computed[parent_idx].width;
    let parent_h = computed[parent_idx].height;
    let children = tree.children_of(parent_idx);

    if children.is_empty() {
        return Vec::new();
    }

    let is_horizontal = parent.direction == Direction::LeftToRight;

    // Calculate total children size along the layout axis.
    let children_main_size: f32 = children
        .iter()
        .map(|&idx| {
            if is_horizontal {
                computed[idx].width
            } else {
                computed[idx].height
            }
        })
        .sum();

    let gap_total = if children.len() > 1 {
        #[allow(clippy::cast_precision_loss)]
        let gap = parent.child_gap * (children.len() - 1) as f32;
        gap
    } else {
        0.0
    };

    // Extra space along the main axis for alignment.
    let main_available = if is_horizontal {
        parent_w - parent.padding.horizontal()
    } else {
        parent_h - parent.padding.vertical()
    };

    let extra_main = (main_available - children_main_size - gap_total).max(0.0);

    let main_offset = if is_horizontal {
        match parent.align_x {
            AlignX::Left => 0.0,
            AlignX::Center => extra_main * 0.5,
            AlignX::Right => extra_main,
        }
    } else {
        match parent.align_y {
            AlignY::Top => 0.0,
            AlignY::Center => extra_main * 0.5,
            AlignY::Bottom => extra_main,
        }
    };

    let mut positions = Vec::with_capacity(children.len());
    let mut cursor = main_offset;

    for &child_idx in children {
        let child_w = computed[child_idx].width;
        let child_h = computed[child_idx].height;

        let (cx, cy) = if is_horizontal {
            let cross_available = parent_h - parent.padding.vertical();
            let cross_offset = match parent.align_y {
                AlignY::Top => 0.0,
                AlignY::Center => (cross_available - child_h).max(0.0) * 0.5,
                AlignY::Bottom => (cross_available - child_h).max(0.0),
            };
            let x = parent_x + parent.padding.left + cursor;
            let y = parent_y + parent.padding.top + cross_offset;
            cursor += child_w + parent.child_gap;
            (x, y)
        } else {
            let cross_available = parent_w - parent.padding.horizontal();
            let cross_offset = match parent.align_x {
                AlignX::Left => 0.0,
                AlignX::Center => (cross_available - child_w).max(0.0) * 0.5,
                AlignX::Right => (cross_available - child_w).max(0.0),
            };
            let x = parent_x + parent.padding.left + cross_offset;
            let y = parent_y + parent.padding.top + cursor;
            cursor += child_h + parent.child_gap;
            (x, y)
        };

        positions.push((child_idx, cx, cy));
    }

    positions
}

/// Emit border-between-children rectangles.
fn emit_between_borders(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    commands: &mut Vec<RenderCommand>,
    parent_idx: usize,
    parent_x: f32,
    parent_y: f32,
    border: &Border,
) {
    let parent = &tree.elements[parent_idx];
    let parent_h = computed[parent_idx].height;
    let parent_w = computed[parent_idx].width;
    let children = tree.children_of(parent_idx);

    if children.len() < 2 {
        return;
    }

    let is_horizontal = parent.direction == Direction::LeftToRight;

    let child_positions = compute_child_positions(tree, computed, parent_idx, parent_x, parent_y);

    // Draw a line between each pair of adjacent children.
    for pair in child_positions.windows(2) {
        let (idx_a, ax, ay) = pair[0];
        let (_, bx, by) = pair[1];

        if is_horizontal {
            let midpoint =
                (bx - (ax + computed[idx_a].width)).mul_add(0.5, ax + computed[idx_a].width);
            let line_x = border.between_children.mul_add(-0.5, midpoint);
            commands.push(RenderCommand {
                bounds: BoundingBox {
                    x: line_x,
                    y: parent_y + parent.padding.top,
                    width: border.between_children,
                    height: parent_h - parent.padding.vertical(),
                },
                kind: RenderCommandKind::Rectangle {
                    color: border.color,
                },
                element_idx: parent_idx,
            });
        } else {
            let midpoint =
                (by - (ay + computed[idx_a].height)).mul_add(0.5, ay + computed[idx_a].height);
            let line_y = border.between_children.mul_add(-0.5, midpoint);
            commands.push(RenderCommand {
                bounds: BoundingBox {
                    x: parent_x + parent.padding.left,
                    y: line_y,
                    width: parent_w - parent.padding.horizontal(),
                    height: border.between_children,
                },
                kind: RenderCommandKind::Rectangle {
                    color: border.color,
                },
                element_idx: parent_idx,
            });
        }
    }
}

// ── Sizing heuristics (free functions) ───────────────────────────────────────

/// Compresses children using the largest-first heuristic.
fn compress_children(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    children: &[usize],
    x_axis: bool,
    to_distribute: &mut f32,
) {
    let mut resizable: Vec<usize> = children
        .iter()
        .copied()
        .filter(|&idx| get_sizing(&tree.elements[idx], x_axis).is_resizable())
        .collect();

    while *to_distribute < -f32::EPSILON && !resizable.is_empty() {
        let mut largest = f32::NEG_INFINITY;
        let mut second_largest = f32::NEG_INFINITY;

        for &idx in &resizable {
            let size = get_size(computed[idx], x_axis);
            if size > largest {
                second_largest = largest;
                largest = size;
            } else if size > second_largest && (size - largest).abs() > f32::EPSILON {
                second_largest = size;
            }
        }

        if largest <= f32::EPSILON {
            break;
        }

        let at_largest: Vec<usize> = resizable
            .iter()
            .copied()
            .filter(|&idx| (get_size(computed[idx], x_axis) - largest).abs() < f32::EPSILON)
            .collect();

        #[allow(clippy::cast_precision_loss)]
        let count = at_largest.len() as f32;
        let delta_even = (-*to_distribute) / count;

        // If all at same size (no second largest), just distribute evenly.
        let shrink_per_child = if second_largest > f32::NEG_INFINITY {
            let delta_to_second = largest - second_largest;
            delta_to_second.min(delta_even)
        } else {
            delta_even
        };

        if shrink_per_child <= f32::EPSILON {
            break;
        }

        for &idx in &at_largest {
            let current = get_size(computed[idx], x_axis);
            let min = get_sizing(&tree.elements[idx], x_axis).min_size();
            let new_size = (current - shrink_per_child).max(min);
            let actual_shrink = current - new_size;
            set_size(&mut computed[idx], x_axis, new_size);
            *to_distribute += actual_shrink;
        }

        resizable.retain(|&idx| {
            let size = get_size(computed[idx], x_axis);
            let min = get_sizing(&tree.elements[idx], x_axis).min_size();
            size > min + f32::EPSILON
        });
    }
}

/// Expands Grow children using the smallest-first heuristic.
fn expand_children(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    children: &[usize],
    x_axis: bool,
    to_distribute: &mut f32,
) {
    let mut growable: Vec<usize> = children
        .iter()
        .copied()
        .filter(|&idx| get_sizing(&tree.elements[idx], x_axis).is_grow())
        .collect();

    while *to_distribute > f32::EPSILON && !growable.is_empty() {
        let mut smallest = f32::INFINITY;
        let mut second_smallest = f32::INFINITY;

        for &idx in &growable {
            let size = get_size(computed[idx], x_axis);
            if size < smallest {
                second_smallest = smallest;
                smallest = size;
            } else if size < second_smallest && (size - smallest).abs() > f32::EPSILON {
                second_smallest = size;
            }
        }

        let at_smallest: Vec<usize> = growable
            .iter()
            .copied()
            .filter(|&idx| (get_size(computed[idx], x_axis) - smallest).abs() < f32::EPSILON)
            .collect();

        #[allow(clippy::cast_precision_loss)]
        let count = at_smallest.len() as f32;
        let delta_even = *to_distribute / count;

        // If all at same size (no second smallest), just distribute evenly.
        let grow_per_child = if second_smallest < f32::INFINITY {
            let delta_to_second = second_smallest - smallest;
            delta_to_second.min(delta_even)
        } else {
            delta_even
        };

        if grow_per_child <= f32::EPSILON {
            // All remaining space is less than epsilon per child — done.
            break;
        }

        for &idx in &at_smallest {
            let current = get_size(computed[idx], x_axis);
            let max = get_sizing(&tree.elements[idx], x_axis).max_size();
            let new_size = (current + grow_per_child).min(max);
            let actual_grow = new_size - current;
            set_size(&mut computed[idx], x_axis, new_size);
            *to_distribute -= actual_grow;
        }

        growable.retain(|&idx| {
            let size = get_size(computed[idx], x_axis);
            let max = get_sizing(&tree.elements[idx], x_axis).max_size();
            size < max - f32::EPSILON
        });
    }
}

// ── Axis helpers ──────────────────────────────────────────────────────────────

/// Returns the sizing rule for the given element along the specified axis.
const fn get_sizing(element: &Element, x_axis: bool) -> Sizing {
    if x_axis {
        element.width
    } else {
        element.height
    }
}

/// Returns the computed size for the given element along the specified axis.
const fn get_size(computed: ComputedLayout, x_axis: bool) -> f32 {
    if x_axis {
        computed.width
    } else {
        computed.height
    }
}

/// Sets the computed size for the given element along the specified axis.
const fn set_size(computed: &mut ComputedLayout, x_axis: bool, value: f32) {
    if x_axis {
        computed.width = value;
    } else {
        computed.height = value;
    }
}

/// Returns `true` if `direction` lays out children along the given axis.
const fn is_layout_axis(direction: Direction, x_axis: bool) -> bool {
    match direction {
        Direction::LeftToRight => x_axis,
        Direction::TopToBottom => !x_axis,
    }
}

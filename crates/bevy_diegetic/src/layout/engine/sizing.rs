use bevy_kana::ToF32;

use super::layout_engine::ComputedLayout;
use crate::layout::ChildLayout;
use crate::layout::Sizing;
use crate::layout::constants::LAYOUT_EPSILON;
use crate::layout::element::ChildOverflow;
use crate::layout::element::Element;
use crate::layout::element::ElementContent;
use crate::layout::element::LayoutTree;

/// Selects which layout axis a sizing or positioning operation targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Axis {
    X,
    Y,
}

/// Returns the total border inset for `element` along `axis`.
pub(super) fn border_inset(element: &Element, axis: Axis) -> f32 {
    element.border.as_ref().map_or(0.0, |b| match axis {
        Axis::X => b.horizontal(),
        Axis::Y => b.vertical(),
    })
}

/// Returns the leading border width (left for X, top for Y).
pub(super) fn border_leading(element: &Element, axis: Axis) -> f32 {
    element.border.as_ref().map_or(0.0, |b| match axis {
        Axis::X => b.left.value,
        Axis::Y => b.top.value,
    })
}

/// Bottom-up pass: set `Fit` container sizes and propagate `minDimensions`.
///
/// This runs before the BFS so that when a parent processes its children,
/// `Fit` containers already have a content-based initial size and every element
/// has its `min_width`/`min_height` floor computed.
///
/// Returns the content size of the element so that parent `Fit` elements can
/// account for it — even if this element is `Grow` (whose actual size is
/// determined later by `size_along_axis`). Without this, a `Fit` parent with
/// `Grow` children would see 0 and compute a collapsed height.
pub(super) fn propagate_fit_sizes(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    index: usize,
    axis: Axis,
) -> f32 {
    let element = &tree.elements[index];
    let children = tree.children_of(index);
    let sizing = get_sizing(element, axis);

    // Leaf node: set `minDimensions` from the element's content.
    //
    // For text elements, Clay sets `minDimensions = { minWidth, height }`:
    // - height: measured height — text can't be compressed vertically.
    // - width: shortest word width — text can wrap horizontally.
    // We use the measured height for Y and the sizing floor for X (since we
    // don't yet track per-word minimum width).
    if children.is_empty() {
        let current_size = get_size(computed[index], axis);
        let leaf_min = if axis == Axis::Y && matches!(element.content, ElementContent::Text { .. })
        {
            // Text min height = measured height (matches Clay line 2003).
            current_size.clamp(sizing.min_size(), sizing.max_size())
        } else {
            0.0_f32.clamp(sizing.min_size(), sizing.max_size())
        };
        set_min_size(&mut computed[index], axis, leaf_min);
        return current_size;
    }

    let is_along = is_layout_axis(&element.child_layout, axis);

    // Recurse into children first (post-order), accumulating sizes inline
    // to avoid per-call Vec allocation.
    let mut content_acc: f32 = 0.0;
    let mut min_acc: f32 = 0.0;
    for &child_idx in children {
        let child_size = propagate_fit_sizes(tree, computed, child_idx, axis);
        let child_min = get_min_size(computed[child_idx], axis);
        if is_along {
            content_acc += child_size;
            min_acc += child_min;
        } else {
            content_acc = content_acc.max(child_size);
            min_acc = min_acc.max(child_min);
        }
    }

    // Fixed elements already have their size — but still compute minDimensions.
    if let Sizing::Fixed(size) = sizing {
        let min = 0.0_f32.clamp(sizing.min_size(), sizing.max_size());
        set_min_size(&mut computed[index], axis, min);
        return size.value;
    }

    let padding = match axis {
        Axis::X => element.padding.horizontal(),
        Axis::Y => element.padding.vertical(),
    };
    let border = border_inset(element, axis);

    let gap_total = if is_along && children.len() > 1 {
        element.child_layout.gap().value * (children.len() - 1).to_f32()
    } else {
        0.0
    };

    let chrome = padding + border;

    // A clipping container does not inflate to contain its children — they
    // overflow (and may scroll), so it reports only its own chrome to ancestors.
    // Without this, a scrollable container's min size equals its full content,
    // forcing every ancestor to grow to the content instead of clipping it.
    // Matches Clay's scroll-container rule.
    let clipped = matches!(element.overflow, ChildOverflow::Clipped);
    let content_acc = if clipped { 0.0 } else { content_acc };
    let min_acc = if clipped { 0.0 } else { min_acc };
    let gap_for_size = if clipped || !is_along { 0.0 } else { gap_total };

    let content_size = content_acc + chrome + gap_for_size;
    let min_from_children = min_acc + chrome + gap_for_size;

    // Clamp minDimensions to [sizing.min, sizing.max] — matches Clay.
    let clamped_min = min_from_children.clamp(sizing.min_size(), sizing.max_size());
    set_min_size(&mut computed[index], axis, clamped_min);

    // Fit elements: set their computed size now.
    if sizing.is_fit() {
        let clamped = content_size.clamp(sizing.min_size(), sizing.max_size());
        set_size(&mut computed[index], axis, clamped);
        return clamped;
    }

    // Grow elements: set initial computed size from content, clamped to [min, max].
    // This matches Clay's `CloseElement` which initializes dimensions for all element
    // types from their children before `SizeContainersAlongAxis`. Without this, GROW
    // elements start at 0, masking overflow and preventing compression from triggering.
    if sizing.is_grow() {
        let clamped = content_size.clamp(sizing.min_size(), sizing.max_size());
        set_size(&mut computed[index], axis, clamped);
        return clamped;
    }

    // Percent elements: return content size so ancestor Fit elements
    // can account for it, but don't set the computed size yet.
    content_size
}

/// BFS sizing pass along one axis.
///
/// When `axis` is `Axis::X`, sizes widths; when `Axis::Y`, sizes heights.
pub(super) fn size_along_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    root: usize,
    axis: Axis,
    viewport_size: f32,
) {
    // Set root size from the viewport.
    // - Fixed roots keep their declared size (already set by initialize_leaf_sizes).
    // - Grow/Fit roots fill the viewport — this is re-applied unconditionally because
    //   propagate_fit_sizes may have set them to their content size, but the root should match the
    //   viewport, not its content.
    let root_element = &tree.elements[root];
    let new_root_size = match get_sizing(root_element, axis) {
        Sizing::Grow { min, max } => viewport_size.clamp(min.value, max.value),
        Sizing::Fit { min, max } => {
            let content = get_size(computed[root], axis);
            if content > 0.0 {
                content.clamp(min.value, max.value)
            } else {
                viewport_size.clamp(min.value, max.value)
            }
        },
        Sizing::Fixed(size) => size.value,
        Sizing::Percent(frac) => viewport_size * frac,
    };
    set_size(&mut computed[root], axis, new_root_size);

    // Top-down traversal using a stack (parents always processed before children).
    let mut queue = Vec::with_capacity(tree.len());
    queue.push(root);

    while let Some(parent_idx) = queue.pop() {
        let children = tree.children_of(parent_idx);
        if children.is_empty() {
            continue;
        }

        let parent_element = &tree.elements[parent_idx];
        let parent_size = get_size(computed[parent_idx], axis);
        let is_along = is_layout_axis(&parent_element.child_layout, axis);

        let padding = match axis {
            Axis::X => parent_element.padding.horizontal(),
            Axis::Y => parent_element.padding.vertical(),
        };
        let border = border_inset(parent_element, axis);
        let chrome = padding + border;

        let gap_total = if is_along && children.len() > 1 {
            parent_element.child_layout.gap().value * (children.len() - 1).to_f32()
        } else {
            0.0
        };

        // Resolve Percent children first.
        let available_for_percent = parent_size - chrome - gap_total;
        for &child_idx in children {
            let child_sizing = get_sizing(&tree.elements[child_idx], axis);
            if let Sizing::Percent(frac) = child_sizing {
                let size = (available_for_percent * frac).max(0.0);
                set_size(&mut computed[child_idx], axis, size);
            }
        }

        if is_along {
            size_children_along_axis(
                tree,
                computed,
                parent_idx,
                children,
                axis,
                AxisMetrics {
                    parent_size,
                    padding,
                    border,
                    gap_total,
                },
            );
        } else {
            size_children_cross_axis(tree, computed, children, axis, parent_size, padding, border);
        }

        // Enqueue children (reverse order so first child is popped first from stack).
        for &child_idx in children.iter().rev() {
            if !tree.children_of(child_idx).is_empty() {
                queue.push(child_idx);
            }
        }
    }
}

/// Parent-axis size budget: the parent's full size along the axis and the
/// three deductions (padding, border, gap totals) that reduce what children
/// can consume.
struct AxisMetrics {
    parent_size: f32,
    padding:     f32,
    border:      f32,
    gap_total:   f32,
}

/// Size children that are laid out ALONG the parent's layout axis.
fn size_children_along_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    parent_idx: usize,
    children: &[usize],
    axis: Axis,
    metrics: AxisMetrics,
) {
    let parent_element = &tree.elements[parent_idx];

    // Sum current child sizes, count grow children.
    let mut content_size: f32 = 0.0;
    let mut grow_count = 0_u32;
    for &child_idx in children {
        let child_sizing = get_sizing(&tree.elements[child_idx], axis);
        let child_size = get_size(computed[child_idx], axis);
        content_size += child_size;
        if child_sizing.is_grow() {
            grow_count += 1;
        }
    }

    let available = metrics.parent_size - metrics.padding - metrics.border - metrics.gap_total;
    let mut to_distribute = available - content_size;

    // Overflow compression: largest-first heuristic.
    if to_distribute < 0.0 && matches!(parent_element.overflow, ChildOverflow::Visible) {
        compress_children(tree, computed, children, axis, &mut to_distribute);
    }

    // Growth expansion: smallest-first heuristic.
    if to_distribute > 0.0 && grow_count > 0 {
        expand_children(tree, computed, children, axis, &mut to_distribute);
    }
}

/// Size children that are laid out ACROSS (perpendicular to) the parent's layout axis.
///
/// Applies `MAX(minDimensions, MIN(childSize, maxSize))` -- Clay's cross-axis rule
/// that prevents children from shrinking below their propagated content minimum.
fn size_children_cross_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    children: &[usize],
    axis: Axis,
    parent_size: f32,
    padding: f32,
    border: f32,
) {
    let max_size = parent_size - padding - border;

    for &child_idx in children {
        let child_element = &tree.elements[child_idx];
        let child_sizing = get_sizing(child_element, axis);
        let current = get_size(computed[child_idx], axis);
        let min_dim = get_min_size(computed[child_idx], axis);

        let new_size = match child_sizing {
            Sizing::Grow { min, max } => max_size.clamp(min.value, max.value),
            Sizing::Fit { min, max } => {
                // Fit elements keep their propagated content size.
                if current > f32::EPSILON {
                    current.clamp(min.value, max.value)
                } else {
                    min.value
                }
            },
            Sizing::Fixed(size) => size.value,
            Sizing::Percent(frac) => (parent_size * frac).max(0.0),
        };

        // Apply minDimensions floor: MAX(minDimensions, MIN(childSize, maxSize)).
        let floored = new_size.max(min_dim);
        set_size(&mut computed[child_idx], axis, floored);
    }
}

/// Compresses children using the largest-first heuristic.
///
/// Iterates `children` directly each pass to avoid per-call Vec allocations.
fn compress_children(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    children: &[usize],
    axis: Axis,
    to_distribute: &mut f32,
) {
    loop {
        if *to_distribute >= -LAYOUT_EPSILON {
            break;
        }

        // Single pass: find largest, second-largest, and count at largest
        // among resizable children still above their minimum.
        let mut largest = f32::NEG_INFINITY;
        let mut second_largest = f32::NEG_INFINITY;
        let mut at_largest_count = 0_u32;

        for &idx in children {
            if !get_sizing(&tree.elements[idx], axis).is_resizable() {
                continue;
            }
            let size = get_size(computed[idx], axis);
            let min = get_min_size(computed[idx], axis);
            if size <= min + LAYOUT_EPSILON {
                continue;
            }
            if size > largest + LAYOUT_EPSILON {
                second_largest = largest;
                largest = size;
                at_largest_count = 1;
            } else if (size - largest).abs() <= LAYOUT_EPSILON {
                at_largest_count += 1;
            } else if size > second_largest {
                second_largest = size;
            }
        }

        if at_largest_count == 0 || largest <= LAYOUT_EPSILON {
            break;
        }

        let count = at_largest_count.to_f32();
        let delta_even = (-*to_distribute) / count;

        // If all at same size (no second largest), just distribute evenly.
        let shrink_per_child = if second_largest > f32::NEG_INFINITY {
            let delta_to_second = largest - second_largest;
            delta_to_second.min(delta_even)
        } else {
            delta_even
        };

        if shrink_per_child <= LAYOUT_EPSILON {
            break;
        }

        // Apply shrink to resizable children at the largest size.
        let mut total_shrink = 0.0_f32;
        for &idx in children {
            if !get_sizing(&tree.elements[idx], axis).is_resizable() {
                continue;
            }
            let current = get_size(computed[idx], axis);
            if (current - largest).abs() > LAYOUT_EPSILON {
                continue;
            }
            let min = get_min_size(computed[idx], axis);
            let new_size = (current - shrink_per_child).max(min);
            let actual_shrink = current - new_size;
            set_size(&mut computed[idx], axis, new_size);
            *to_distribute += actual_shrink;
            total_shrink += actual_shrink;
        }

        if total_shrink <= LAYOUT_EPSILON {
            break;
        }
    }
}

/// Expands `Grow` children using the smallest-first heuristic.
///
/// Iterates `children` directly each pass to avoid per-call Vec allocations.
fn expand_children(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    children: &[usize],
    axis: Axis,
    to_distribute: &mut f32,
) {
    loop {
        if *to_distribute <= LAYOUT_EPSILON {
            break;
        }

        // Single pass: find smallest, second-smallest, and count at smallest
        // among growable children still below their maximum.
        let mut smallest = f32::INFINITY;
        let mut second_smallest = f32::INFINITY;
        let mut at_smallest_count = 0_u32;

        for &idx in children {
            if !get_sizing(&tree.elements[idx], axis).is_grow() {
                continue;
            }
            let size = get_size(computed[idx], axis);
            let max = get_sizing(&tree.elements[idx], axis).max_size();
            if size >= max - LAYOUT_EPSILON {
                continue;
            }
            if size < smallest - LAYOUT_EPSILON {
                second_smallest = smallest;
                smallest = size;
                at_smallest_count = 1;
            } else if (size - smallest).abs() <= LAYOUT_EPSILON {
                at_smallest_count += 1;
            } else if size < second_smallest {
                second_smallest = size;
            }
        }

        if at_smallest_count == 0 {
            break;
        }

        let count = at_smallest_count.to_f32();
        let delta_even = *to_distribute / count;

        // If all at same size (no second smallest), just distribute evenly.
        let grow_per_child = if second_smallest < f32::INFINITY {
            let delta_to_second = second_smallest - smallest;
            delta_to_second.min(delta_even)
        } else {
            delta_even
        };

        if grow_per_child <= LAYOUT_EPSILON {
            break;
        }

        // Apply growth to growable children at the smallest size.
        let mut total_grow = 0.0_f32;
        for &idx in children {
            if !get_sizing(&tree.elements[idx], axis).is_grow() {
                continue;
            }
            let current = get_size(computed[idx], axis);
            if (current - smallest).abs() > LAYOUT_EPSILON {
                continue;
            }
            let max = get_sizing(&tree.elements[idx], axis).max_size();
            let new_size = (current + grow_per_child).min(max);
            let actual_grow = new_size - current;
            set_size(&mut computed[idx], axis, new_size);
            *to_distribute -= actual_grow;
            total_grow += actual_grow;
        }

        if total_grow <= LAYOUT_EPSILON {
            break;
        }
    }
}

/// Returns the sizing rule for the given element along the specified axis.
const fn get_sizing(element: &Element, axis: Axis) -> Sizing {
    match axis {
        Axis::X => element.width,
        Axis::Y => element.height,
    }
}

/// Returns the computed size for the given element along the specified axis.
const fn get_size(computed: ComputedLayout, axis: Axis) -> f32 {
    match axis {
        Axis::X => computed.width,
        Axis::Y => computed.height,
    }
}

/// Sets the computed size for the given element along the specified axis.
const fn set_size(computed: &mut ComputedLayout, axis: Axis, value: f32) {
    match axis {
        Axis::X => computed.width = value,
        Axis::Y => computed.height = value,
    }
}

/// Returns the propagated minimum content size along the specified axis.
const fn get_min_size(computed: ComputedLayout, axis: Axis) -> f32 {
    match axis {
        Axis::X => computed.min_width,
        Axis::Y => computed.min_height,
    }
}

/// Sets the propagated minimum content size along the specified axis.
const fn set_min_size(computed: &mut ComputedLayout, axis: Axis, value: f32) {
    match axis {
        Axis::X => computed.min_width = value,
        Axis::Y => computed.min_height = value,
    }
}

/// Returns `true` if `child_layout` lays out children along the given axis.
const fn is_layout_axis(child_layout: &ChildLayout, axis: Axis) -> bool {
    match axis {
        Axis::X => child_layout.is_row(),
        Axis::Y => child_layout.is_column(),
    }
}

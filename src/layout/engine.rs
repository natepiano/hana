//! Core layout computation engine.
//!
//! Implements a Clay-inspired two-pass layout algorithm:
//! 1. **Sizing pass** — BFS traversal determines element dimensions (called twice: X then Y).
//! 2. **Positioning pass** — DFS traversal computes final positions and emits render commands.
//!
//! The engine is fully self-contained with no global state. Multiple instances can run
//! concurrently on different threads without interference.

use std::sync::Arc;

use bevy_kana::ToF32;

use super::element::Element;
use super::element::ElementContent;
use super::element::LayoutTree;
use super::render::RectangleSource;
use super::render::RenderCommand;
use super::render::RenderCommandKind;
use super::types::AlignX;
use super::types::AlignY;
use super::types::Border;
use super::types::BoundingBox;
use super::types::Direction;
use super::types::LayoutTextStyle;
use super::types::Sizing;
use super::types::TextDimensions;
use super::types::TextMeasure;
use super::types::TextWrap;

/// Selects which layout axis a sizing or positioning operation targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// Callback type for measuring text dimensions.
///
/// Given a text string and its measurement properties, returns the measured
/// dimensions in layout units. The layout engine calls this during sizing to
/// determine how much space text elements need.
///
/// Takes [`TextMeasure`] (a generic-free extraction from [`TextConfig`]) to
/// avoid leaking the typestate generic into the measurement function.
///
/// Uses `Arc` so the function can be shared across threads and cloned cheaply
/// (e.g. stored in a Bevy `Resource` and cloned to create `LayoutEngine` instances).
pub type MeasureTextFn = Arc<dyn Fn(&str, &TextMeasure) -> TextDimensions + Send + Sync>;

/// Clay-style tolerance for layout convergence and float grouping.
///
/// Layout operates in pixel-like units, so differences below one hundredth
/// of a unit are not visually meaningful but can cause iterative sizing loops
/// to spin forever if treated as significant.
const LAYOUT_EPSILON: f32 = 0.01;

/// Computed layout data for a single element.
#[derive(Clone, Copy, Debug, Default)]
pub struct ComputedLayout {
    /// Final bounding box in layout coordinates.
    pub bounds:         BoundingBox,
    /// Resolved width before positioning.
    pub width:          f32,
    /// Resolved height before positioning.
    pub height:         f32,
    /// Propagated minimum width from children's content.
    ///
    /// Computed bottom-up alongside `propagate_fit_sizes`. Used as a hard
    /// floor during overflow compression and cross-axis sizing — an element
    /// must never shrink below its children's irreducible content size.
    min_width:          f32,
    /// Propagated minimum height from children's content.
    min_height:         f32,
    /// Cached natural (unwrapped) text width from `initialize_leaf_sizes`.
    ///
    /// Stored once during initial measurement so that `rewrap_text_elements`
    /// can check whether wrapping is needed without re-calling the measure
    /// function. Zero for non-text elements.
    natural_text_width: f32,
}

/// The layout engine. Thread-safe, no global state.
///
/// # Usage
///
/// ```ignore
/// let engine = LayoutEngine::new(measure_fn);
/// let result = engine.compute(&tree, 800.0, 600.0, 1.0);
/// ```
///
/// Viewport culling is always enabled — elements whose bounding box lies
/// entirely outside the viewport are omitted from the render command list.
pub struct LayoutEngine {
    measure_text: MeasureTextFn,
}

impl LayoutEngine {
    /// Creates a new layout engine with the given text measurement callback.
    #[must_use]
    pub fn new(measure_text: MeasureTextFn) -> Self { Self { measure_text } }

    /// Computes layout for the given tree within the specified viewport dimensions.
    ///
    /// Returns a list of render commands in draw order, and the computed layout
    /// for each element (indexed by element index).
    #[must_use]
    /// Computes layout for the given tree within the specified viewport dimensions.
    ///
    /// `font_scale` converts font sizes from font units to layout units.
    /// When font and layout units are the same, pass `1.0`.
    pub fn compute(
        &self,
        tree: &LayoutTree,
        viewport_width: f32,
        viewport_height: f32,
        font_scale: f32,
    ) -> LayoutResult {
        let Some(root) = tree.root else {
            return LayoutResult::default();
        };

        let element_count = tree.len();
        let mut computed = vec![ComputedLayout::default(); element_count];

        // Initialize leaf sizes (text measurement, fixed values).
        self.initialize_leaf_sizes(tree, &mut computed, font_scale);

        // Propagate Fit container sizes bottom-up from their children.
        propagate_fit_sizes(tree, &mut computed, root, Axis::X);
        propagate_fit_sizes(tree, &mut computed, root, Axis::Y);

        // Phase 1: Size along X axis (BFS top-down).
        size_along_axis(tree, &mut computed, root, Axis::X, viewport_width);

        // Phase 2: Re-wrap text elements within their resolved widths.
        // This may change text heights (more lines), so we re-propagate Y
        // and re-size along Y afterwards — but only if wrapping actually changed sizes.
        let (wrapped, text_sizes_changed) =
            rewrap_text_elements(tree, &mut computed, &self.measure_text, font_scale);
        if text_sizes_changed {
            propagate_fit_sizes(tree, &mut computed, root, Axis::Y);
        }

        // Phase 3: Size along Y axis (BFS top-down) with wrap-corrected heights.
        size_along_axis(tree, &mut computed, root, Axis::Y, viewport_height);

        // Phase 4: Position elements and generate render commands (DFS).
        let commands = position_and_render(
            tree,
            &mut computed,
            root,
            &wrapped,
            viewport_width,
            viewport_height,
            font_scale,
        );

        LayoutResult { computed, commands }
    }

    /// Initialize leaf element dimensions from text measurement and fixed sizing rules.
    fn initialize_leaf_sizes(
        &self,
        tree: &LayoutTree,
        computed: &mut [ComputedLayout],
        font_scale: f32,
    ) {
        for (index, element) in tree.elements.iter().enumerate() {
            // Set initial size from Fixed rules.
            computed[index].width = match element.width {
                Sizing::Fixed(size) => size.value,
                _ => 0.0,
            };
            computed[index].height = match element.height {
                Sizing::Fixed(size) => size.value,
                _ => 0.0,
            };

            // Measure text content and cache the natural width for the
            // rewrap fast-path (avoids re-measuring every text element later).
            if let ElementContent::Text {
                ref text,
                ref config,
            } = element.content
            {
                let dims = (self.measure_text)(text, &config.as_measure().scaled(font_scale));
                computed[index].natural_text_width = dims.width;
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

impl LayoutResult {
    /// Returns the bounding box of the first user-defined element.
    ///
    /// Element 0 is the implicit viewport-sized root created by
    /// [`LayoutBuilder`]. The actual content starts at element 1 — the first
    /// child passed to `builder.with()`. With `Sizing::FIT`, this element's
    /// bounds reflect the real content size rather than the full viewport.
    ///
    /// Returns `None` if no user-defined element exists.
    pub fn content_bounds(&self) -> Option<BoundingBox> { self.computed.get(1).map(|c| c.bounds) }
}

// ── Text wrapping ─────────────────────────────────────────────────────────────

/// A single line of wrapped text with its measured width.
struct WrappedLine {
    text:  String,
    width: f32,
}

/// Pre-computed word-wrap results for a text element.
struct WrappedText {
    lines:       Vec<WrappedLine>,
    line_height: f32,
}

/// Word-wraps text within `max_width`, splitting at whitespace boundaries.
///
/// Explicit `\n` characters are respected as paragraph breaks. Each paragraph
/// is then word-wrapped independently. A word that exceeds `max_width` on its
/// own is placed on a single line without breaking.
fn wrap_text_words(
    text: &str,
    config: &LayoutTextStyle,
    max_width: f32,
    measure: &MeasureTextFn,
    font_scale: f32,
) -> WrappedText {
    let text_measure = config.as_measure().scaled(font_scale);
    let space_dims = measure(" ", &text_measure);
    let line_height = space_dims.line_height;
    let space_width = space_dims.width;
    let mut all_lines = Vec::new();

    for paragraph in text.split('\n') {
        let words: Vec<&str> = paragraph.split_whitespace().collect();

        if words.is_empty() {
            all_lines.push(WrappedLine {
                text:  String::new(),
                width: 0.0,
            });
            continue;
        }

        let mut current_text = String::new();
        let mut current_width: f32 = 0.0;

        for word in words {
            let word_width = measure(word, &text_measure).width;

            if current_text.is_empty() {
                // First word on this line — always take it, even if it overflows.
                current_text.push_str(word);
                current_width = word_width;
            } else {
                let projected = current_width + space_width + word_width;
                if projected > max_width {
                    // Break: emit current line, start new line with this word.
                    // Re-measure the complete line text so the width accounts for
                    // kerning and glyph bearings that word-level accumulation misses.
                    let line_width = measure(&current_text, &text_measure).width;
                    all_lines.push(WrappedLine {
                        text:  current_text,
                        width: line_width,
                    });
                    current_text = word.to_string();
                    current_width = word_width;
                } else {
                    current_text.push(' ');
                    current_text.push_str(word);
                    current_width = projected;
                }
            }
        }

        // Emit the last line of this paragraph — re-measure the full line.
        let line_width = if current_text.is_empty() {
            0.0
        } else {
            measure(&current_text, &text_measure).width
        };
        all_lines.push(WrappedLine {
            text:  current_text,
            width: line_width,
        });
    }

    if all_lines.is_empty() {
        all_lines.push(WrappedLine {
            text:  String::new(),
            width: 0.0,
        });
    }

    WrappedText {
        lines: all_lines,
        line_height,
    }
}

/// Splits text at explicit `\n` characters and measures each line as a single run.
fn wrap_text_newlines(
    text: &str,
    config: &LayoutTextStyle,
    measure: &MeasureTextFn,
    font_scale: f32,
) -> WrappedText {
    let text_measure = config.as_measure().scaled(font_scale);
    let mut lines = Vec::new();
    let mut line_height = 0.0_f32;

    for line in text.split('\n') {
        let dims = measure(line, &text_measure);
        line_height = dims.line_height;
        lines.push(WrappedLine {
            text:  line.to_string(),
            width: dims.width,
        });
    }

    if lines.is_empty() {
        lines.push(WrappedLine {
            text:  String::new(),
            width: 0.0,
        });
    }

    WrappedText { lines, line_height }
}

/// Re-wraps text elements within their parent's content area and updates
/// computed widths and heights.
///
/// Returns per-element wrapped text data (indexed by element index) and a flag
/// indicating whether any computed sizes actually changed (used to skip
/// redundant re-propagation).
///
/// Two key optimizations avoid work in the common case (short text that fits):
///
/// 1. **Cached natural width** — uses the `natural_text_width` stored during
///    `initialize_leaf_sizes` instead of re-calling the measure function. If the cached width fits
///    within the element's post-sizing width, the text won't reflow, so we skip wrapping entirely.
///
/// 2. **Lazy `parent_of`** — the parent lookup table (one `Vec<Option<usize>>` the size of the
///    element array) is only built if a text element actually needs wrapping. For layouts where all
///    text fits without reflowing, this allocation and O(N) build cost are avoided completely.
fn rewrap_text_elements(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    measure: &MeasureTextFn,
    font_scale: f32,
) -> (Vec<Option<WrappedText>>, bool) {
    let mut wrapped: Vec<Option<WrappedText>> = (0..tree.len()).map(|_| None).collect();
    let mut any_changed = false;

    // Parent lookup for finding each text element's container width.
    // Cheap O(N) build — far less expensive than the ~1000 measure() calls
    // that the cached-width fast path eliminates.
    let parent_of = build_parent_of(tree);

    for (index, element) in tree.elements.iter().enumerate() {
        if let ElementContent::Text {
            ref text,
            ref config,
        } = element.content
        {
            let result = match config.wrap_mode() {
                TextWrap::Words => {
                    let max_width = parent_content_width(tree, computed, &parent_of, index);
                    // Fast path: compare the cached natural text width (measured
                    // once in `initialize_leaf_sizes`) against the parent's
                    // content area. If the text fits and has no explicit
                    // newlines, wrapping would produce one identical line — skip.
                    // Uses the cached width to avoid re-calling the measure fn.
                    let natural_width = computed[index].natural_text_width;
                    if !text.contains('\n') && natural_width <= max_width {
                        continue;
                    }
                    wrap_text_words(text, config, max_width, measure, font_scale)
                },
                TextWrap::Newlines => {
                    // Fast path: no explicit newlines means a single line.
                    if !text.contains('\n') {
                        continue;
                    }
                    wrap_text_newlines(text, config, measure, font_scale)
                },
                TextWrap::None => continue,
            };

            // Track whether wrapping actually changed element sizes.
            let old_width = computed[index].width;
            let old_height = computed[index].height;

            // Update width to the widest wrapped line, clamped by sizing rules.
            let max_line_width = result.lines.iter().map(|l| l.width).fold(0.0_f32, f32::max);
            if element.width.is_fit() {
                computed[index].width =
                    max_line_width.clamp(element.width.min_size(), element.width.max_size());
            }

            // Update height from the wrapped line count.
            let new_height = result.line_height * result.lines.len().to_f32();
            computed[index].height =
                new_height.clamp(element.height.min_size(), element.height.max_size());

            if (computed[index].width - old_width).abs() > f32::EPSILON
                || (computed[index].height - old_height).abs() > f32::EPSILON
            {
                any_changed = true;
            }

            wrapped[index] = Some(result);
        }
    }

    (wrapped, any_changed)
}

/// Builds a parent-index lookup table (child index → parent index).
fn build_parent_of(tree: &LayoutTree) -> Vec<Option<usize>> {
    let mut parent_of: Vec<Option<usize>> = vec![None; tree.len()];
    for idx in 0..tree.len() {
        for &child in tree.children_of(idx) {
            parent_of[child] = Some(idx);
        }
    }
    parent_of
}

/// Returns the available content width from the parent of element at `index`.
///
/// Falls back to the element's own computed width if it has no parent.
fn parent_content_width(
    tree: &LayoutTree,
    computed: &[ComputedLayout],
    parent_of: &[Option<usize>],
    index: usize,
) -> f32 {
    if let Some(parent_idx) = parent_of[index] {
        let parent = &tree.elements[parent_idx];
        computed[parent_idx].width - parent.padding.horizontal()
    } else {
        computed[index].width
    }
}

// ── Layout passes (free functions) ────────────────────────────────────────────

/// Bottom-up pass: set Fit container sizes and propagate `minDimensions`.
///
/// This runs before the BFS so that when a parent processes its children,
/// Fit containers already have a content-based initial size and every element
/// has its `min_width`/`min_height` floor computed.
///
/// Returns the content size of the element so that parent Fit elements can
/// account for it — even if this element is Grow (whose actual size is
/// determined later by `size_along_axis`). Without this, a Fit parent with
/// Grow children would see 0 and compute a collapsed height.
fn propagate_fit_sizes(
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

    let is_along = is_layout_axis(element.direction, axis);

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

    let gap_total = if is_along && children.len() > 1 {
        element.child_gap.value * (children.len() - 1).to_f32()
    } else {
        0.0
    };

    let content_size = content_acc + padding + if is_along { gap_total } else { 0.0 };
    let min_from_children = min_acc + padding + if is_along { gap_total } else { 0.0 };

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
fn size_along_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    root: usize,
    axis: Axis,
    viewport_size: f32,
) {
    // Set root size if it hasn't been set.
    let root_element = &tree.elements[root];
    let root_size = get_size(computed[root], axis);
    if root_size <= 0.0 {
        let new_size = match get_sizing(root_element, axis) {
            Sizing::Grow { min, max } | Sizing::Fit { min, max } => {
                viewport_size.clamp(min.value, max.value)
            },
            Sizing::Fixed(size) => size.value,
            Sizing::Percent(frac) => viewport_size * frac,
        };
        set_size(&mut computed[root], axis, new_size);
    }

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
        let is_along = is_layout_axis(parent_element.direction, axis);

        let padding = match axis {
            Axis::X => parent_element.padding.horizontal(),
            Axis::Y => parent_element.padding.vertical(),
        };

        let gap_total = if is_along && children.len() > 1 {
            parent_element.child_gap.value * (children.len() - 1).to_f32()
        } else {
            0.0
        };

        // Resolve Percent children first.
        let available_for_percent = parent_size - padding - gap_total;
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
                parent_size,
                padding,
                gap_total,
            );
        } else {
            size_children_cross_axis(tree, computed, children, axis, parent_size, padding);
        }

        // Enqueue children (reverse order so first child is popped first from stack).
        for &child_idx in children.iter().rev() {
            if !tree.children_of(child_idx).is_empty() {
                queue.push(child_idx);
            }
        }
    }
}

/// Size children that are laid out ALONG the parent's layout axis.
fn size_children_along_axis(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    parent_idx: usize,
    children: &[usize],
    axis: Axis,
    parent_size: f32,
    padding: f32,
    gap_total: f32,
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

    let available = parent_size - padding - gap_total;
    let mut to_distribute = available - content_size;

    // Overflow compression: largest-first heuristic.
    if to_distribute < 0.0 && !parent_element.clip {
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
) {
    let max_size = parent_size - padding;

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

/// Returns true if the bounding box is entirely outside the viewport.
const fn is_offscreen(x: f32, y: f32, w: f32, h: f32, vp_w: f32, vp_h: f32) -> bool {
    x > vp_w || y > vp_h || x + w < 0.0 || y + h < 0.0
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
    offscreen: bool,
) {
    if !offscreen && let Some(ref border) = element.border {
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

    if element.clip {
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
    offscreen: bool,
    font_scale: f32,
) {
    // Emit rectangle if background is set.
    if !offscreen && let Some(color) = element.background {
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
    if element.clip {
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
    if !offscreen
        && let ElementContent::Text {
            ref config,
            ref text,
        } = element.content
    {
        emit_text_commands(commands, wrapped, config, text, bounds, index, font_scale);
    }

    // Emit image render commands.
    if !offscreen && let ElementContent::Image { ref handle, tint } = element.content {
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
    config: &LayoutTextStyle,
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
            commands.push(RenderCommand {
                bounds:      BoundingBox {
                    x:      bounds.x,
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
    let parent_w = computed[index].width;
    let parent_h = computed[index].height;
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

    let main_available = if is_horizontal {
        parent_w - parent_el.padding.horizontal()
    } else {
        parent_h - parent_el.padding.vertical()
    };

    let extra_main = (main_available - children_main_size - gap_total).max(0.0);

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
        let child_w = computed[child_idx].width;
        let child_h = computed[child_idx].height;
        let child_main = if is_horizontal { child_w } else { child_h };

        reverse_cursor -= child_main;

        let (cx, cy) = if is_horizontal {
            let cross_available = parent_h - parent_el.padding.vertical();
            let cross_offset = match parent_el.child_align_y {
                AlignY::Top => 0.0,
                AlignY::Center => (cross_available - child_h).max(0.0) * 0.5,
                AlignY::Bottom => (cross_available - child_h).max(0.0),
            };
            (
                x + parent_el.padding.left.value + reverse_cursor,
                y + parent_el.padding.top.value + cross_offset,
            )
        } else {
            let cross_available = parent_w - parent_el.padding.horizontal();
            let cross_offset = match parent_el.child_align_x {
                AlignX::Left => 0.0,
                AlignX::Center => (cross_available - child_w).max(0.0) * 0.5,
                AlignX::Right => (cross_available - child_w).max(0.0),
            };
            (
                x + parent_el.padding.left.value + cross_offset,
                y + parent_el.padding.top.value + reverse_cursor,
            )
        };

        stack.push((child_idx, cx, cy, false));
        reverse_cursor -= parent_el.child_gap.value;
    }
}

/// DFS positioning pass. Computes final bounding boxes and emits render commands.
///
/// Elements whose bounding box lies entirely outside the viewport are
/// omitted from the command list (viewport culling).
fn position_and_render(
    tree: &LayoutTree,
    computed: &mut [ComputedLayout],
    root: usize,
    wrapped: &[Option<WrappedText>],
    viewport_width: f32,
    viewport_height: f32,
    font_scale: f32,
) -> Vec<RenderCommand> {
    let mut commands = Vec::with_capacity(tree.len() * 2);

    // Stack entries: (element_index, x, y, is_second_visit)
    let mut stack: Vec<(usize, f32, f32, bool)> = Vec::with_capacity(tree.len());
    stack.push((root, 0.0, 0.0, false));

    loop {
        let Some(&mut (index, x, y, ref mut visited)) = stack.last_mut() else {
            break;
        };
        let element = &tree.elements[index];
        let bounds = BoundingBox {
            x,
            y,
            width: computed[index].width,
            height: computed[index].height,
        };

        if *visited {
            let offscreen = is_offscreen(
                x,
                y,
                bounds.width,
                bounds.height,
                viewport_width,
                viewport_height,
            );
            emit_up_traversal_commands(
                tree,
                computed,
                &mut commands,
                element,
                bounds,
                index,
                offscreen,
            );
            stack.pop();
        } else {
            *visited = true;

            // Store the final bounding box (always, even if culled — computed
            // layout is the full picture, only render commands are filtered).
            computed[index].bounds = bounds;

            // Cull off-screen elements: skip render commands but still recurse
            // into children (a parent can be off-screen while children are on-screen
            // due to overflow).
            let offscreen = is_offscreen(
                x,
                y,
                bounds.width,
                bounds.height,
                viewport_width,
                viewport_height,
            );

            emit_down_traversal_commands(
                &mut commands,
                element,
                wrapped[index].as_ref(),
                bounds,
                index,
                offscreen,
                font_scale,
            );

            push_children_to_stack(tree, computed, &mut stack, index, x, y);
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

// ── Sizing heuristics (free functions) ───────────────────────────────────────

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

/// Expands Grow children using the smallest-first heuristic.
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

// ── Axis helpers ──────────────────────────────────────────────────────────────

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

/// Returns `true` if `direction` lays out children along the given axis.
const fn is_layout_axis(direction: Direction, axis: Axis) -> bool {
    match (direction, axis) {
        (Direction::LeftToRight, Axis::X) | (Direction::TopToBottom, Axis::Y) => true,
        (Direction::LeftToRight, Axis::Y) | (Direction::TopToBottom, Axis::X) => false,
    }
}

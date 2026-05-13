use std::sync::Arc;

use super::positioning;
use super::sizing;
use super::sizing::Axis;
use super::wrapping;
use super::wrapping::WrappedText;
use crate::layout::BoundingBox;
use crate::layout::Sizing;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;
use crate::layout::element::ElementContent;
use crate::layout::element::LayoutTree;
use crate::layout::render::RenderCommand;

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

/// Computed layout data for a single element.
#[derive(Clone, Copy, Debug, Default)]
pub struct ComputedLayout {
    /// Final bounding box in layout coordinates.
    pub bounds:                    BoundingBox,
    /// Width after sizing, before positioning.
    pub width:                     f32,
    /// Height after sizing, before positioning.
    pub height:                    f32,
    /// Propagated minimum width from children's content.
    ///
    /// Computed bottom-up alongside `propagate_fit_sizes`. Used as a hard
    /// floor during overflow compression and cross-axis sizing — an element
    /// must never shrink below its children's irreducible content size.
    pub(super) min_width:          f32,
    /// Propagated minimum height from children's content.
    pub(super) min_height:         f32,
    /// Cached natural (unwrapped) text width from `initialize_leaf_sizes`.
    ///
    /// Stored once during initial measurement so that `rewrap_text_elements`
    /// can check whether wrapping is needed without re-calling the measure
    /// function. Zero for non-text elements.
    pub(super) natural_text_width: f32,
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
/// The layout result keeps a complete render-command stream. Render-side
/// systems decide which commands are visible in the current viewport.
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
        sizing::propagate_fit_sizes(tree, &mut computed, root, Axis::X);
        sizing::propagate_fit_sizes(tree, &mut computed, root, Axis::Y);

        // Phase 1: Size along X axis (BFS top-down).
        sizing::size_along_axis(tree, &mut computed, root, Axis::X, viewport_width);

        // Phase 2: Re-wrap text elements within their resolved widths.
        // This may change text heights (more lines), so we re-propagate Y
        // and re-size along Y afterwards — but only if wrapping actually changed sizes.
        let (wrapped, text_sizes_changed) =
            wrapping::rewrap_text_elements(tree, &mut computed, &self.measure_text, font_scale);
        if text_sizes_changed {
            sizing::propagate_fit_sizes(tree, &mut computed, root, Axis::Y);
        }

        // Phase 3: Size along Y axis (BFS top-down) with wrap-corrected heights.
        sizing::size_along_axis(tree, &mut computed, root, Axis::Y, viewport_height);

        // Phase 4: Position elements and generate the complete render
        // command stream (DFS).
        let commands = positioning::position_and_render(
            tree,
            &mut computed,
            root,
            &wrapped,
            viewport_width,
            viewport_height,
            font_scale,
        );

        LayoutResult {
            computed,
            commands,
            wrapped,
            viewport_width,
            viewport_height,
            font_scale,
            structure_hash: tree.structure_hash(),
        }
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
    /// Computed layout for each `Element`, indexed by element index.
    pub computed:    Vec<ComputedLayout>,
    /// Render commands in draw order.
    pub commands:    Vec<RenderCommand>,
    wrapped:         Vec<Option<WrappedText>>,
    viewport_width:  f32,
    viewport_height: f32,
    font_scale:      f32,
    structure_hash:  u64,
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
    #[must_use = "callers use the content bounds to resolve fit-sized panels"]
    pub fn content_bounds(&self) -> Option<BoundingBox> { self.computed.get(1).map(|c| c.bounds) }

    /// Regenerates render commands from cached geometry and wrapped text data.
    pub fn regenerate_commands(&mut self, tree: &LayoutTree) {
        let Some(root) = tree.root else {
            debug_assert!(self.computed.is_empty());
            self.commands.clear();
            return;
        };

        debug_assert_eq!(self.computed.len(), tree.len());
        debug_assert_eq!(self.wrapped.len(), tree.len());
        debug_assert_eq!(self.structure_hash, tree.structure_hash());

        if self.computed.len() != tree.len() || self.wrapped.len() != tree.len() {
            return;
        }

        self.commands = positioning::render_commands_from_geometry(
            tree,
            &self.computed,
            root,
            &self.wrapped,
            self.viewport_width,
            self.viewport_height,
            self.font_scale,
        );
    }
}

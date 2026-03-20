//! Element tree representation for layout computation.
//!
//! [`Element`] is the data struct stored in the arena-based [`LayoutTree`]. It holds every
//! layout property (sizing, padding, direction, background, border, etc.) plus an
//! [`ElementContent`] that determines whether the node is a container, a text leaf, or empty.
//!
//! Users rarely construct `Element` directly. Instead, the [`El`](super::builder::El) builder
//! provides a fluent API that converts into an `Element` via `into_element()`. Think of `El`
//! as the ergonomic front door and `Element` as the canonical storage format.

use bevy::color::Color;
use smallvec::SmallVec;

use super::types::AlignX;
use super::types::AlignY;
use super::types::Border;
use super::types::Direction;
use super::types::Padding;
use super::types::Sizing;
use super::types::TextConfig;

/// A single element in the layout tree.
///
/// Elements are either containers (with children) or text leaves. The tree
/// is built via [`LayoutTree`] and then sized/positioned by the layout engine.
#[derive(Clone, Debug)]
pub(super) struct Element {
    /// Width sizing rule.
    pub(super) width:         Sizing,
    /// Height sizing rule.
    pub(super) height:        Sizing,
    /// Interior padding.
    pub(super) padding:       Padding,
    /// Gap between children along the layout axis.
    pub(super) child_gap:     f32,
    /// Direction children are laid out.
    pub(super) direction:     Direction,
    /// Horizontal alignment of children.
    pub(super) child_align_x: AlignX,
    /// Vertical alignment of children.
    pub(super) child_align_y: AlignY,
    /// Optional background color.
    pub(super) background:    Option<Color>,
    /// Optional border.
    pub(super) border:        Option<Border>,
    /// Whether this element clips overflowing children.
    pub(super) clip:          bool,
    /// Content of this element.
    pub(super) content:       ElementContent,
}

/// Inline capacity for child index lists. Most elements have 1–4 children;
/// only top-level containers (e.g., a column of many rows) exceed this and
/// spill to the heap.
const INLINE_CHILDREN: usize = 4;

/// What an element contains.
#[derive(Clone, Debug)]
pub(super) enum ElementContent {
    /// Container with child element indices.
    Children(SmallVec<[usize; INLINE_CHILDREN]>),
    /// Text leaf.
    Text {
        /// The text string.
        text:   String,
        /// Text configuration.
        config: TextConfig,
    },
    /// Empty (no children, no text).
    Empty,
}

impl Default for Element {
    fn default() -> Self {
        Self {
            width:         Sizing::FIT,
            height:        Sizing::FIT,
            padding:       Padding::default(),
            child_gap:     0.0,
            direction:     Direction::default(),
            child_align_x: AlignX::default(),
            child_align_y: AlignY::default(),
            background:    None,
            border:        None,
            clip:          false,
            content:       ElementContent::Empty,
        }
    }
}

/// Arena-based layout tree.
///
/// Elements are stored in a flat `Vec` and reference children by index.
/// The first element (index 0) is always the root.
#[derive(Clone, Debug, Default)]
pub struct LayoutTree {
    /// All elements in insertion order.
    pub(super) elements:    Vec<Element>,
    /// Index of the root element.
    pub(super) root:        Option<usize>,
    /// Hash of layout-relevant fields (excludes colors).
    ///
    /// Computed once by [`LayoutBuilder::build()`]. Two trees with the same
    /// `layout_hash` have identical structure and sizing — only render-only
    /// properties like text color or background color may differ.
    pub(super) layout_hash: u64,
}

impl LayoutTree {
    /// Creates a new empty layout tree.
    #[must_use]
    pub fn new() -> Self { Self::default() }

    /// Creates a new empty layout tree with pre-allocated capacity.
    ///
    /// Use this when you know the approximate element count upfront to
    /// avoid reallocation during tree construction.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            elements:    Vec::with_capacity(capacity),
            root:        None,
            layout_hash: 0,
        }
    }

    /// Adds an element and returns its index.
    pub(super) fn add(&mut self, element: Element) -> usize {
        let index = self.elements.len();
        self.elements.push(element);
        index
    }

    /// Adds an element as a child of the given parent.
    ///
    /// Returns the child's index.
    pub(super) fn add_child(&mut self, parent: usize, element: Element) -> usize {
        let child_index = self.add(element);
        if let Some(parent_element) = self.elements.get_mut(parent) {
            match &mut parent_element.content {
                ElementContent::Children(children) => {
                    children.push(child_index);
                },
                ElementContent::Empty => {
                    parent_element.content =
                        ElementContent::Children(SmallVec::from_elem(child_index, 1));
                },
                ElementContent::Text { .. } => {
                    // Text elements cannot have children — this is a programming error.
                    // In release builds we silently ignore it; debug builds will catch it
                    // via the orphan check in layout computation.
                },
            }
        }
        child_index
    }

    /// Sets the root element index.
    pub(super) const fn set_root(&mut self, index: usize) { self.root = Some(index); }

    /// Returns an iterator over child indices of the given element.
    #[must_use]
    pub(super) fn children_of(&self, index: usize) -> &[usize] {
        self.elements
            .get(index)
            .map_or(&[], |element| match &element.content {
                ElementContent::Children(children) => children.as_slice(),
                _ => &[],
            })
    }

    /// Returns the number of elements in the tree.
    #[must_use]
    pub const fn len(&self) -> usize { self.elements.len() }

    /// Returns `true` if the tree has no elements.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.elements.is_empty() }

    /// Returns the layout hash (excludes colors).
    ///
    /// Two trees with the same hash have identical structure and sizing —
    /// only render-only properties like text color may differ.
    #[must_use]
    pub const fn layout_hash(&self) -> u64 { self.layout_hash }

    /// Returns the render-only colors for the element at `index`, or `None`
    /// if the index is out of bounds.
    #[must_use]
    pub fn element_colors_at(&self, index: usize) -> Option<ElementColors> {
        let element = self.elements.get(index)?;
        Some(ElementColors {
            text:       match &element.content {
                ElementContent::Text { config, .. } => Some(config.color()),
                _ => None,
            },
            background: element.background,
            border:     element.border.map(|b| b.color),
        })
    }

    /// Visits every element and lets the caller update render-only colors.
    ///
    /// The closure receives the element index and an [`ElementColors`] view
    /// that exposes text color, background color, and border color. Mutating
    /// these does not affect layout — the [`layout_hash`](Self::layout_hash)
    /// remains unchanged, so the layout system will take the color-only fast
    /// path automatically.
    ///
    /// ```ignore
    /// tree.recolor(|idx, colors| {
    ///     colors.set_text(Color::RED);
    ///     colors.set_background(Some(Color::BLUE));
    ///     colors.set_border(Color::BLACK);
    /// });
    /// ```
    pub fn recolor(&mut self, mut f: impl FnMut(usize, &mut ElementColors)) {
        for (idx, element) in self.elements.iter_mut().enumerate() {
            let mut colors = ElementColors {
                text:       match &element.content {
                    ElementContent::Text { config, .. } => Some(config.color()),
                    _ => None,
                },
                background: element.background,
                border:     element.border.map(|b| b.color),
            };
            f(idx, &mut colors);
            // Write back.
            if let ElementContent::Text { config, .. } = &mut element.content {
                if let Some(c) = colors.text {
                    config.set_color(c);
                }
            }
            element.background = colors.background;
            if let Some(border) = &mut element.border {
                if let Some(c) = colors.border {
                    border.color = c;
                }
            }
        }
    }
}

/// Mutable view of an element's render-only color properties.
///
/// Passed to the closure in [`LayoutTree::recolor`]. Only non-`None` fields
/// are applicable — `text` is `None` for non-text elements, `border` is
/// `None` for elements without a border.
pub struct ElementColors {
    /// Text color (`None` for non-text elements).
    pub text:       Option<Color>,
    /// Background fill color.
    pub background: Option<Color>,
    /// Border color (`None` for elements without a border).
    pub border:     Option<Color>,
}

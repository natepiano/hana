//! Element tree representation for layout computation.

use super::types::AlignX;
use super::types::AlignY;
use super::types::BackgroundColor;
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
pub struct Element {
    /// Optional debug name for this element.
    pub name: Option<String>,
    /// Width sizing rule.
    pub width: Sizing,
    /// Height sizing rule.
    pub height: Sizing,
    /// Interior padding.
    pub padding: Padding,
    /// Gap between children along the layout axis.
    pub child_gap: f32,
    /// Direction children are laid out.
    pub direction: Direction,
    /// Horizontal alignment of children.
    pub align_x: AlignX,
    /// Vertical alignment of children.
    pub align_y: AlignY,
    /// Optional background color.
    pub background: Option<BackgroundColor>,
    /// Optional border.
    pub border: Option<Border>,
    /// Whether this element clips overflowing children.
    pub clip: bool,
    /// Content of this element.
    pub content: ElementContent,
}

/// What an element contains.
#[derive(Clone, Debug)]
pub enum ElementContent {
    /// Container with child element indices.
    Children(Vec<usize>),
    /// Text leaf.
    Text {
        /// The text string.
        text: String,
        /// Text configuration.
        config: TextConfig,
    },
    /// Empty (no children, no text).
    Empty,
}

impl Default for Element {
    fn default() -> Self {
        Self {
            name: None,
            width: Sizing::FIT,
            height: Sizing::FIT,
            padding: Padding::default(),
            child_gap: 0.0,
            direction: Direction::default(),
            align_x: AlignX::default(),
            align_y: AlignY::default(),
            background: None,
            border: None,
            clip: false,
            content: ElementContent::Empty,
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
    pub elements: Vec<Element>,
    /// Index of the root element.
    pub root: Option<usize>,
}

impl LayoutTree {
    /// Creates a new empty layout tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an element and returns its index.
    pub fn add(&mut self, element: Element) -> usize {
        let index = self.elements.len();
        self.elements.push(element);
        index
    }

    /// Adds an element as a child of the given parent.
    ///
    /// Returns the child's index.
    pub fn add_child(&mut self, parent: usize, element: Element) -> usize {
        let child_index = self.add(element);
        if let Some(parent_element) = self.elements.get_mut(parent) {
            match &mut parent_element.content {
                ElementContent::Children(children) => {
                    children.push(child_index);
                }
                ElementContent::Empty => {
                    parent_element.content = ElementContent::Children(vec![child_index]);
                }
                ElementContent::Text { .. } => {
                    // Text elements cannot have children — this is a programming error.
                    // In release builds we silently ignore it; debug builds will catch it
                    // via the orphan check in layout computation.
                }
            }
        }
        child_index
    }

    /// Sets the root element index.
    pub const fn set_root(&mut self, index: usize) {
        self.root = Some(index);
    }

    /// Returns an iterator over child indices of the given element.
    #[must_use]
    pub fn children_of(&self, index: usize) -> &[usize] {
        self.elements
            .get(index)
            .map_or(&[], |element| match &element.content {
                ElementContent::Children(children) => children.as_slice(),
                _ => &[],
            })
    }

    /// Returns the number of elements in the tree.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.elements.len()
    }

    /// Returns `true` if the tree has no elements.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

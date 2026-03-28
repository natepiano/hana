//! Element tree representation for layout computation.
//!
//! [`Element`] is the data struct stored in the arena-based [`LayoutTree`]. It holds every
//! layout property (sizing, padding, direction, background, border, etc.) plus an
//! [`ElementContent`] that determines whether the node is a container, a text leaf, or empty.
//!
//! Users rarely construct `Element` directly. Instead, the [`El`](super::builder::El) builder
//! provides a fluent API that converts into an `Element` via `into_element()`. Think of `El`
//! as the ergonomic front door and `Element` as the canonical storage format.

use bevy::asset::Handle;
use bevy::color::Color;
use bevy::image::Image;
use bevy::pbr::StandardMaterial;
use smallvec::SmallVec;

use super::types::AlignX;
use super::types::AlignY;
use super::types::Border;
use super::types::Dimension;
use super::types::Direction;
use super::types::LayoutTextStyle;
use super::types::Padding;
use super::types::Sizing;
use super::types::Unit;

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
    pub(super) child_gap:     Dimension,
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
    /// Optional PBR material override for this element's surface (backgrounds, borders).
    /// When present, the rendering system uses this instead of the panel-level default.
    /// `base_color` is overridden by the layout color if both are set.
    pub(super) material:      Option<Box<StandardMaterial>>,
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
        config: LayoutTextStyle,
    },
    /// Image leaf — rendered as a textured quad.
    Image {
        /// Handle to the image asset.
        handle: Handle<Image>,
        /// Tint color multiplied against the texture (white = no tint).
        tint:   Color,
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
            child_gap:     Dimension {
                value: 0.0,
                unit:  None,
            },
            direction:     Direction::default(),
            child_align_x: AlignX::default(),
            child_align_y: AlignY::default(),
            background:    None,
            border:        None,
            clip:          false,
            material:      None,
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
    pub(super) elements: Vec<Element>,
    /// Index of the root element.
    pub(super) root:     Option<usize>,
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
            elements: Vec::with_capacity(capacity),
            root:     None,
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
                ElementContent::Text { .. } | ElementContent::Image { .. } => {
                    // Leaf elements cannot have children — this is a programming error.
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

    /// Returns the PBR material override for the element at `index`, if any.
    #[must_use]
    pub fn element_material(&self, index: usize) -> Option<&StandardMaterial> {
        self.elements.get(index).and_then(|e| e.material.as_deref())
    }

    /// Returns a copy of this tree with all dimensions converted to points.
    ///
    /// `layout_scale` multiplies spatial values (padding, gaps, borders, fixed sizes).
    /// `font_scale` multiplies font-related values (size, line height, letter/word spacing).
    ///
    /// Used by the panel layout system to ensure the layout engine and parley
    /// always operate in points, avoiding parley's integer quantization at small sizes.
    #[must_use]
    pub fn scaled(&self, layout_scale: f32, font_scale: f32) -> Self {
        let mut tree = self.clone();
        for element in &mut tree.elements {
            element.width = element.width.resolved(layout_scale);
            element.height = element.height.resolved(layout_scale);
            element.padding = element.padding.resolved(layout_scale);
            element.child_gap = Dimension {
                value: element.child_gap.to_points(layout_scale),
                unit:  None,
            };
            if let Some(ref mut border) = element.border {
                *border = border.resolved(layout_scale);
            }
            if let ElementContent::Text { ref mut config, .. } = element.content {
                // If this text element carries an explicit unit (e.g., from
                // `LayoutTextStyle::new(Mm(6.0))`), convert from that unit to
                // points directly. Otherwise use the panel-wide font_scale.
                let scale = config.unit().map_or(font_scale, Unit::to_points);
                *config = config.scaled(scale);
            }
        }
        tree
    }
}

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

use super::AlignX;
use super::AlignY;
use super::Border;
use super::CornerRadius;
use super::Dimension;
use super::Direction;
use super::LayoutTextStyle;
use super::Padding;
use super::Sizing;
use super::Unit;
use super::constants::INLINE_CHILDREN;

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
    /// Corner radius for rounded backgrounds and borders.
    pub(super) corner_radius: CornerRadius,
    /// Whether this element clips overflowing children.
    pub(super) clip:          bool,
    /// Optional PBR material override for this element's surface (backgrounds, borders).
    /// When present, the rendering system uses this instead of the panel-level default.
    /// `base_color` is overridden by the layout color if both are set.
    pub(super) material:      Option<Box<StandardMaterial>>,
    /// Content of this element.
    pub(super) content:       ElementContent,
}

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

/// Classifies the difference between two layout trees.
#[cfg(feature = "bench_support")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutTreeChange {
    /// Trees are exactly identical for the fields this classifier inspects.
    Identical,
    /// Trees differ only in fields that should not affect layout bounds.
    VisualOnly,
    /// Trees differ in structure, sizing, measurement, or placement fields.
    LayoutAffecting,
}

#[cfg(feature = "bench_support")]
impl LayoutTreeChange {
    const fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::LayoutAffecting, _) | (_, Self::LayoutAffecting) => Self::LayoutAffecting,
            (Self::VisualOnly, _) | (_, Self::VisualOnly) => Self::VisualOnly,
            (Self::Identical, Self::Identical) => Self::Identical,
        }
    }
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
            corner_radius: CornerRadius::ZERO,
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

    /// Changes the root element's width sizing to `Grow { min, max }`.
    ///
    /// Used by `build_screen_space()` when the panel width is dynamic
    /// (e.g. `Percent` or `Grow`) so that changing `panel.width`
    /// triggers correct reflow without a tree rebuild. Pass
    /// `Dimension { value: 0.0, unit: None }` for `min` and
    /// `Dimension { value: f32::MAX, unit: None }` for `max` to match
    /// the previous unbounded behavior.
    pub(super) fn set_root_grow_width(&mut self, min: Dimension, max: Dimension) {
        if let Some(root) = self.root
            && let Some(element) = self.elements.get_mut(root)
        {
            element.width = Sizing::Grow { min, max };
        }
    }

    /// Changes the root element's height sizing to `Grow { min, max }`.
    ///
    /// See [`set_root_grow_width`](Self::set_root_grow_width) for rationale.
    pub(super) fn set_root_grow_height(&mut self, min: Dimension, max: Dimension) {
        if let Some(root) = self.root
            && let Some(element) = self.elements.get_mut(root)
        {
            element.height = Sizing::Grow { min, max };
        }
    }

    /// Changes the root element's width sizing to `FIT { min, max }`.
    ///
    /// Paired with `DiegeticPanel::screen().size(Sizing::Fit { .. }, _)` so
    /// the two-pass layout (`propagate_fit_sizes` bottom-up +
    /// `size_along_axis` top-down) resolves root to its natural content
    /// width, clamped to `[min, max]`.
    pub(super) fn set_root_fit_width(&mut self, min: Dimension, max: Dimension) {
        if let Some(root) = self.root
            && let Some(element) = self.elements.get_mut(root)
        {
            element.width = Sizing::Fit { min, max };
        }
    }

    /// Changes the root element's height sizing to `FIT { min, max }`.
    ///
    /// See [`set_root_fit_width`](Self::set_root_fit_width) for rationale.
    pub(super) fn set_root_fit_height(&mut self, min: Dimension, max: Dimension) {
        if let Some(root) = self.root
            && let Some(element) = self.elements.get_mut(root)
        {
            element.height = Sizing::Fit { min, max };
        }
    }

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

    /// Classifies whether `next` differs from this tree only in render-only
    /// fields.
    #[cfg(feature = "bench_support")]
    #[must_use]
    pub fn classify_change(&self, next: &Self) -> LayoutTreeChange {
        if self.root != next.root || self.elements.len() != next.elements.len() {
            return LayoutTreeChange::LayoutAffecting;
        }

        let mut change = LayoutTreeChange::Identical;
        for (element, next_element) in self.elements.iter().zip(&next.elements) {
            change = change.combine(classify_element_change(element, next_element));
            if change == LayoutTreeChange::LayoutAffecting {
                return change;
            }
        }
        change
    }

    /// Returns the PBR material override for the element at `index`, if any.
    #[must_use]
    pub fn element_material(&self, index: usize) -> Option<&StandardMaterial> {
        self.elements.get(index).and_then(|e| e.material.as_deref())
    }

    /// Returns the corner radius for the element at `index`.
    #[must_use]
    pub fn element_corner_radius(&self, index: usize) -> CornerRadius {
        self.elements
            .get(index)
            .map_or(CornerRadius::ZERO, |e| e.corner_radius)
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
            element.corner_radius = element.corner_radius.resolved(layout_scale);
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

#[cfg(feature = "bench_support")]
fn classify_element_change(element: &Element, next: &Element) -> LayoutTreeChange {
    if element.width != next.width
        || element.height != next.height
        || element.padding != next.padding
        || element.child_gap != next.child_gap
        || element.direction != next.direction
        || element.child_align_x != next.child_align_x
        || element.child_align_y != next.child_align_y
        || element.clip != next.clip
    {
        return LayoutTreeChange::LayoutAffecting;
    }

    let border_change = classify_border_change(element.border, next.border);
    if border_change == LayoutTreeChange::LayoutAffecting {
        return LayoutTreeChange::LayoutAffecting;
    }

    let mut change = border_change;
    if element.background != next.background
        || element.corner_radius != next.corner_radius
        || element.material.is_some()
        || next.material.is_some()
    {
        change = change.combine(LayoutTreeChange::VisualOnly);
    }

    change.combine(classify_content_change(&element.content, &next.content))
}

#[cfg(feature = "bench_support")]
fn classify_border_change(border: Option<Border>, next: Option<Border>) -> LayoutTreeChange {
    match (border, next) {
        (None, None) => LayoutTreeChange::Identical,
        (Some(border), Some(next)) => {
            if border.left != next.left
                || border.right != next.right
                || border.top != next.top
                || border.bottom != next.bottom
                || border.between_children != next.between_children
            {
                LayoutTreeChange::LayoutAffecting
            } else if border.color != next.color {
                LayoutTreeChange::VisualOnly
            } else {
                LayoutTreeChange::Identical
            }
        },
        (None, Some(_)) | (Some(_), None) => LayoutTreeChange::LayoutAffecting,
    }
}

#[cfg(feature = "bench_support")]
fn classify_content_change(content: &ElementContent, next: &ElementContent) -> LayoutTreeChange {
    match (content, next) {
        (ElementContent::Children(children), ElementContent::Children(next_children)) => {
            if children == next_children {
                LayoutTreeChange::Identical
            } else {
                LayoutTreeChange::LayoutAffecting
            }
        },
        (
            ElementContent::Text { text, config },
            ElementContent::Text {
                text: next_text,
                config: next_config,
            },
        ) => {
            if text != next_text || !config.layout_eq_excluding_visuals(next_config) {
                LayoutTreeChange::LayoutAffecting
            } else if config != next_config {
                LayoutTreeChange::VisualOnly
            } else {
                LayoutTreeChange::Identical
            }
        },
        (
            ElementContent::Image { handle, tint },
            ElementContent::Image {
                handle: next_handle,
                tint: next_tint,
            },
        ) => {
            if handle == next_handle && tint == next_tint {
                LayoutTreeChange::Identical
            } else {
                LayoutTreeChange::VisualOnly
            }
        },
        (ElementContent::Empty, ElementContent::Empty) => LayoutTreeChange::Identical,
        _ => LayoutTreeChange::LayoutAffecting,
    }
}

//! Ergonomic builder for constructing layout trees.
//!
//! [`El`] is a lightweight builder that mirrors every layout property on
//! [`Element`](super::element::Element) but exposes them as a fluent chain. When added to the
//! tree, `El` converts itself into an `Element` via `into_element()` — it exists purely for
//! ergonomics so users never have to construct `Element` or `ElementContent` by hand.
//!
//! [`LayoutBuilder`] manages parent-child nesting with an internal stack. Calling
//! `.with(el, |b| { ... })` pushes a parent, runs the closure, and pops — so there are no
//! open/close pairs to get wrong.
//!
//! The closure-based nesting API is inspired by Clay's C API:
//!
//! ```ignore
//! let tree = LayoutBuilder::new(160.0, 160.0)
//!     .with(El::new().width(Sizing::GROW).height(Sizing::GROW).padding(Padding::all(8.0))
//!           .direction(Direction::TopToBottom).background(Color::srgb_u8(180, 96, 122)),
//!         |b| {
//!             b.text("STATUS", TextConfig::new(7));
//!             b.with(El::new().width(Sizing::GROW).height(Sizing::fixed(4.0))
//!                    .background(Color::srgb_u8(74, 196, 172)),
//!                 |_| {},
//!             );
//!         },
//!     )
//!     .build();
//! ```

use super::element::Element;
use super::element::ElementContent;
use super::element::LayoutTree;
use super::types::AlignX;
use super::types::AlignY;
use super::types::Border;
use super::types::Direction;
use super::types::Padding;
use super::types::Sizing;
use super::types::TextConfig;
use bevy::color::Color;

/// Shorthand element declaration for the builder API.
///
/// This is a temporary configuration object that gets converted into an [`Element`]
/// when added to the tree.
#[must_use]
#[derive(Clone, Debug, Default)]
pub struct El {
    name: Option<String>,
    width: Sizing,
    height: Sizing,
    padding: Padding,
    child_gap: f32,
    direction: Direction,
    child_align_x: AlignX,
    child_align_y: AlignY,
    background: Option<Color>,
    border: Option<Border>,
    clip: bool,
}

impl El {
    /// Creates a new element declaration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the element name (for debugging).
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the width sizing rule.
    pub const fn width(mut self, sizing: Sizing) -> Self {
        self.width = sizing;
        self
    }

    /// Sets the height sizing rule.
    pub const fn height(mut self, sizing: Sizing) -> Self {
        self.height = sizing;
        self
    }

    /// Sets padding on all sides.
    pub const fn padding(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }

    /// Sets the gap between children.
    pub const fn child_gap(mut self, gap: f32) -> Self {
        self.child_gap = gap;
        self
    }

    /// Sets the layout direction.
    pub const fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Sets both horizontal and vertical child alignment.
    pub const fn child_alignment(mut self, x: AlignX, y: AlignY) -> Self {
        self.child_align_x = x;
        self.child_align_y = y;
        self
    }

    /// Sets horizontal child alignment.
    pub const fn child_align_x(mut self, align: AlignX) -> Self {
        self.child_align_x = align;
        self
    }

    /// Sets vertical child alignment.
    pub const fn child_align_y(mut self, align: AlignY) -> Self {
        self.child_align_y = align;
        self
    }

    /// Sets a background color.
    pub const fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Sets a border.
    pub const fn border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }

    /// Enables clipping of overflowing children.
    pub const fn clip(mut self) -> Self {
        self.clip = true;
        self
    }

    /// Converts this declaration into an [`Element`] with the given content.
    fn into_element(self, content: ElementContent) -> Element {
        Element {
            name: self.name,
            width: self.width,
            height: self.height,
            padding: self.padding,
            child_gap: self.child_gap,
            direction: self.direction,
            child_align_x: self.child_align_x,
            child_align_y: self.child_align_y,
            background: self.background,
            border: self.border,
            clip: self.clip,
            content,
        }
    }
}

/// Builds a [`LayoutTree`] using a closure-based nesting API.
pub struct LayoutBuilder {
    tree: LayoutTree,
    /// Stack of parent indices for nesting.
    parent_stack: Vec<usize>,
}

impl LayoutBuilder {
    /// Creates a new builder with a root element sized to the given dimensions.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        let mut tree = LayoutTree::new();
        let root = tree.add(Element {
            width: Sizing::Fixed(width),
            height: Sizing::Fixed(height),
            ..Element::default()
        });
        tree.set_root(root);

        Self {
            tree,
            parent_stack: vec![root],
        }
    }

    /// Creates a builder with a custom root element declaration.
    #[must_use]
    pub fn with_root(el: El) -> Self {
        let mut tree = LayoutTree::new();
        let root = tree.add(el.into_element(ElementContent::Empty));
        tree.set_root(root);

        Self {
            tree,
            parent_stack: vec![root],
        }
    }

    /// Adds a container element with children defined by the closure.
    pub fn with(&mut self, el: El, children: impl FnOnce(&mut Self)) -> &mut Self {
        let parent = self.current_parent();
        let index = self
            .tree
            .add_child(parent, el.into_element(ElementContent::Empty));
        self.parent_stack.push(index);
        children(self);
        self.parent_stack.pop();
        self
    }

    /// Adds a text element as a child of the current parent.
    pub fn text(&mut self, text: impl Into<String>, config: TextConfig) -> &mut Self {
        let parent = self.current_parent();
        self.tree.add_child(
            parent,
            Element {
                content: ElementContent::Text {
                    text: text.into(),
                    config,
                },
                ..Element::default()
            },
        );
        self
    }

    /// Finishes building and returns the layout tree.
    #[must_use]
    pub fn build(self) -> LayoutTree {
        self.tree
    }

    /// Returns the current parent index.
    fn current_parent(&self) -> usize {
        self.parent_stack.last().copied().unwrap_or(0)
    }
}

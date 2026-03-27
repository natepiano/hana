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

use bevy::color::Color;

use super::element::Element;
use super::element::ElementContent;
use super::element::LayoutTree;
use super::types::AlignX;
use super::types::AlignY;
use super::types::Border;
use super::types::Dimension;
use super::types::Direction;
use super::types::LayoutTextStyle;
use super::types::Padding;
use super::types::Sizing;

/// Shorthand element declaration for the builder API.
///
/// This is a temporary configuration object that gets converted into an [`Element`]
/// when added to the tree.
#[must_use]
#[derive(Clone, Debug, Default)]
pub struct El {
    width:         Sizing,
    height:        Sizing,
    padding:       Padding,
    child_gap:     f32,
    direction:     Direction,
    child_align_x: AlignX,
    child_align_y: AlignY,
    background:    Option<Color>,
    border:        Option<Border>,
    clip:          bool,
}

impl El {
    /// Creates a new element declaration with default settings.
    pub fn new() -> Self { Self::default() }

    /// Sets the width sizing rule.
    ///
    /// Common patterns: [`Sizing::GROW`], [`Sizing::FIT`], [`Sizing::fixed`], [`Sizing::percent`].
    pub const fn width(mut self, sizing: Sizing) -> Self {
        self.width = sizing;
        self
    }

    /// Sets the height sizing rule.
    ///
    /// Common patterns: [`Sizing::GROW`], [`Sizing::FIT`], [`Sizing::fixed`], [`Sizing::percent`].
    pub const fn height(mut self, sizing: Sizing) -> Self {
        self.height = sizing;
        self
    }

    /// Sets padding on all sides.
    pub const fn padding(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }

    /// Sets the gap between adjacent children along the layout direction.
    pub fn child_gap(mut self, gap: impl Into<Dimension>) -> Self {
        self.child_gap = gap.into().value;
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
    const fn into_element(self, content: ElementContent) -> Element {
        Element {
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
    tree:         LayoutTree,
    /// Stack of parent indices for nesting.
    parent_stack: Vec<usize>,
}

impl LayoutBuilder {
    /// Creates a new builder with an implicit fixed-size root.
    ///
    /// This is the "layout inside a viewport/canvas" constructor. The builder
    /// inserts a root element whose width and height are [`Sizing::Fixed`],
    /// using the provided `width` and `height`.
    ///
    /// That means the returned tree always has an outer box of exactly this
    /// size, even if the visible content inside it shrink-wraps smaller. This
    /// is useful when you want a stable layout viewport for:
    ///
    /// - mapping layout units to world-space dimensions,
    /// - wrapping text against a known maximum width,
    /// - `Grow` / `Percent` sizing against a known parent size,
    /// - keeping panel dimensions stable while content changes.
    ///
    /// Use [`Self::with_root`] instead when you do not want this extra fixed
    /// wrapper and want the root element itself to be content-driven (`Fit`),
    /// growable, or otherwise fully caller-defined.
    #[must_use]
    pub fn new(width: impl Into<Dimension>, height: impl Into<Dimension>) -> Self {
        let mut tree = LayoutTree::new();
        let root = tree.add(Element {
            width: Sizing::Fixed(width.into().value),
            height: Sizing::Fixed(height.into().value),
            ..Element::default()
        });
        tree.set_root(root);

        Self {
            tree,
            parent_stack: vec![root],
        }
    }

    /// Like [`Self::new`] but pre-allocates capacity for the element vec.
    ///
    /// Each row of content typically creates 3–5 elements. Pre-allocating
    /// avoids repeated vec reallocations during tree construction.
    #[must_use]
    pub fn with_capacity(
        width: impl Into<Dimension>,
        height: impl Into<Dimension>,
        capacity: usize,
    ) -> Self {
        let mut tree = LayoutTree::with_capacity(capacity);
        let root = tree.add(Element {
            width: Sizing::Fixed(width.into().value),
            height: Sizing::Fixed(height.into().value),
            ..Element::default()
        });
        tree.set_root(root);

        Self {
            tree,
            parent_stack: vec![root],
        }
    }

    /// Creates a new builder with a caller-supplied root element.
    ///
    /// This is the "my visible panel *is* the root" constructor. Unlike
    /// [`Self::new`], it does not insert an implicit fixed-size wrapper first.
    /// The `El` you provide becomes the actual root of the layout tree.
    ///
    /// Use this when you want the root itself to control sizing, for example:
    ///
    /// - a `Fit` root that grows with its content,
    /// - a root with its own border/background/padding,
    /// - a root constrained by `fit_range` rather than fixed dimensions,
    /// - a tree where the computed root bounds should reflect the visible panel rather than an
    ///   invisible outer viewport.
    ///
    /// Note that this only changes the layout tree structure. It does not
    /// remove the need for higher-level code to decide how layout units map to
    /// world space.
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

    /// Adds a child container under the current parent, then fills it in.
    ///
    /// The passed `El` is converted into an [`Element`] and inserted as a
    /// child of whatever the current parent is:
    ///
    /// - after [`Self::new`], the initial current parent is the implicit fixed-size root inserted
    ///   by the builder,
    /// - after [`Self::with_root`], the initial current parent is the custom root you supplied.
    ///
    /// The closure runs with this newly inserted child pushed as the current
    /// parent, so nested calls to `.with(...)` or `.text(...)` add descendants
    /// inside it. When the closure returns, the parent stack is restored.
    ///
    /// In other words, `.with(...)` always creates another node in the tree.
    /// It does not modify the existing root element; choose that root up front
    /// with [`Self::new`] or [`Self::with_root`].
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

    /// Adds a text leaf as a child of the current parent.
    ///
    /// Like [`Self::with`], this inserts a new node under the current parent:
    ///
    /// - after [`Self::new`], that initially means the implicit fixed-size root,
    /// - after [`Self::with_root`], that initially means your custom root,
    /// - inside a `.with(...)` closure, it means the container introduced by that `.with(...)`
    ///   call.
    ///
    /// Text nodes are leaves, not containers, so they cannot receive children
    /// of their own. Use [`Self::with`] when you want to create another nested
    /// container instead of a text leaf.
    pub fn text(&mut self, text: impl Into<String>, config: LayoutTextStyle) -> &mut Self {
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
    pub fn build(self) -> LayoutTree { self.tree }

    /// Returns the current parent index.
    fn current_parent(&self) -> usize { self.parent_stack.last().copied().unwrap_or(0) }
}

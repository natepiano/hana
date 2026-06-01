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

use bevy::asset::Handle;
use bevy::color::Color;
use bevy::image::Image;
use bevy::math::Vec2;
use bevy::pbr::StandardMaterial;

use super::AlignX;
use super::AlignY;
use super::Border;
use super::CornerRadius;
use super::Dimension;
use super::Direction;
use super::Padding;
use super::Sizing;
use super::TextStyle;
use super::element::ChildOverflow;
use super::element::Element;
use super::element::ElementContent;
use super::element::LayoutTree;
use super::element::ScrollAnchor;
use crate::DimensionMatch;
use crate::ImeEditableFieldSpec;
use crate::ImePanelField;
use crate::PanelFieldId;

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
    child_gap:     Dimension,
    direction:     Direction,
    child_align_x: AlignX,
    child_align_y: AlignY,
    background:    Option<Color>,
    border:        Option<Border>,
    corner_radius: CornerRadius,
    overflow:      ChildOverflow,
    scroll_offset: Vec2,
    scroll_anchor: ScrollAnchor,
    material:      Option<Box<StandardMaterial>>,
    editable:      Option<ImePanelField>,
}

impl El {
    /// Creates a new element declaration with default settings.
    pub fn new() -> Self { Self::default() }

    /// Sets the width sizing rule.
    ///
    /// Can be overridden by a subsequent `.size()` call (last wins).
    ///
    /// Common patterns: [`Sizing::GROW`], [`Sizing::FIT`], [`Sizing::fixed`], [`Sizing::percent`].
    pub const fn width(mut self, sizing: Sizing) -> Self {
        self.width = sizing;
        self
    }

    /// Sets the height sizing rule.
    ///
    /// Can be overridden by a subsequent `.size()` call (last wins).
    ///
    /// Common patterns: [`Sizing::GROW`], [`Sizing::FIT`], [`Sizing::fixed`], [`Sizing::percent`].
    pub const fn height(mut self, sizing: Sizing) -> Self {
        self.height = sizing;
        self
    }

    /// Sets both width and height to [`Sizing::fixed`] from two matching dimensions.
    ///
    /// Bare floats inherit the panel's layout unit. Typed wrappers like
    /// [`Mm`](crate::Mm) or [`Pt`](crate::Pt) set the unit explicitly.
    /// Both arguments must have the same type; use `.width(...)` and
    /// `.height(...)` separately when you intentionally want different
    /// unit types on each axis.
    ///
    /// Can be overridden by subsequent `.width()` or `.height()` calls
    /// (last wins).
    pub fn size<DM: DimensionMatch>(self, w: DM, h: DM) -> Self {
        let wd = w.into();
        let hd = h.into();
        self.width(Sizing::fixed(wd)).height(Sizing::fixed(hd))
    }

    /// Sets padding on all sides.
    pub const fn padding(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }

    /// Sets the gap between adjacent children along the layout direction.
    pub fn child_gap(mut self, gap: impl Into<Dimension>) -> Self {
        self.child_gap = gap.into();
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

    /// Sets the corner radius for rounded backgrounds and borders.
    ///
    /// Accepts `CornerRadius::all(8.0)`, `CornerRadius::new(tl, tr, br, bl)`,
    /// or a bare `f32` for uniform radius on all corners.
    pub fn corner_radius(mut self, radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = radius.into();
        self
    }

    /// Sets overflow to `Clipped`; default is `Visible`.
    pub const fn clip(mut self) -> Self {
        self.overflow = ChildOverflow::Clipped;
        self
    }

    /// Scrolls children vertically by `offset` logical px from the top and clips
    /// overflow.
    ///
    /// The offset is clamped during positioning to `[0, content - viewport]`;
    /// pass `f32::MAX` to pin to the bottom.
    pub const fn scroll_y(mut self, offset: f32) -> Self {
        self.scroll_offset.y = offset;
        self.scroll_anchor = ScrollAnchor::Start;
        self.overflow = ChildOverflow::Clipped;
        self
    }

    /// Scrolls children vertically by `scrollback` logical px measured from the
    /// bottom and clips overflow.
    ///
    /// `0` pins to the bottom, so a log following a growing tail needs no
    /// knowledge of its content height; increasing `scrollback` walks upward.
    /// Clamped during positioning to `[0, content - viewport]`.
    pub const fn scroll_y_from_end(mut self, scrollback: f32) -> Self {
        self.scroll_offset.y = scrollback;
        self.scroll_anchor = ScrollAnchor::End;
        self.overflow = ChildOverflow::Clipped;
        self
    }

    /// Scrolls children horizontally by `offset` logical px and clips overflow.
    ///
    /// The offset is clamped during positioning to `[0, content - viewport]`;
    /// pass `f32::MAX` to pin to the right edge.
    pub const fn scroll_x(mut self, offset: f32) -> Self {
        self.scroll_offset.x = offset;
        self.overflow = ChildOverflow::Clipped;
        self
    }

    /// Sets a PBR material override for this element.
    ///
    /// Controls surface properties (roughness, metallic, reflectance, etc.)
    /// for backgrounds and borders on this element. If the element also has
    /// a `.background()` color, that color overrides the material's `base_color`.
    pub fn material(mut self, material: StandardMaterial) -> Self {
        self.material = Some(Box::new(material));
        self
    }

    /// Marks this element as an editable IME field.
    ///
    /// The `field_id` is panel-local semantic identity used for hit testing,
    /// anchoring, and commit routing.
    pub fn editable_field(
        mut self,
        field_id: impl Into<PanelFieldId>,
        field_spec: ImeEditableFieldSpec,
    ) -> Self {
        self.editable = Some(ImePanelField::new(field_id, field_spec));
        self
    }

    /// Converts this declaration into an [`Element`] with the given content.
    fn into_element(self, content: ElementContent) -> Element {
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
            corner_radius: self.corner_radius,
            overflow: self.overflow,
            scroll_offset: self.scroll_offset,
            scroll_anchor: self.scroll_anchor,
            material: self.material,
            editable: self.editable,
            content,
        }
    }
}

/// Builds a [`LayoutTree`] using a closure-based nesting API.
pub struct LayoutBuilder {
    tree:         LayoutTree,
    /// Stack of parent indices for nesting.
    parent_stack: Vec<usize>,
    /// Per-build counter that mints [`PanelFieldId::Auto`] ids for unnamed text
    /// runs in build order. It starts at `0` for every builder, so auto ids are
    /// stable only within one build (`set_tree` rebuilds restart it) and never
    /// persisted or compared across panels — the positional identity an unnamed
    /// run keeps from the old `(element_idx, command_index)` reuse key.
    next_auto_id: u32,
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
            width: Sizing::Fixed(width.into()),
            height: Sizing::Fixed(height.into()),
            ..Element::default()
        });
        tree.set_root(root);

        Self {
            tree,
            parent_stack: vec![root],
            next_auto_id: 0,
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
            width: Sizing::Fixed(width.into()),
            height: Sizing::Fixed(height.into()),
            ..Element::default()
        });
        tree.set_root(root);

        Self {
            tree,
            parent_stack: vec![root],
            next_auto_id: 0,
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
            next_auto_id: 0,
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
    ///
    /// The run is given a builder-minted [`PanelFieldId::Auto`] id, so it renders
    /// but is not addressable at runtime. Use [`Self::text_id`] when a run needs
    /// to be looked up or retexted later.
    pub fn text(&mut self, text: impl Into<String>, config: TextStyle) -> &mut Self {
        let id = self.take_auto_id();
        self.add_text(id, text, config)
    }

    /// Adds a text leaf with an author-assigned id, so it can be addressed at
    /// runtime via [`text_child`](crate::DiegeticPanel::text_child).
    ///
    /// The id is passed by value (mirroring
    /// [`editable_field`](El::editable_field)), so a caller binds it once and
    /// reuses the same value at the lookup site:
    ///
    /// ```ignore
    /// let id = PanelFieldId::named("title");
    /// builder.text_id(id.clone(), "Hello", TextStyle::new(16.0));
    /// // …later…
    /// let entity = panel.text_child(&id);
    /// ```
    ///
    /// Text-run ids share one panel-local namespace with editable-field ids; a
    /// duplicate author-assigned id is rejected by `DiegeticPanelBuilder::build`.
    pub fn text_id(
        &mut self,
        id: impl Into<PanelFieldId>,
        text: impl Into<String>,
        config: TextStyle,
    ) -> &mut Self {
        self.add_text(id.into(), text, config)
    }

    fn add_text(
        &mut self,
        id: PanelFieldId,
        text: impl Into<String>,
        config: TextStyle,
    ) -> &mut Self {
        let parent = self.current_parent();
        self.tree.add_child(
            parent,
            Element {
                content: ElementContent::Text {
                    id,
                    text: text.into(),
                    config,
                },
                ..Element::default()
            },
        );
        self
    }

    /// Mints the next build-order [`PanelFieldId::Auto`] id for an unnamed run.
    const fn take_auto_id(&mut self) -> PanelFieldId {
        let id = PanelFieldId::auto(self.next_auto_id);
        self.next_auto_id += 1;
        id
    }

    /// Adds an image leaf as a child of the current parent.
    ///
    /// Image elements are leaves — they cannot have children. The element's
    /// [`Sizing`] rules control the rendered dimensions. Use
    /// [`Sizing::GROW`] to fill the parent or [`Sizing::fixed`] for an
    /// explicit size.
    ///
    /// The `tint` color is multiplied against the texture sample
    /// ([`Color::WHITE`] = no tint).
    pub fn image(&mut self, el: El, handle: Handle<Image>, tint: Color) -> &mut Self {
        let parent = self.current_parent();
        self.tree.add_child(
            parent,
            el.into_element(ElementContent::Image { handle, tint }),
        );
        self
    }

    /// Finishes building and returns the layout tree.
    #[must_use]
    pub fn build(self) -> LayoutTree { self.tree }

    /// Returns the current parent index.
    fn current_parent(&self) -> usize { self.parent_stack.last().copied().unwrap_or(0) }
}

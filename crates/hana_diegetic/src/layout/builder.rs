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
//!     .with(El::column().width(Sizing::GROW).height(Sizing::GROW).padding(Padding::all(8.0))
//!           .background(Color::srgb_u8(180, 96, 122)),
//!         |b| {
//!             b.text(("STATUS", TextStyle::new(7.0)));
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
use super::ChildDivider;
use super::CornerRadius;
use super::Dimension;
use super::DrawZIndex;
use super::Padding;
use super::PanelDraw;
use super::ShadowCasting;
use super::Sizing;
use super::TextStyle;
use super::TextWrap;
use super::child_layout::ChildLayout;
use super::element::ChildOverflow;
use super::element::Element;
use super::element::ElementContent;
use super::element::LayoutTree;
use super::element::PrecomposeMode;
use super::element::ScrollAnchor;
use crate::DimensionMatch;
use crate::ImeEditableFieldSpec;
use crate::ImePanelField;
use crate::PanelElementId;
use crate::cascade::Cascade;
use crate::render::AntiAlias;
use crate::render::HairlineFade;
use crate::widgets::Button;
use crate::widgets::Slider;
use crate::widgets::VisualSlotId;
use crate::widgets::WidgetInteractivity;
use crate::widgets::WidgetSpec;

/// Shorthand element declaration for the builder API.
///
/// This is a temporary configuration object that gets converted into an `Element`
/// when added to the tree.
#[must_use]
#[derive(Clone, Debug)]
pub struct El<L = Row> {
    common:       CommonEl,
    child_layout: L,
}

/// Text sizing and wrapping policy for a layout text leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextSizing {
    /// Measure the visible text naturally, optionally wrapping it.
    Natural {
        /// Wrapping policy for the visible text.
        wrap: TextWrap,
    },
    /// Measure this surrogate string while rendering the visible text as one line.
    MeasureAs {
        /// Surrogate string used for measurement.
        text: String,
    },
    /// Reserve the measured width of this surrogate string, then wrap visible text to it.
    WrapAtMeasure {
        /// Surrogate string whose measured width becomes the wrap width.
        text: String,
    },
}

impl Default for TextSizing {
    fn default() -> Self {
        Self::Natural {
            wrap: TextWrap::Words,
        }
    }
}

impl TextSizing {
    /// Creates natural text sizing with the requested wrapping mode.
    #[must_use]
    pub const fn wrap(wrap: TextWrap) -> Self { Self::Natural { wrap } }

    /// Creates sizing that measures a surrogate string instead of the visible text.
    #[must_use]
    pub fn measure_as(text: impl Into<String>) -> Self { Self::MeasureAs { text: text.into() } }

    /// Creates sizing that wraps visible text at the surrogate string's measured width.
    #[must_use]
    pub fn wrap_at_measure(text: impl Into<String>) -> Self {
        Self::WrapAtMeasure { text: text.into() }
    }

    pub(crate) fn measure_text<'a>(&'a self, visible_text: &'a str) -> &'a str {
        match self {
            Self::Natural { .. } => visible_text,
            Self::MeasureAs { text } | Self::WrapAtMeasure { text } => text,
        }
    }

    pub(crate) const fn visible_text_affects_layout(&self) -> bool {
        match self {
            Self::Natural { .. } | Self::WrapAtMeasure { .. } => true,
            Self::MeasureAs { .. } => false,
        }
    }
}

/// Text leaf declaration for [`LayoutBuilder::text`].
#[must_use]
#[derive(Clone, Debug)]
pub struct Text {
    layout:  CommonEl,
    content: String,
    style:   TextStyle,
    sizing:  TextSizing,
}

impl Text {
    /// Creates a text declaration with visible text and style.
    pub fn new(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            layout: CommonEl::default(),
            content: text.into(),
            style,
            sizing: TextSizing::default(),
        }
    }

    /// Assigns a panel-local id so this run can be addressed at runtime.
    pub fn id(mut self, id: impl Into<PanelElementId>) -> Self {
        self.layout.id = Some(id.into());
        self
    }

    /// Renders this text leaf into an LDR image, then draws that image in the
    /// parent panel.
    ///
    /// Use this when text should keep SDR edge behavior under an HDR scene
    /// camera, while surrounding panel backgrounds and borders stay on the
    /// normal analytic path.
    pub const fn precompose_ldr(mut self) -> Self {
        self.layout.precompose = PrecomposeMode::Ldr;
        self
    }

    /// Sets the element layout declaration for this text leaf.
    pub fn layout<L>(mut self, layout: El<L>) -> Self
    where
        L: ChildLayoutState,
    {
        let current_id = self.layout.id.take();
        let El { common, .. } = layout;
        self.layout = common;
        if self.layout.id.is_none() {
            self.layout.id = current_id;
        }
        self
    }

    /// Sets the complete sizing policy for this text leaf.
    pub fn sizing(mut self, sizing: TextSizing) -> Self {
        self.sizing = sizing;
        self
    }

    /// Measures the visible text naturally with the requested wrapping mode.
    pub fn wrap(mut self, wrap: TextWrap) -> Self {
        self.sizing = TextSizing::wrap(wrap);
        self
    }

    /// Measures this text leaf as though it contained the surrogate string.
    pub fn measure_as(mut self, text: impl Into<String>) -> Self {
        self.sizing = TextSizing::measure_as(text);
        self
    }

    /// Reserves the surrogate string's width and wraps visible text to that width.
    pub fn wrap_at_measure(mut self, text: impl Into<String>) -> Self {
        self.sizing = TextSizing::wrap_at_measure(text);
        self
    }

    fn into_element(self) -> Element {
        let Self {
            layout,
            content,
            style,
            sizing,
        } = self;
        text_leaf_element(
            layout,
            ElementContent::Text {
                text: content,
                config: style,
                sizing,
            },
        )
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self { Self::new(text, TextStyle::default()) }
}

impl From<&String> for Text {
    fn from(text: &String) -> Self { Self::new(text, TextStyle::default()) }
}

impl From<String> for Text {
    fn from(text: String) -> Self { Self::new(text, TextStyle::default()) }
}

impl<T> From<(T, TextStyle)> for Text
where
    T: Into<String>,
{
    fn from((text, style): (T, TextStyle)) -> Self { Self::new(text, style) }
}

/// Public row child-layout state for [`El`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Row {
    gap:     Dimension,
    divider: Option<ChildDivider>,
}

/// Public column child-layout state for [`El`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Column {
    gap:     Dimension,
    divider: Option<ChildDivider>,
}

/// Public overlay child-layout state for [`El`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Overlay;

/// Public marker trait for child-layout states accepted by [`LayoutBuilder`].
pub trait ChildLayoutState: private::Sealed {}

impl ChildLayoutState for Row {}

impl ChildLayoutState for Column {}

impl ChildLayoutState for Overlay {}

#[derive(Clone, Debug)]
struct CommonEl {
    id:              Option<PanelElementId>,
    width:           Sizing,
    height:          Sizing,
    padding:         Padding,
    align_x:         AlignX,
    align_y:         AlignY,
    background:      Option<Color>,
    border:          Option<Border>,
    corner_radius:   CornerRadius,
    overflow:        ChildOverflow,
    scroll_offset:   Vec2,
    scroll_anchor_x: ScrollAnchor,
    scroll_anchor_y: ScrollAnchor,
    material:        Cascade<Handle<StandardMaterial>>,
    interactivity:   Cascade<WidgetInteractivity>,
    editable:        Option<ImePanelField>,
    widget:          Option<WidgetSpec>,
    visual_slot:     Option<VisualSlotId>,
    draw:            Option<PanelDraw>,
    z_index:         DrawZIndex,
    anti_alias:      Cascade<AntiAlias>,
    hairline_fade:   Cascade<HairlineFade>,
    shadow_casting:  Cascade<ShadowCasting>,
    precompose:      PrecomposeMode,
}

impl Default for CommonEl {
    fn default() -> Self {
        Self {
            id:              None,
            width:           Sizing::FIT,
            height:          Sizing::FIT,
            padding:         Padding::default(),
            align_x:         AlignX::default(),
            align_y:         AlignY::default(),
            background:      None,
            border:          None,
            corner_radius:   CornerRadius::ZERO,
            overflow:        ChildOverflow::Visible,
            scroll_offset:   Vec2::ZERO,
            scroll_anchor_x: ScrollAnchor::Start,
            scroll_anchor_y: ScrollAnchor::Start,
            material:        Cascade::Inherit,
            interactivity:   Cascade::Inherit,
            editable:        None,
            widget:          None,
            visual_slot:     None,
            draw:            None,
            z_index:         DrawZIndex::default(),
            anti_alias:      Cascade::Inherit,
            hairline_fade:   Cascade::Inherit,
            shadow_casting:  Cascade::Inherit,
            precompose:      PrecomposeMode::Direct,
        }
    }
}

fn text_leaf_element(common: CommonEl, content: ElementContent) -> Element {
    Element {
        id: common.id,
        width: common.width,
        height: common.height,
        padding: common.padding,
        child_layout: ChildLayout::default(),
        background: common.background,
        border: common.border,
        corner_radius: common.corner_radius,
        overflow: common.overflow,
        scroll_offset: common.scroll_offset,
        scroll_anchor_x: common.scroll_anchor_x,
        scroll_anchor_y: common.scroll_anchor_y,
        material: common.material,
        interactivity: common.interactivity,
        editable: common.editable,
        widget: common.widget,
        visual_slot: common.visual_slot,
        draw: common.draw,
        z_index: common.z_index,
        anti_alias: common.anti_alias,
        hairline_fade: common.hairline_fade,
        shadow_casting: common.shadow_casting,
        precompose: common.precompose,
        content,
    }
}

impl<L> Default for El<L>
where
    L: Default,
{
    fn default() -> Self {
        Self {
            common:       CommonEl::default(),
            child_layout: L::default(),
        }
    }
}

impl El<Row> {
    /// Creates a new row element declaration with default settings.
    pub fn new() -> Self { Self::row() }

    /// Creates a left-to-right row element declaration.
    pub fn row() -> Self { Self::default() }

    /// Sets the gap between adjacent row children.
    pub fn gap(mut self, gap: impl Into<Dimension>) -> Self {
        self.child_layout.gap = gap.into();
        self
    }

    /// Sets a separator between adjacent row children.
    pub const fn child_divider(mut self, divider: ChildDivider) -> Self {
        self.child_layout.divider = Some(divider);
        self
    }
}

impl El<Column> {
    /// Creates a top-to-bottom column element declaration.
    pub fn column() -> Self { Self::default() }

    /// Sets the gap between adjacent column children.
    pub fn gap(mut self, gap: impl Into<Dimension>) -> Self {
        self.child_layout.gap = gap.into();
        self
    }

    /// Sets a separator between adjacent column children.
    pub const fn child_divider(mut self, divider: ChildDivider) -> Self {
        self.child_layout.divider = Some(divider);
        self
    }
}

impl El<Overlay> {
    /// Creates an overlay element declaration.
    pub fn overlay() -> Self { Self::default() }
}

impl<L> El<L> {
    /// Sets the width sizing rule.
    ///
    /// Can be overridden by a subsequent `.size()` call (last wins).
    ///
    /// Common patterns: [`Sizing::GROW`], [`Sizing::FIT`], [`Sizing::fixed`], [`Sizing::percent`].
    pub const fn width(mut self, sizing: Sizing) -> Self {
        self.common.width = sizing;
        self
    }

    /// Assigns a panel-local id to this element.
    ///
    /// Named element ids share one namespace across the panel tree, including
    /// text elements and editable fields. Use ids for persistent element
    /// identity such as text lookup, hit targets, and precompose cache keys.
    pub fn id(mut self, id: impl Into<PanelElementId>) -> Self {
        self.common.id = Some(id.into());
        self
    }

    /// Sets the height sizing rule.
    ///
    /// Can be overridden by a subsequent `.size()` call (last wins).
    ///
    /// Common patterns: [`Sizing::GROW`], [`Sizing::FIT`], [`Sizing::fixed`], [`Sizing::percent`].
    pub const fn height(mut self, sizing: Sizing) -> Self {
        self.common.height = sizing;
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
        self.common.padding = padding;
        self
    }

    /// Sets both horizontal and vertical child alignment.
    pub const fn alignment(mut self, x: AlignX, y: AlignY) -> Self {
        self.common.align_x = x;
        self.common.align_y = y;
        self
    }

    /// Sets horizontal child alignment.
    pub const fn align_x(mut self, align: AlignX) -> Self {
        self.common.align_x = align;
        self
    }

    /// Sets vertical child alignment.
    pub const fn align_y(mut self, align: AlignY) -> Self {
        self.common.align_y = align;
        self
    }

    /// Sets a background color.
    pub const fn background(mut self, color: Color) -> Self {
        self.common.background = Some(color);
        self
    }

    /// Sets a border.
    pub const fn border(mut self, border: Border) -> Self {
        self.common.border = Some(border);
        self
    }

    /// Sets the corner radius for rounded backgrounds and borders.
    ///
    /// Accepts `CornerRadius::all(8.0)`, `CornerRadius::new(tl, tr, br, bl)`,
    /// or a bare `f32` for uniform radius on all corners.
    pub fn corner_radius(mut self, radius: impl Into<CornerRadius>) -> Self {
        self.common.corner_radius = radius.into();
        self
    }

    /// Sets overflow to `Clipped`; default is `Visible`.
    pub const fn clip(mut self) -> Self {
        self.common.overflow = ChildOverflow::Clipped;
        self
    }

    /// Scrolls children vertically by `offset` logical px from the top and clips
    /// overflow.
    ///
    /// The offset is clamped during positioning to `[0, content - viewport]`;
    /// pass `f32::MAX` to pin to the bottom.
    pub const fn scroll_y(mut self, offset: f32) -> Self {
        self.common.scroll_offset.y = offset;
        self.common.scroll_anchor_y = ScrollAnchor::Start;
        self.common.overflow = ChildOverflow::Clipped;
        self
    }

    /// Scrolls children vertically by `scrollback` logical px measured from the
    /// bottom and clips overflow.
    ///
    /// `0` pins to the bottom, so a log following a growing tail needs no
    /// knowledge of its content height; increasing `scrollback` walks upward.
    /// Clamped during positioning to `[0, content - viewport]`.
    pub const fn scroll_y_from_end(mut self, scrollback: f32) -> Self {
        self.common.scroll_offset.y = scrollback;
        self.common.scroll_anchor_y = ScrollAnchor::End;
        self.common.overflow = ChildOverflow::Clipped;
        self
    }

    /// Scrolls children horizontally by `offset` logical px and clips overflow.
    ///
    /// The offset is clamped during positioning to `[0, content - viewport]`;
    /// pass `f32::MAX` to pin to the right edge.
    pub const fn scroll_x(mut self, offset: f32) -> Self {
        self.common.scroll_offset.x = offset;
        self.common.scroll_anchor_x = ScrollAnchor::Start;
        self.common.overflow = ChildOverflow::Clipped;
        self
    }

    /// Sets a PBR material handle override for this element.
    ///
    /// Controls surface properties (roughness, metallic, reflectance, etc.)
    /// for backgrounds, borders, and element-owned panel-shape primitives on
    /// this element. For panel-shape primitives, this is the element-local
    /// source above the panel `.shape_material(...)` handle and the global
    /// `ShapeMaterial` cascade default. For backgrounds and borders, it is
    /// above the panel `.material(...)` handle and the global `SdfMaterial`
    /// cascade default. If the element also has a `.background()` color, that
    /// color overrides the material's `base_color`. Create the material asset
    /// once through `Assets<StandardMaterial>`; do not create assets per frame.
    pub fn material(mut self, material: Handle<StandardMaterial>) -> Self {
        self.common.material = Cascade::Override(material);
        self
    }

    /// Marks this element as an editable IME field.
    ///
    /// The `field_id` is panel-local semantic identity used for hit testing,
    /// anchoring, and commit routing.
    pub fn editable_field(
        mut self,
        field_id: impl Into<PanelElementId>,
        field_spec: ImeEditableFieldSpec,
    ) -> Self {
        self.common.editable = Some(ImePanelField::new(field_id, field_spec));
        self
    }

    /// Marks this element as a button with panel-local semantic identity `id`.
    ///
    /// The id and button declaration are assigned together so a widget cannot
    /// be authored without its identity. A [`Button::on_click`] callback on
    /// the declaration stays a cloneable template here; the builder never
    /// touches a `World`, and reify registers the callback when the widget
    /// entity exists. The element also receives the private root visual slot
    /// that the button's state presentation builders patch at runtime.
    pub fn button(mut self, id: impl Into<PanelElementId>, button: Button) -> Self {
        self.common.id = Some(id.into());
        self.common.widget = Some(WidgetSpec::Button(button));
        self.visual_slot(VisualSlotId::BUTTON_ROOT)
    }

    /// Marks this element as a slider with panel-local semantic identity `id`.
    ///
    /// The id and slider declaration are assigned together so a widget cannot
    /// be authored without its identity.
    pub fn slider(mut self, id: impl Into<PanelElementId>, slider: Slider) -> Self {
        self.common.id = Some(id.into());
        self.common.widget = Some(WidgetSpec::Slider(slider));
        self
    }

    /// Authors widget interactivity for this element and its widget descendants.
    ///
    /// A descendant can replace this value with its own override.
    pub const fn widget_interactivity(mut self, value: WidgetInteractivity) -> Self {
        self.common.interactivity = Cascade::Override(value);
        self
    }

    /// Attaches a stable private visual-slot id to this element's retained
    /// render records.
    ///
    /// Crate widget authoring assigns slot ids to ordinary primitives inside
    /// a widget subtree; widget state then patches those retained records
    /// through crate-private overrides without regenerating the tree.
    /// [`Self::button`] authors [`VisualSlotId::BUTTON_ROOT`] through this
    /// hook.
    pub(crate) const fn visual_slot(mut self, slot: VisualSlotId) -> Self {
        self.common.visual_slot = Some(slot);
        self
    }

    /// Sets paint-only draw primitives owned by this element.
    ///
    /// `PanelDraw` does not affect layout measurement. It is stored for later
    /// render-command resolution after the element has computed bounds.
    pub fn draw(mut self, panel_draw: PanelDraw) -> Self {
        self.common.draw = Some(panel_draw);
        self
    }

    /// Sets the authored `z_index` for this element's render commands.
    pub fn z_index(mut self, z_index: impl Into<DrawZIndex>) -> Self {
        self.common.z_index = z_index.into();
        self
    }

    /// Overrides the anti-alias mode for this element's analytic line marks.
    ///
    /// Without an override the element inherits the panel entity's
    /// cascade-resolved [`AntiAlias`] (panel override else the global
    /// resource). Per-record data — an override never splits a batch.
    pub const fn anti_alias(mut self, mode: AntiAlias) -> Self {
        self.common.anti_alias = Cascade::Override(mode);
        self
    }

    /// Overrides the hairline fade policy for this element's analytic line
    /// marks.
    ///
    /// Without an override the element inherits the panel entity's
    /// cascade-resolved [`HairlineFade`] (panel override else
    /// [`HairlineWidth::fade`](crate::HairlineWidth)). Per-record data — an
    /// override never splits a batch.
    pub const fn hairline_fade(mut self, fade: HairlineFade) -> Self {
        self.common.hairline_fade = Cascade::Override(fade);
        self
    }

    /// Overrides shadow casting for this element and its render commands.
    pub const fn shadow_casting(mut self, shadow_casting: ShadowCasting) -> Self {
        self.common.shadow_casting = Cascade::Override(shadow_casting);
        self
    }

    /// Renders this element's subtree into an LDR image, then draws that image
    /// in the parent panel.
    ///
    /// This is useful when a panel subtree should keep the SDR text edge
    /// behavior even while the main scene camera renders HDR. The flattened
    /// result behaves as one alpha-blended image in the parent panel; it does
    /// not preserve per-descendant depth or interaction.
    pub const fn precompose_ldr(mut self) -> Self {
        self.common.precompose = PrecomposeMode::Ldr;
        self
    }

    /// Converts this declaration into an [`Element`] with the given content.
    fn into_element(self, content: ElementContent) -> Element
    where
        L: ChildLayoutState,
    {
        let Self {
            common,
            child_layout,
        } = self;
        let child_layout = if matches!(
            content,
            ElementContent::Text { .. } | ElementContent::Image { .. }
        ) {
            ChildLayout::default()
        } else {
            private::Sealed::into_child_layout(child_layout, common.align_x, common.align_y)
        };
        Element {
            id: common.id,
            width: common.width,
            height: common.height,
            padding: common.padding,
            child_layout,
            background: common.background,
            border: common.border,
            corner_radius: common.corner_radius,
            overflow: common.overflow,
            scroll_offset: common.scroll_offset,
            scroll_anchor_x: common.scroll_anchor_x,
            scroll_anchor_y: common.scroll_anchor_y,
            material: common.material,
            interactivity: common.interactivity,
            editable: common.editable,
            widget: common.widget,
            visual_slot: common.visual_slot,
            draw: common.draw,
            z_index: common.z_index,
            anti_alias: common.anti_alias,
            hairline_fade: common.hairline_fade,
            shadow_casting: common.shadow_casting,
            precompose: common.precompose,
            content,
        }
    }
}

mod private {
    use super::AlignX;
    use super::AlignY;
    use super::Column;
    use super::Overlay;
    use super::Row;
    use crate::layout::child_layout::ChildLayout;

    pub trait Sealed {
        fn into_child_layout(self, align_x: AlignX, align_y: AlignY) -> ChildLayout;
    }

    impl Sealed for Row {
        fn into_child_layout(self, align_x: AlignX, align_y: AlignY) -> ChildLayout {
            ChildLayout::Row {
                gap: self.gap,
                align_x,
                align_y,
                divider: self.divider,
            }
        }
    }

    impl Sealed for Column {
        fn into_child_layout(self, align_x: AlignX, align_y: AlignY) -> ChildLayout {
            ChildLayout::Column {
                gap: self.gap,
                align_x,
                align_y,
                divider: self.divider,
            }
        }
    }

    impl Sealed for Overlay {
        fn into_child_layout(self, align_x: AlignX, align_y: AlignY) -> ChildLayout {
            ChildLayout::Overlay { align_x, align_y }
        }
    }
}

/// Builds a [`LayoutTree`] using a closure-based nesting API.
pub struct LayoutBuilder {
    tree:         LayoutTree,
    /// Stack of parent indices for nesting.
    parent_stack: Vec<usize>,
    /// Per-build counter that mints [`PanelElementId::Auto`] ids for unnamed text
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
    pub fn with_root<L>(el: El<L>) -> Self
    where
        L: ChildLayoutState,
    {
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
    /// The passed `El` is converted into an `Element` and inserted as a
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
    pub fn with<L>(&mut self, el: El<L>, children: impl FnOnce(&mut Self)) -> &mut Self
    where
        L: ChildLayoutState,
    {
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
    /// The run is given a builder-minted [`PanelElementId::Auto`] id unless the
    /// declaration supplies [`Text::id`].
    pub fn text(&mut self, text: impl Into<Text>) -> &mut Self {
        let parent = self.current_parent();
        let mut text = text.into();
        let id = text
            .layout
            .id
            .clone()
            .unwrap_or_else(|| self.take_auto_id());
        text.layout.id = Some(id);
        self.tree.add_child(parent, text.into_element());
        self
    }

    /// Mints the next build-order [`PanelElementId::Auto`] id for an unnamed run.
    const fn take_auto_id(&mut self) -> PanelElementId {
        let id = PanelElementId::auto(self.next_auto_id);
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
    pub fn image<L>(&mut self, el: El<L>, handle: Handle<Image>, tint: Color) -> &mut Self
    where
        L: ChildLayoutState,
    {
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

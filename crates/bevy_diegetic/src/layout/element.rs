//! Element tree representation for layout computation.
//!
//! [`Element`] is the data struct stored in the arena-based [`LayoutTree`]. It holds every
//! layout property (sizing, padding, direction, background, border, etc.) plus an
//! [`ElementContent`] that determines whether the node is a container, a text leaf, or empty.
//!
//! Users rarely construct `Element` directly. Instead, the [`El`](super::builder::El) builder
//! provides a fluent API that converts into an `Element` via `into_element()`. Think of `El`
//! as the ergonomic front door and `Element` as the canonical storage format.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

use bevy::asset::Handle;
use bevy::color::Color;
use bevy::image::Image;
use bevy::math::Vec2;
use bevy::pbr::StandardMaterial;
use smallvec::SmallVec;

use super::Border;
use super::ChildDivider;
use super::CornerRadius;
use super::Dimension;
use super::DrawZIndex;
use super::Padding;
use super::PanelDraw;
use super::Sizing;
use super::TextSizing;
use super::TextStyle;
use super::Unit;
use super::child_layout::ChildLayout;
use super::constants::INLINE_CHILDREN;
use crate::ImePanelField;
use crate::PanelFieldId;
use crate::render::AntiAlias;
use crate::render::HairlineFade;

/// Result of replacing the display text for a panel field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldDisplayTextUpdate {
    /// Exactly one field matched and its display text was replaced.
    Updated,
    /// No editable field had the requested id.
    MissingField,
    /// More than one editable field had the requested id.
    DuplicateField,
    /// The field had no text descendant to update.
    MissingText,
}

/// Whether overflowing children are clipped to the parent's content box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ChildOverflow {
    #[default]
    Visible,
    Clipped,
}

/// Which edge [`Element::scroll_offset`] measures from. `Start` is an absolute
/// offset from the top/left (clamped to `[0, max]`); `End` is a distance from
/// the bottom/right, so `0` pins to the end and following a growing tail needs
/// no knowledge of the content size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ScrollAnchor {
    #[default]
    Start,
    End,
}

/// A single element in the layout tree.
///
/// Elements are either containers (with children) or text leaves. The tree
/// is built via [`LayoutTree`] and then sized/positioned by the layout engine.
#[derive(Clone, Debug)]
pub(super) struct Element {
    /// Width sizing rule.
    pub(super) width:           Sizing,
    /// Height sizing rule.
    pub(super) height:          Sizing,
    /// Interior padding.
    pub(super) padding:         Padding,
    /// Child layout mode, spacing, and alignment.
    pub(super) child_layout:    ChildLayout,
    /// Optional background color.
    pub(super) background:      Option<Color>,
    /// Optional border.
    pub(super) border:          Option<Border>,
    /// Corner radius for rounded backgrounds and borders.
    pub(super) corner_radius:   CornerRadius,
    /// How this element handles overflowing children (`Visible` or `Clipped`).
    pub(super) overflow:        ChildOverflow,
    /// Scroll offset (logical px) subtracted from child positions when this
    /// element clips. Clamped during positioning to `[0, content - viewport]`
    /// per axis. Interpreted relative to each axis' scroll anchor.
    pub(super) scroll_offset:   Vec2,
    /// Which horizontal edge `scroll_offset.x` measures from.
    pub(super) scroll_anchor_x: ScrollAnchor,
    /// Which vertical edge `scroll_offset.y` measures from.
    pub(super) scroll_anchor_y: ScrollAnchor,
    /// Optional PBR source-material handle for this element's surfaces.
    /// When present, render systems use this as the element-local override over
    /// the panel material handle and global material cascade defaults.
    /// `base_color` is overridden by layout or primitive color when both are set.
    pub(super) material:        Option<Handle<StandardMaterial>>,
    /// Optional editable field contract.
    pub(super) editable:        Option<ImePanelField>,
    /// Optional paint-only draw data.
    pub(super) draw:            Option<PanelDraw>,
    /// `DrawZIndex` stamped onto this element's render commands.
    pub(super) z_index:         DrawZIndex,
    /// Optional anti-alias override for this element's analytic line marks.
    /// `None` inherits the panel entity's cascade-resolved mode.
    pub(super) anti_alias:      Option<AntiAlias>,
    /// Optional hairline fade override for this element's analytic line marks.
    /// `None` inherits the panel entity's cascade-resolved policy.
    pub(super) hairline_fade:   Option<HairlineFade>,
    /// Content of this element.
    pub(super) content:         ElementContent,
}

/// What an element contains.
#[derive(Clone, Debug)]
pub(super) enum ElementContent {
    /// Container with child element indices.
    Children(SmallVec<[usize; INLINE_CHILDREN]>),
    /// Text leaf.
    Text {
        /// Panel-local id for this run — an author-assigned
        /// [`PanelFieldId::Named`] (addressable at runtime via
        /// [`text_child`](crate::DiegeticPanel::text_child)) or a builder-minted
        /// [`PanelFieldId::Auto`] for an unnamed run. Doubles as the reconcile
        /// reuse identity, so a named run survives a sibling reorder.
        id:     PanelFieldId,
        /// The text string.
        text:   String,
        /// Text configuration.
        config: TextStyle,
        /// Sizing and wrapping policy.
        sizing: TextSizing,
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
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LayoutTreeChange {
    /// Trees are exactly identical for the fields this classifier inspects.
    Identical       = 0,
    /// Trees differ only in fields that should not affect layout bounds.
    VisualOnly      = 1,
    /// Trees differ in structure, sizing, measurement, or placement fields.
    LayoutAffecting = 2,
}

impl LayoutTreeChange {
    pub(crate) fn combine(self, other: Self) -> Self { self.max(other) }
}

impl Default for Element {
    fn default() -> Self {
        Self {
            width:           Sizing::FIT,
            height:          Sizing::FIT,
            padding:         Padding::default(),
            child_layout:    ChildLayout::default(),
            background:      None,
            border:          None,
            corner_radius:   CornerRadius::ZERO,
            overflow:        ChildOverflow::Visible,
            scroll_offset:   Vec2::ZERO,
            scroll_anchor_x: ScrollAnchor::Start,
            scroll_anchor_y: ScrollAnchor::Start,
            material:        None,
            editable:        None,
            draw:            None,
            z_index:         DrawZIndex::default(),
            anti_alias:      None,
            hairline_fade:   None,
            content:         ElementContent::Empty,
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

    /// Hashes only structural facts needed to safely reuse computed geometry.
    #[must_use]
    pub(super) fn structure_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.root.hash(&mut hasher);
        self.elements.len().hash(&mut hasher);
        for element in &self.elements {
            match &element.content {
                ElementContent::Children(children) => {
                    0_u8.hash(&mut hasher);
                    children.hash(&mut hasher);
                },
                ElementContent::Text { .. } => {
                    1_u8.hash(&mut hasher);
                },
                ElementContent::Image { .. } => {
                    2_u8.hash(&mut hasher);
                },
                ElementContent::Empty => {
                    3_u8.hash(&mut hasher);
                },
            }
        }
        hasher.finish()
    }

    /// Returns the PBR material override for the element at `index`, if any.
    #[must_use]
    pub fn element_material(&self, index: usize) -> Option<&Handle<StandardMaterial>> {
        self.elements.get(index).and_then(|e| e.material.as_ref())
    }

    /// Returns the corner radius for the element at `index`.
    #[must_use]
    pub fn element_corner_radius(&self, index: usize) -> CornerRadius {
        self.elements
            .get(index)
            .map_or(CornerRadius::ZERO, |e| e.corner_radius)
    }

    /// Returns editable field metadata for the element at `index`, if any.
    #[must_use]
    pub(crate) fn editable_field(&self, index: usize) -> Option<&ImePanelField> {
        self.elements.get(index).and_then(|e| e.editable.as_ref())
    }

    /// Returns the paint-only draw data for the element at `index`, if any.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn element_draw(&self, index: usize) -> Option<&PanelDraw> {
        self.elements.get(index).and_then(|e| e.draw.as_ref())
    }

    /// Returns the anti-alias override for the element at `index`, if any.
    #[must_use]
    pub fn element_anti_alias(&self, index: usize) -> Option<AntiAlias> {
        self.elements.get(index).and_then(|e| e.anti_alias)
    }

    /// Returns the hairline fade override for the element at `index`, if any.
    #[must_use]
    pub fn element_hairline_fade(&self, index: usize) -> Option<HairlineFade> {
        self.elements.get(index).and_then(|e| e.hairline_fade)
    }

    /// Returns text content for the element at `index`, if any.
    #[must_use]
    pub(crate) fn element_text(&self, index: usize) -> Option<&str> {
        self.elements
            .get(index)
            .and_then(|element| match &element.content {
                ElementContent::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
    }

    /// Overwrites the cached run string at `index`, returning whether it
    /// changed. `TextContent` on the materialized child is the single source;
    /// this keeps the `El.text` layout cache (which the engine measures and
    /// word-wraps) current after an out-of-flow edit. Returns `false` (no write)
    /// when `index` is not a text leaf or the string already matches, so the
    /// caller can skip dirtying the panel.
    pub(crate) fn set_element_text(&mut self, index: usize, text: &str) -> bool {
        let Some(element) = self.elements.get_mut(index) else {
            return false;
        };
        let ElementContent::Text { text: existing, .. } = &mut element.content else {
            return false;
        };
        if existing == text {
            return false;
        }
        text.clone_into(existing);
        true
    }

    /// Returns the authored [`TextStyle`] of the text run at `index`, if that
    /// element is a text leaf. The tree config — not the run child's
    /// `for_shaping`-derived style — is authoritative for measurement, so a
    /// restyle reads it here, applies its edit, and writes back via
    /// [`set_element_style`](Self::set_element_style).
    #[must_use]
    pub(crate) fn element_style(&self, index: usize) -> Option<&TextStyle> {
        self.elements
            .get(index)
            .and_then(|element| match &element.content {
                ElementContent::Text { config, .. } => Some(config),
                _ => None,
            })
    }

    /// Overwrites the authored run style at `index`, returning whether it
    /// changed. The tree config is the single source the layout engine measures
    /// and reconcile re-derives the run from; writing it here is how a font /
    /// size restyle reaches both measurement and rendering. Returns `false` (no
    /// write) when `index` is not a text leaf or the style already matches, so
    /// the caller can skip dirtying the panel.
    pub(crate) fn set_element_style(&mut self, index: usize, style: TextStyle) -> bool {
        let Some(element) = self.elements.get_mut(index) else {
            return false;
        };
        let ElementContent::Text { config, .. } = &mut element.content else {
            return false;
        };
        if *config == style {
            return false;
        }
        *config = style;
        true
    }

    /// Returns the panel-local id of the text run at `index`, if that element is
    /// a text leaf. Reconcile reads this to key a child by its run id instead of
    /// the former positional `(element_idx, command_index)` pair.
    #[must_use]
    pub(crate) fn element_field_id(&self, index: usize) -> Option<&PanelFieldId> {
        self.elements
            .get(index)
            .and_then(|element| match &element.content {
                ElementContent::Text { id, .. } => Some(id),
                _ => None,
            })
    }

    /// Whether any text-run element in the tree carries `id`.
    ///
    /// The tree is the authoritative list of valid run ids at build time,
    /// independent of reconcile timing, so a lookup that misses the panel's
    /// `text_index` consults this to tell a genuine typo (`id` absent here) from a
    /// run not yet materialized into an entity (`id` present here, index just not
    /// rebuilt). See [`PanelText`](crate::PanelText).
    // The runtime caller is the `#[cfg(debug_assertions)]` typo-warn path in
    // `PanelTextReader::resolve`; test harnesses also compile this for coverage.
    #[cfg(any(debug_assertions, test))]
    #[must_use]
    pub(crate) fn contains_text_id(&self, id: &PanelFieldId) -> bool {
        (0..self.elements.len()).any(|index| self.element_field_id(index) == Some(id))
    }

    /// Returns the first author-assigned [`PanelFieldId::Named`] id that appears
    /// on more than one element, scanning text-run ids and editable-field ids
    /// together — they share one panel-local namespace. Auto ids are skipped
    /// (unforgeable and unique by construction), so this only flags a real
    /// author collision. `DiegeticPanelBuilder::build` calls this to reject a
    /// duplicate at build time.
    #[must_use]
    pub(crate) fn duplicate_named_field_id(&self) -> Option<&PanelFieldId> {
        let mut seen: Vec<&PanelFieldId> = Vec::new();
        for index in 0..self.elements.len() {
            let text_id = self.element_field_id(index);
            let field_id = self.editable_field(index).map(|field| &field.field_id);
            for id in [text_id, field_id].into_iter().flatten() {
                if id.is_named() {
                    if seen.contains(&id) {
                        return Some(id);
                    }
                    seen.push(id);
                }
            }
        }
        None
    }

    /// Returns the first text string owned by `index` or one of its descendants.
    #[must_use]
    pub(crate) fn field_display_text(&self, index: usize) -> Option<&str> {
        let mut stack = vec![index];
        while let Some(current) = stack.pop() {
            if let Some(text) = self.element_text(current) {
                return Some(text);
            }
            for &child in self.children_of(current).iter().rev() {
                stack.push(child);
            }
        }
        None
    }

    pub(crate) fn set_field_display_text(
        &mut self,
        field_id: &PanelFieldId,
        text: impl Into<String>,
    ) -> FieldDisplayTextUpdate {
        let matches: Vec<usize> = self
            .elements
            .iter()
            .enumerate()
            .filter_map(|(index, element)| {
                element
                    .editable
                    .as_ref()
                    .is_some_and(|field| field.field_id == *field_id)
                    .then_some(index)
            })
            .collect();

        let [field_index] = matches.as_slice() else {
            return if matches.is_empty() {
                FieldDisplayTextUpdate::MissingField
            } else {
                FieldDisplayTextUpdate::DuplicateField
            };
        };

        let Some(text_index) = self.first_text_descendant(*field_index) else {
            return FieldDisplayTextUpdate::MissingText;
        };
        if let ElementContent::Text {
            text: existing_text,
            ..
        } = &mut self.elements[text_index].content
        {
            *existing_text = text.into();
        }
        FieldDisplayTextUpdate::Updated
    }

    fn first_text_descendant(&self, index: usize) -> Option<usize> {
        let mut stack = vec![index];
        while let Some(current) = stack.pop() {
            if matches!(
                self.elements.get(current).map(|element| &element.content),
                Some(ElementContent::Text { .. })
            ) {
                return Some(current);
            }
            for &child in self.children_of(current).iter().rev() {
                stack.push(child);
            }
        }
        None
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
            element.child_layout = element.child_layout.to_points(layout_scale);
            if let Some(ref mut border) = element.border {
                *border = border.resolved(layout_scale);
            }
            element.corner_radius = element.corner_radius.resolved(layout_scale);
            if let Some(ref mut panel_draw) = element.draw {
                *panel_draw = panel_draw.scaled(layout_scale);
            }
            element.scroll_offset *= layout_scale;
            if let ElementContent::Text { ref mut config, .. } = element.content {
                // If this text element carries an explicit unit (e.g., from
                // `TextStyle::new(Mm(6.0))`), convert from that unit to
                // points directly. Otherwise use the panel-wide font_scale.
                let scale = config.unit().map_or(font_scale, Unit::to_points);
                *config = config.scaled(scale);
            }
        }
        tree
    }

    /// Returns a copy of this tree authored in screen-pixel source values.
    ///
    /// The first pass resolves the existing authored dimensions into layout
    /// points. The second pass converts those resolved point values into pixel
    /// source values so a screen-space panel with `Unit::Pixels` renders the
    /// same relative content size as the original world panel projection.
    #[must_use]
    pub(crate) fn screen_source_scaled(
        &self,
        layout_to_points: f32,
        font_to_points: f32,
        points_to_pixels: f32,
    ) -> Self {
        let mut tree = self.scaled(layout_to_points, font_to_points);
        for element in &mut tree.elements {
            element.width = element.width.resolved(points_to_pixels);
            element.height = element.height.resolved(points_to_pixels);
            element.padding = element.padding.resolved(points_to_pixels);
            element.child_layout = element.child_layout.to_points(points_to_pixels);
            if let Some(ref mut border) = element.border {
                *border = border.resolved(points_to_pixels);
            }
            element.corner_radius = element.corner_radius.resolved(points_to_pixels);
            if let Some(ref mut panel_draw) = element.draw {
                *panel_draw = panel_draw.scaled(points_to_pixels);
            }
            element.scroll_offset *= points_to_pixels;
            if let ElementContent::Text { ref mut config, .. } = element.content {
                *config = config.scaled_as_unit(points_to_pixels, Unit::Pixels);
            }
        }
        tree
    }
}

fn classify_element_change(element: &Element, next: &Element) -> LayoutTreeChange {
    let Element {
        width,
        height,
        padding,
        child_layout,
        background,
        border,
        corner_radius,
        overflow,
        scroll_offset,
        scroll_anchor_x,
        scroll_anchor_y,
        material,
        editable,
        draw,
        z_index,
        anti_alias,
        hairline_fade,
        content,
    } = element;
    let Element {
        width: n_width,
        height: n_height,
        padding: n_padding,
        child_layout: n_child_layout,
        background: n_background,
        border: n_border,
        corner_radius: n_corner_radius,
        overflow: n_overflow,
        scroll_offset: n_scroll_offset,
        scroll_anchor_x: n_scroll_anchor_x,
        scroll_anchor_y: n_scroll_anchor_y,
        material: n_material,
        editable: n_editable,
        draw: n_draw,
        z_index: n_z_index,
        anti_alias: n_anti_alias,
        hairline_fade: n_hairline_fade,
        content: n_content,
    } = next;

    let child_layout_change = classify_child_layout_change(child_layout, n_child_layout);
    if width != n_width
        || height != n_height
        || padding != n_padding
        || child_layout_change == LayoutTreeChange::LayoutAffecting
        || overflow != n_overflow
        || scroll_offset != n_scroll_offset
        || scroll_anchor_x != n_scroll_anchor_x
        || scroll_anchor_y != n_scroll_anchor_y
    {
        return LayoutTreeChange::LayoutAffecting;
    }

    if editable != n_editable {
        return LayoutTreeChange::LayoutAffecting;
    }

    let border_change = classify_border_change(*border, *n_border);
    if border_change == LayoutTreeChange::LayoutAffecting {
        return LayoutTreeChange::LayoutAffecting;
    }

    let mut change = border_change.combine(child_layout_change);
    if background != n_background || corner_radius != n_corner_radius {
        change = change.combine(LayoutTreeChange::VisualOnly);
    }

    if draw != n_draw || z_index != n_z_index {
        change = change.combine(LayoutTreeChange::VisualOnly);
    }

    if anti_alias != n_anti_alias || hairline_fade != n_hairline_fade {
        change = change.combine(LayoutTreeChange::VisualOnly);
    }

    change = change.combine(classify_material_change(
        material.as_ref(),
        n_material.as_ref(),
    ));

    change.combine(classify_content_change(content, n_content))
}

fn classify_child_layout_change(old: &ChildLayout, next: &ChildLayout) -> LayoutTreeChange {
    match (old, next) {
        (
            ChildLayout::Row {
                gap,
                align_x,
                align_y,
                divider,
            },
            ChildLayout::Row {
                gap: n_gap,
                align_x: n_align_x,
                align_y: n_align_y,
                divider: n_divider,
            },
        )
        | (
            ChildLayout::Column {
                gap,
                align_x,
                align_y,
                divider,
            },
            ChildLayout::Column {
                gap: n_gap,
                align_x: n_align_x,
                align_y: n_align_y,
                divider: n_divider,
            },
        ) => {
            if gap != n_gap || align_x != n_align_x || align_y != n_align_y {
                LayoutTreeChange::LayoutAffecting
            } else {
                classify_child_divider_change(*divider, *n_divider)
            }
        },
        (
            ChildLayout::Overlay { align_x, align_y },
            ChildLayout::Overlay {
                align_x: n_align_x,
                align_y: n_align_y,
            },
        ) => {
            if align_x != n_align_x || align_y != n_align_y {
                LayoutTreeChange::LayoutAffecting
            } else {
                LayoutTreeChange::Identical
            }
        },
        _ => LayoutTreeChange::LayoutAffecting,
    }
}

fn classify_child_divider_change(
    divider: Option<ChildDivider>,
    next: Option<ChildDivider>,
) -> LayoutTreeChange {
    match (divider, next) {
        (None, None) => LayoutTreeChange::Identical,
        (Some(divider), Some(next)) => {
            if divider.width() != next.width() {
                LayoutTreeChange::LayoutAffecting
            } else if divider.color() != next.color() {
                LayoutTreeChange::VisualOnly
            } else {
                LayoutTreeChange::Identical
            }
        },
        (None, Some(_)) | (Some(_), None) => LayoutTreeChange::LayoutAffecting,
    }
}

fn classify_border_change(border: Option<Border>, next: Option<Border>) -> LayoutTreeChange {
    match (border, next) {
        (None, None) => LayoutTreeChange::Identical,
        (Some(border), Some(next)) => {
            let Border {
                color,
                left,
                right,
                top,
                bottom,
            } = border;
            let Border {
                color: n_color,
                left: n_left,
                right: n_right,
                top: n_top,
                bottom: n_bottom,
            } = next;
            if left != n_left || right != n_right || top != n_top || bottom != n_bottom {
                LayoutTreeChange::LayoutAffecting
            } else if color != n_color {
                LayoutTreeChange::VisualOnly
            } else {
                LayoutTreeChange::Identical
            }
        },
        (None, Some(_)) | (Some(_), None) => LayoutTreeChange::LayoutAffecting,
    }
}

fn classify_material_change(
    material: Option<&Handle<StandardMaterial>>,
    next: Option<&Handle<StandardMaterial>>,
) -> LayoutTreeChange {
    match (material, next) {
        (None, None) => LayoutTreeChange::Identical,
        (Some(material), Some(next)) if material == next => LayoutTreeChange::Identical,
        (Some(_) | None, Some(_)) | (Some(_), None) => LayoutTreeChange::VisualOnly,
    }
}

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
            ElementContent::Text {
                text,
                config,
                sizing,
                ..
            },
            ElementContent::Text {
                text: next_text,
                config: next_config,
                sizing: next_sizing,
                ..
            },
        ) => {
            if sizing != next_sizing
                || (!config.layout_eq_excluding_visuals(next_config))
                || (sizing.visible_text_affects_layout() && text != next_text)
            {
                LayoutTreeChange::LayoutAffecting
            } else if text != next_text || config != next_config {
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

#[cfg(test)]
mod tests {
    use bevy::asset::Assets;
    use bevy::asset::Handle;
    use bevy::color::Color;
    use bevy::image::Image;
    use bevy::pbr::StandardMaterial;
    use bevy::prelude::default;

    use super::ElementContent;
    use super::FieldDisplayTextUpdate;
    use super::LayoutTree;
    use super::LayoutTreeChange;
    use crate::CalloutCap;
    use crate::ImeBuiltInFieldKind;
    use crate::ImeBuiltInFieldSpec;
    use crate::ImeEditableFieldSpec;
    use crate::Mm;
    use crate::PanelFieldId;
    use crate::layout::AlignX;
    use crate::layout::AlignY;
    use crate::layout::Border;
    use crate::layout::ChildDivider;
    use crate::layout::ChildLayoutState;
    use crate::layout::Dimension;
    use crate::layout::DrawZIndex;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::Padding;
    use crate::layout::PanelCoord;
    use crate::layout::PanelDraw;
    use crate::layout::PanelLine;
    use crate::layout::PanelShape;
    use crate::layout::Sizing;
    use crate::layout::Text;
    use crate::layout::TextStyle;
    use crate::layout::Unit;
    use crate::layout::child_layout::ChildLayout;

    const LARGE_CHILD_GAP: f32 = 2.0;
    const SMALL_CHILD_GAP: f32 = 1.0;
    const FLOAT_TOLERANCE: f32 = 0.001;

    fn text_tree(text: &str, style: TextStyle) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text((text, style));
        builder.build()
    }

    #[test]
    fn contains_text_id_discriminates_named_typo_from_present() {
        let present = PanelFieldId::named("title");
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(Text::new("Hi", TextStyle::new(10.0)).id(present.clone()));
        let tree = builder.build();

        // A present named id is found; a typo is not — this is the discriminator a
        // `PanelText` lookup miss consults to tell a real typo from a not-yet-built
        // run, so it only warns on the former.
        assert!(tree.contains_text_id(&present));
        assert!(!tree.contains_text_id(&PanelFieldId::named("typo")));

        // An auto-id run (plain `.text`) is not name-addressable, so a named query
        // never matches it.
        let auto = text_tree("Hi", TextStyle::new(10.0));
        assert!(!auto.contains_text_id(&PanelFieldId::named("Hi")));
    }

    fn root_tree<L: ChildLayoutState>(root: El<L>) -> LayoutTree {
        let mut builder = LayoutBuilder::with_root(root);
        builder.text(("child", TextStyle::new(10.0)));
        builder.build()
    }

    #[test]
    fn screen_source_scaled_resolves_world_units_to_pixel_source_values() {
        let mut builder = LayoutBuilder::with_root(
            El::column()
                .width(Sizing::fixed(Mm(20.0)))
                .height(Sizing::fixed(Mm(10.0)))
                .padding(Padding::all(Mm(2.0))),
        );
        builder.text(("child", TextStyle::new(3.0)));
        let tree = builder.build();

        let millimeters_to_points = Unit::Millimeters.to_points();
        let points_to_pixels = 2.0;
        let scaled = tree.screen_source_scaled(
            millimeters_to_points,
            millimeters_to_points,
            points_to_pixels,
        );

        assert!(matches!(scaled.elements[0].width, Sizing::Fixed(_)));
        let Sizing::Fixed(width) = scaled.elements[0].width else {
            return;
        };
        let expected_width = 20.0 * millimeters_to_points * points_to_pixels;
        assert!((width.value - expected_width).abs() < FLOAT_TOLERANCE);
        assert_eq!(width.unit, None);

        let padding = scaled.elements[0].padding;
        let expected_padding = 2.0 * millimeters_to_points * points_to_pixels;
        assert!((padding.left.value - expected_padding).abs() < FLOAT_TOLERANCE);
        assert_eq!(padding.left.unit, None);

        assert!(matches!(
            &scaled.elements[1].content,
            ElementContent::Text { .. }
        ));
        let ElementContent::Text { config, .. } = &scaled.elements[1].content else {
            return;
        };
        let expected_text_size = 3.0 * millimeters_to_points * points_to_pixels;
        assert!((config.size() - expected_text_size).abs() < FLOAT_TOLERANCE);
        assert_eq!(config.unit(), Some(Unit::Pixels));
    }

    fn assert_default_leaf_child_layout(child_layout: ChildLayout) {
        assert_eq!(child_layout, ChildLayout::default());
    }

    fn field_spec() -> ImeEditableFieldSpec {
        ImeEditableFieldSpec::BuiltIn(ImeBuiltInFieldSpec::new(ImeBuiltInFieldKind::Text))
    }

    fn panel_line() -> PanelLine {
        PanelLine::new((1.0, 2.0), (PanelCoord::end(3.0), PanelCoord::percent(0.5)))
            .width(0.25)
            .cap_size(4.0)
            .start_inset(0.5)
            .end_inset(0.75)
            .start_cap(CalloutCap::arrow().length_dimension(Mm(2.0)))
            .end_cap(CalloutCap::circle().radius(1.5))
    }

    #[test]
    fn text_leaf_normalizes_authored_child_layout() {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            Text::new("child", TextStyle::new(10.0)).layout(
                El::column()
                    .gap(4.0)
                    .alignment(AlignX::Right, AlignY::Bottom)
                    .child_divider(ChildDivider::new(1.0, Color::WHITE)),
            ),
        );
        let tree = builder.build();

        assert_default_leaf_child_layout(tree.elements[1].child_layout);
    }

    #[test]
    fn image_leaf_normalizes_authored_child_layout() {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.image(
            El::column()
                .gap(4.0)
                .alignment(AlignX::Right, AlignY::Bottom)
                .child_divider(ChildDivider::new(1.0, Color::WHITE)),
            Handle::<Image>::default(),
            Color::WHITE,
        );
        let tree = builder.build();

        assert_default_leaf_child_layout(tree.elements[1].child_layout);
    }

    #[test]
    fn text_leaf_normalizes_authored_overlay_child_layout() {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            Text::new("child", TextStyle::new(10.0))
                .layout(El::overlay().alignment(AlignX::Right, AlignY::Bottom)),
        );
        let tree = builder.build();

        assert_default_leaf_child_layout(tree.elements[1].child_layout);
    }

    #[test]
    fn image_leaf_normalizes_authored_overlay_child_layout() {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.image(
            El::overlay().alignment(AlignX::Right, AlignY::Bottom),
            Handle::<Image>::default(),
            Color::WHITE,
        );
        let tree = builder.build();

        assert_default_leaf_child_layout(tree.elements[1].child_layout);
    }

    fn approx_eq(a: f32, b: f32) -> bool { (a - b).abs() < f32::EPSILON }

    fn assert_some_approx(actual: Option<f32>, expected: f32) {
        assert!(actual.is_some_and(|value| approx_eq(value, expected)));
    }

    #[test]
    fn identical_tree_classifies_as_identical() {
        let tree = text_tree("same", TextStyle::new(10.0));

        assert_eq!(
            tree.classify_change(&tree.clone()),
            LayoutTreeChange::Identical
        );
    }

    #[test]
    fn text_color_only_classifies_as_visual_only() {
        let tree = text_tree("same", TextStyle::new(10.0).with_color(Color::WHITE));
        let next = text_tree("same", TextStyle::new(10.0).with_color(Color::BLACK));

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::VisualOnly);
    }

    #[test]
    fn background_add_remove_classifies_as_visual_only() {
        let tree = root_tree(El::new().width(Sizing::GROW).height(Sizing::GROW));
        let next = root_tree(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .background(Color::srgb(0.2, 0.3, 0.4)),
        );

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::VisualOnly);
    }

    #[test]
    fn z_index_only_classifies_as_visual_only() {
        let tree = root_tree(El::new().width(Sizing::GROW).height(Sizing::GROW));
        let next = root_tree(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .z_index(DrawZIndex(1)),
        );

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::VisualOnly);
    }

    #[test]
    fn builder_stores_panel_draw_on_element() {
        let tree = root_tree(El::new().draw(PanelDraw::lines([panel_line()])));

        assert_eq!(
            tree.element_draw(0).map(|draw| draw.shapes_ref().len()),
            Some(1)
        );
    }

    #[test]
    fn draw_only_change_classifies_as_visual_only() {
        let tree = root_tree(El::new());
        let next = root_tree(El::new().draw(PanelDraw::lines([panel_line()])));

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::VisualOnly);
    }

    #[test]
    fn draw_only_change_is_excluded_from_structure_hash() {
        let tree = root_tree(El::new());
        let next = root_tree(El::new().draw(PanelDraw::lines([panel_line()])));

        assert_eq!(tree.structure_hash(), next.structure_hash());
    }

    #[test]
    fn scaled_tree_scales_panel_draw_dimensions() {
        let scale = 3.0;
        let tree = root_tree(El::new().draw(PanelDraw::lines([panel_line()])));
        let scaled = tree.scaled(scale, scale);
        let line = scaled
            .element_draw(0)
            .and_then(|draw| draw.shapes_ref().first())
            .and_then(PanelShape::as_line);

        assert_some_approx(
            line.and_then(|line| line.start().x().start_dimension())
                .map(|dimension| dimension.value),
            3.0,
        );
        assert_some_approx(
            line.and_then(|line| line.start().y().start_dimension())
                .map(|dimension| dimension.value),
            6.0,
        );
        assert_some_approx(
            line.and_then(|line| line.end().x().end_dimension())
                .map(|dimension| dimension.value),
            9.0,
        );
        assert_eq!(
            line.and_then(|line| line.end().y().percent_value()),
            Some(0.5)
        );
        assert_some_approx(
            line.map(|line| line.line_style().width_dimension().value),
            0.75,
        );
        assert_some_approx(
            line.map(|line| line.line_style().cap_size_dimension().value),
            12.0,
        );
        assert_some_approx(
            line.map(PanelLine::start_inset_dimension)
                .map(|dimension| dimension.value),
            1.5,
        );
        assert_some_approx(
            line.map(PanelLine::end_inset_dimension)
                .map(|dimension| dimension.value),
            2.25,
        );

        let expected_arrow_inset = Dimension::from(Mm(2.0)).to_points(scale);
        assert_some_approx(
            line.map(|line| {
                line.line_style()
                    .start_cap_value()
                    .resolved_primitives(99.0, Color::WHITE, |dimension| dimension.value)
                    .shaft_inset
            }),
            expected_arrow_inset,
        );
        assert_some_approx(
            line.map(|line| {
                line.line_style()
                    .end_cap_value()
                    .resolved_primitives(99.0, Color::WHITE, |dimension| dimension.value)
                    .shaft_inset
            }),
            4.5,
        );
    }

    #[test]
    fn text_content_change_classifies_as_layout_affecting() {
        let tree = text_tree("before", TextStyle::new(10.0));
        let next = text_tree("after", TextStyle::new(10.0));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn text_measurement_change_classifies_as_layout_affecting() {
        let tree = text_tree("same", TextStyle::new(10.0));
        let next = text_tree("same", TextStyle::new(11.0));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn border_color_only_classifies_as_visual_only() {
        let tree = root_tree(El::new().border(Border::all(2.0, Color::WHITE)));
        let next = root_tree(El::new().border(Border::all(2.0, Color::BLACK)));

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::VisualOnly);
    }

    #[test]
    fn border_width_change_classifies_as_layout_affecting() {
        let tree = root_tree(El::new().border(Border::all(2.0, Color::WHITE)));
        let next = root_tree(El::new().border(Border::all(3.0, Color::WHITE)));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn divider_color_only_classifies_as_visual_only() {
        let tree = root_tree(El::row().child_divider(ChildDivider::new(2.0, Color::WHITE)));
        let next = root_tree(El::row().child_divider(ChildDivider::new(2.0, Color::BLACK)));

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::VisualOnly);
    }

    #[test]
    fn divider_width_change_classifies_as_layout_affecting() {
        let tree = root_tree(El::row().child_divider(ChildDivider::new(2.0, Color::WHITE)));
        let next = root_tree(El::row().child_divider(ChildDivider::new(3.0, Color::WHITE)));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn combined_visual_and_layout_change_classifies_as_layout_affecting() {
        let tree = root_tree(
            El::new()
                .padding(Padding::all(4.0))
                .background(Color::WHITE),
        );
        let next = root_tree(
            El::new()
                .padding(Padding::all(8.0))
                .background(Color::BLACK),
        );

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn row_gap_change_classifies_as_layout_affecting() {
        let tree = root_tree(El::row().gap(SMALL_CHILD_GAP));
        let next = root_tree(El::row().gap(LARGE_CHILD_GAP));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn column_alignment_change_classifies_as_layout_affecting() {
        let tree = root_tree(El::column().align_x(AlignX::Left));
        let next = root_tree(El::column().align_x(AlignX::Right));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn row_to_column_change_classifies_as_layout_affecting() {
        let tree = root_tree(El::row());
        let next = root_tree(El::column());

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn overlay_alignment_change_classifies_as_layout_affecting() {
        let tree = root_tree(El::overlay().align_x(AlignX::Left));
        let next = root_tree(El::overlay().align_x(AlignX::Right));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn row_to_overlay_change_classifies_as_layout_affecting() {
        let tree = root_tree(El::row());
        let next = root_tree(El::overlay());

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn empty_to_populated_tree_classifies_as_layout_affecting() {
        let tree = LayoutTree::new();
        let next = text_tree("child", TextStyle::new(10.0));

        assert_eq!(
            tree.classify_change(&next),
            LayoutTreeChange::LayoutAffecting
        );
        assert_eq!(
            next.classify_change(&tree),
            LayoutTreeChange::LayoutAffecting
        );
    }

    #[test]
    fn material_handle_add_remove_and_swap_are_visual_only() {
        let mut materials = Assets::<StandardMaterial>::default();
        let first = materials.add(StandardMaterial::default());
        let second = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.0, 0.0),
            ..default()
        });

        let tree = root_tree(El::new());
        let next = root_tree(El::new().material(first));
        assert_eq!(tree.classify_change(&next), LayoutTreeChange::VisualOnly);
        assert_eq!(next.classify_change(&tree), LayoutTreeChange::VisualOnly);

        let swapped = root_tree(El::new().material(second));
        assert_eq!(next.classify_change(&swapped), LayoutTreeChange::VisualOnly);
    }

    #[test]
    fn identical_material_handle_is_identical() {
        let mut materials = Assets::<StandardMaterial>::default();
        let material = materials.add(StandardMaterial::default());
        let tree = root_tree(El::new().material(material.clone()));
        let next = root_tree(El::new().material(material));

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::Identical);
    }

    #[test]
    fn layout_text_ignores_standalone_only_world_scale() {
        let tree = text_tree("same", TextStyle::new(10.0));
        let next = text_tree("same", TextStyle::new(10.0));

        assert_eq!(tree.classify_change(&next), LayoutTreeChange::Identical);
    }

    #[test]
    fn updates_field_display_text_descendant() {
        let mut builder = LayoutBuilder::new(100.0, 40.0);
        builder.with(El::new().editable_field("gain", field_spec()), |builder| {
            builder.text(("10", TextStyle::new(10.0)));
        });
        let mut tree = builder.build();

        let update = tree.set_field_display_text(&"gain".into(), "11");

        assert_eq!(update, FieldDisplayTextUpdate::Updated);
        assert_eq!(tree.field_display_text(1), Some("11"));
    }

    #[test]
    fn rejects_duplicate_field_display_update() {
        let mut builder = LayoutBuilder::new(100.0, 40.0);
        builder.with(El::new().editable_field("gain", field_spec()), |builder| {
            builder.text(("10", TextStyle::new(10.0)));
        });
        builder.with(El::new().editable_field("gain", field_spec()), |builder| {
            builder.text(("12", TextStyle::new(10.0)));
        });
        let mut tree = builder.build();

        let update = tree.set_field_display_text(&"gain".into(), "11");

        assert_eq!(update, FieldDisplayTextUpdate::DuplicateField);
    }
}

//! Core layout types shared across the layout engine.
//!
//! This module defines the fundamental building blocks for layout configuration:
//! [`Sizing`], [`Direction`], [`AlignX`]/[`AlignY`], [`Padding`], [`Border`],
//! and [`TextProps`].
//!
//! [`TextProps`] uses a typestate pattern parameterized by context markers
//! ([`ForLayout`] / [`ForStandalone`]) to enforce compile-time validity.
//! Type aliases [`TextConfig`] and [`TextStyle`] provide ergonomic names.

use std::marker::PhantomData;

use bevy::color::Color;
use bevy::prelude::Component;
use bevy::prelude::Reflect;

/// Sizing behavior for a layout element along one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    /// Shrink-wrap to content, clamped to `[min, max]`.
    ///
    /// The element's content size is computed first (e.g. via text measurement
    /// or children accumulation), then clamped to the `[min, max]` range.
    /// If content is smaller than `min`, the element grows to `min`.
    /// If content is larger than `max`, the element is capped at `max`.
    Fit {
        /// Minimum size in layout units.
        min: f32,
        /// Maximum size in layout units.
        max: f32,
    },
    /// Expand to fill remaining parent space, clamped to `[min, max]`.
    ///
    /// After all non-`Grow` siblings are sized, remaining space is distributed
    /// among `Grow` siblings using a smallest-first equalising heuristic:
    /// the smallest `Grow` elements receive space first until they match the
    /// next-smallest, then all are grown together, repeating until space is
    /// exhausted or every element hits its `max`.
    ///
    /// `min` acts as a guaranteed floor -- the element never shrinks below it.
    /// `max` caps expansion -- the element stops growing once it reaches `max`.
    Grow {
        /// Minimum size in layout units.
        min: f32,
        /// Maximum size in layout units.
        max: f32,
    },
    /// Exact size in layout units.
    Fixed(f32),
    /// Fraction of the parent's size along this axis (0.0--1.0).
    ///
    /// Along the parent's layout direction, padding and child gaps are
    /// subtracted before computing the fraction.
    Percent(f32),
}

impl Default for Sizing {
    fn default() -> Self {
        Self::Fit {
            min: 0.0,
            max: f32::MAX,
        }
    }
}

impl Sizing {
    /// Shrink-wrap to content with no size constraints.
    ///
    /// The element's content size is computed first (e.g. via text measurement
    /// or children accumulation) and used as-is — there is no minimum floor
    /// and no maximum cap.
    pub const FIT: Self = Self::Fit {
        min: 0.0,
        max: f32::MAX,
    };

    /// Expand to fill available space with no size constraints.
    ///
    /// After all non-`Grow` siblings are sized, remaining space is distributed
    /// among `Grow` siblings using a smallest-first equalising heuristic.
    /// With no constraints, this element will absorb as much space as possible.
    pub const GROW: Self = Self::Grow {
        min: 0.0,
        max: f32::MAX,
    };

    /// Shrink-wrap to content with a minimum floor.
    ///
    /// The element will never be smaller than `min`, even if content is smaller.
    #[must_use]
    pub const fn fit_min(min: f32) -> Self { Self::Fit { min, max: f32::MAX } }

    /// Shrink-wrap to content, clamped to `[min, max]`.
    ///
    /// Content smaller than `min` grows to `min`; content larger than `max`
    /// is capped at `max`.
    #[must_use]
    pub const fn fit_range(min: f32, max: f32) -> Self { Self::Fit { min, max } }

    /// Expand to fill available space with a minimum floor.
    ///
    /// The element is guaranteed at least `min` even if no space remains.
    #[must_use]
    pub const fn grow_min(min: f32) -> Self { Self::Grow { min, max: f32::MAX } }

    /// Expand to fill available space, clamped to `[min, max]`.
    ///
    /// `min` is a guaranteed floor; `max` caps expansion.
    #[must_use]
    pub const fn grow_range(min: f32, max: f32) -> Self { Self::Grow { min, max } }

    /// Exact size in layout units, ignoring content and siblings.
    #[must_use]
    pub const fn fixed(size: f32) -> Self { Self::Fixed(size) }

    /// Fraction of the parent's content area (0.0–1.0).
    #[must_use]
    pub const fn percent(fraction: f32) -> Self { Self::Percent(fraction) }

    /// Returns the minimum bound for this sizing rule.
    #[must_use]
    pub const fn min_size(&self) -> f32 {
        match self {
            Self::Fit { min, .. } | Self::Grow { min, .. } => *min,
            Self::Fixed(size) => *size,
            Self::Percent(_) => 0.0,
        }
    }

    /// Returns the maximum bound for this sizing rule.
    #[must_use]
    pub const fn max_size(&self) -> f32 {
        match self {
            Self::Fit { max, .. } | Self::Grow { max, .. } => *max,
            Self::Fixed(size) => *size,
            Self::Percent(_) => f32::MAX,
        }
    }

    /// Returns `true` if this is a `Grow` variant.
    #[must_use]
    pub const fn is_grow(&self) -> bool { matches!(self, Self::Grow { .. }) }

    /// Returns `true` if this is a `Fit` variant.
    #[must_use]
    pub const fn is_fit(&self) -> bool { matches!(self, Self::Fit { .. }) }

    /// Returns `true` if this element can be compressed during overflow.
    #[must_use]
    pub const fn is_resizable(&self) -> bool {
        matches!(self, Self::Fit { .. } | Self::Grow { .. })
    }
}

/// Direction in which children are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Children flow left to right.
    #[default]
    LeftToRight,
    /// Children flow top to bottom.
    TopToBottom,
}

/// Horizontal alignment of children within their parent.
///
/// When [`Direction::LeftToRight`], this controls main-axis alignment (distributes
/// extra space before/after the row of children). When [`Direction::TopToBottom`],
/// this controls cross-axis alignment (positions each child horizontally within
/// the parent's content area).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignX {
    /// Align to the left edge.
    #[default]
    Left,
    /// Center horizontally.
    Center,
    /// Align to the right edge.
    Right,
}

/// Vertical alignment of children within their parent.
///
/// When [`Direction::TopToBottom`], this controls main-axis alignment (distributes
/// extra space before/after the column of children). When [`Direction::LeftToRight`],
/// this controls cross-axis alignment (positions each child vertically within
/// the parent's content area).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignY {
    /// Align to the top edge.
    #[default]
    Top,
    /// Center vertically.
    Center,
    /// Align to the bottom edge.
    Bottom,
}

/// Interior padding between an element's edges and its children.
///
/// Note: [`Sizing::Percent`] on child elements is computed against the parent's
/// content area (i.e., after this padding and child gap are subtracted).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    /// Left padding in layout units.
    pub left:   f32,
    /// Right padding in layout units.
    pub right:  f32,
    /// Top padding in layout units.
    pub top:    f32,
    /// Bottom padding in layout units.
    pub bottom: f32,
}

impl Padding {
    /// Uniform padding on all sides.
    #[must_use]
    pub const fn all(value: f32) -> Self {
        Self {
            left:   value,
            right:  value,
            top:    value,
            bottom: value,
        }
    }

    /// Symmetric padding: `x` for left/right, `y` for top/bottom.
    #[must_use]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            left:   x,
            right:  x,
            top:    y,
            bottom: y,
        }
    }

    /// Individual padding per side.
    #[must_use]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    /// Total horizontal padding (left + right).
    #[must_use]
    pub const fn horizontal(&self) -> f32 { self.left + self.right }

    /// Total vertical padding (top + bottom).
    #[must_use]
    pub const fn vertical(&self) -> f32 { self.top + self.bottom }
}

/// Computed axis-aligned bounding box in layout coordinates (top-left origin).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoundingBox {
    /// X position of the top-left corner.
    pub x:      f32,
    /// Y position of the top-left corner.
    pub y:      f32,
    /// Width of the bounding box.
    pub width:  f32,
    /// Height of the bounding box.
    pub height: f32,
}

impl BoundingBox {
    /// Returns the center point of this bounding box.
    #[must_use]
    pub const fn center(&self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }
}

/// Controls how the layout engine breaks text across lines.
///
/// The engine splits text according to this mode and measures individual
/// runs via the [`MeasureTextFn`](super::engine::MeasureTextFn) callback
/// to determine break points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TextWrap {
    /// Break at word boundaries when text exceeds the element's width.
    ///
    /// Words are split on ASCII whitespace. The engine measures each word
    /// individually, accumulates widths on a line, and breaks when the
    /// next word would exceed the available width.
    #[default]
    Words,
    /// Break only at explicit `\n` characters.
    ///
    /// Each line between newlines is measured as a single run. The element's
    /// width is the widest line; height is the sum of all line heights.
    Newlines,
    /// Never wrap. The full text is measured as a single run and may
    /// overflow the element's bounds.
    None,
}

// ── Font property types ──────────────────────────────────────────────────────

/// Font weight (boldness) as a numeric value on the 1–1000 scale.
///
/// Standard weights: 100 (Thin) through 900 (Black). `400` is normal, `700` is bold.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct FontWeight(pub f32);

impl FontWeight {
    /// Normal weight (400).
    pub const NORMAL: Self = Self(400.0);
    /// Bold weight (700).
    pub const BOLD: Self = Self(700.0);
    /// Light weight (300).
    pub const LIGHT: Self = Self(300.0);
}

impl Default for FontWeight {
    fn default() -> Self { Self::NORMAL }
}

/// Font slant (posture).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum FontSlant {
    /// Upright (roman) style.
    #[default]
    Normal,
    /// Italic style (true italic glyphs).
    Italic,
    /// Oblique style (slanted roman glyphs).
    Oblique,
}

/// Horizontal text alignment within bounds.
///
/// Used by [`TextProps<ForStandalone>`] for standalone text rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TextAlign {
    /// Align to the left edge.
    #[default]
    Left,
    /// Center horizontally.
    Center,
    /// Align to the right edge.
    Right,
}

/// Anchor point for standalone text positioning.
///
/// Determines which point of the text block's bounding box is placed
/// at the entity's [`Transform`] position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TextAnchor {
    /// Top-left corner at the transform position.
    TopLeft,
    /// Top-center at the transform position.
    TopCenter,
    /// Top-right corner at the transform position.
    TopRight,
    /// Center-left at the transform position.
    CenterLeft,
    /// Center of the text block at the transform position.
    #[default]
    Center,
    /// Center-right at the transform position.
    CenterRight,
    /// Bottom-left corner at the transform position.
    BottomLeft,
    /// Bottom-center at the transform position.
    BottomCenter,
    /// Bottom-right corner at the transform position.
    BottomRight,
}

/// How the visible glyph renders.
///
/// Controls the MSDF shader's alpha computation. All modes use
/// `AlphaMode::Blend` for smooth anti-aliased edges.
/// Discriminants are `#[repr(u32)]` and explicit because they map
/// directly to shader constants in `msdf_text.wgsl`. Adding or
/// reordering variants without updating the shader will cause a
/// compile-time test failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[repr(u32)]
pub enum GlyphRenderMode {
    /// No visible text — only the shadow proxy renders (if shadow mode
    /// is not `None`). Useful for shadow-only effects.
    Invisible = 0,
    /// Normal MSDF text rendering — smooth alpha-blended edges.
    #[default]
    Text      = 1,
    /// Background quad with the text shape cut out (inverted alpha).
    PunchOut  = 2,
    /// Opaque quad matching the glyph shape (no MSDF decode).
    SolidQuad = 3,
}

/// What shape the shadow cast by glyphs takes.
///
/// Independent of [`GlyphRenderMode`] — the visible glyph and its shadow
/// can use different shapes. Shaped shadows (`Text`, `PunchOut`) spawn a
/// separate shadow proxy mesh with `AlphaMode::Mask` that is invisible
/// in the main pass but contributes to the shadow prepass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum GlyphShadowMode {
    /// No shadow casting.
    None,
    /// Rectangular shadow from quad geometry (default, current behavior).
    #[default]
    SolidQuad,
    /// Shadow follows the text outline (MSDF-decoded in prepass).
    Text,
    /// Shadow follows the punch-out shape (inverted MSDF in prepass).
    PunchOut,
}

// ── Typestate markers ────────────────────────────────────────────────────────

/// Context marker: text properties for the layout engine.
///
/// [`TextProps<ForLayout>`] (aliased as [`TextConfig`]) exposes wrapping
/// controls but not color, alignment, or anchor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Reflect)]
pub struct ForLayout;

/// Context marker: text properties for standalone 3D text rendering.
///
/// [`TextProps<ForStandalone>`] (aliased as [`TextStyle`]) exposes color,
/// alignment, and anchor but not wrapping.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Reflect)]
pub struct ForStandalone;

// ── TextProps<C> ─────────────────────────────────────────────────────────────

/// Type alias for layout engine text configuration.
pub type TextConfig = TextProps<ForLayout>;

/// Type alias for standalone text styling (Bevy `Component`).
pub type TextStyle = TextProps<ForStandalone>;

/// Default font size in layout units.
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Unified text properties parameterized by usage context.
///
/// `TextProps<ForLayout>` is used by the layout engine for measurement
/// and wrapping. `TextProps<ForStandalone>` is used as a Bevy `Component`
/// for standalone `Text3d` entities.
///
/// All fields are private. Context-appropriate builder methods expose
/// only the fields that make sense for each context. Shared measurement
/// fields are accessible on both via the `impl<C>` block.
///
/// ```ignore
/// // Layout (aliased as TextConfig):
/// TextConfig::new(14.0).with_font(FontId::MONOSPACE.0).bold().no_wrap()
///
/// // Standalone (aliased as TextStyle):
/// TextStyle::new().with_font(FontId::MONOSPACE.0).with_size(24.0).bold().with_color(Color::RED)
/// ```
#[derive(Component, Clone, Debug, Reflect)]
pub struct TextProps<C: Send + Sync + 'static> {
    font_id:        u16,
    size:           f32,
    weight:         FontWeight,
    slant:          FontSlant,
    line_height:    f32,
    letter_spacing: f32,
    word_spacing:   f32,
    wrap:           TextWrap,
    color:          Color,
    align:          TextAlign,
    anchor:         TextAnchor,
    render_mode:    GlyphRenderMode,
    shadow_mode:    GlyphShadowMode,
    #[reflect(ignore)]
    _context:       PhantomData<C>,
}

impl<C: Send + Sync + 'static> PartialEq for TextProps<C> {
    fn eq(&self, other: &Self) -> bool {
        self.font_id == other.font_id
            && self.size == other.size
            && self.weight == other.weight
            && self.slant == other.slant
            && self.line_height == other.line_height
            && self.letter_spacing == other.letter_spacing
            && self.word_spacing == other.word_spacing
            && self.wrap == other.wrap
            && self.color == other.color
            && self.align == other.align
            && self.anchor == other.anchor
            && self.render_mode == other.render_mode
            && self.shadow_mode == other.shadow_mode
    }
}

// ── Shared methods (both contexts) ───────────────────────────────────────────

impl<C: Send + Sync + 'static> TextProps<C> {
    /// Returns the font identifier.
    #[must_use]
    pub const fn font_id(&self) -> u16 { self.font_id }

    /// Returns the font size in layout units.
    #[must_use]
    pub const fn size(&self) -> f32 { self.size }

    /// Returns the font weight.
    #[must_use]
    pub const fn weight(&self) -> FontWeight { self.weight }

    /// Returns the font slant.
    #[must_use]
    pub const fn slant(&self) -> FontSlant { self.slant }

    /// Returns the line height in layout units (0.0 = use `size`).
    #[must_use]
    pub const fn line_height_raw(&self) -> f32 { self.line_height }

    /// Returns the letter spacing in layout units.
    #[must_use]
    pub const fn letter_spacing(&self) -> f32 { self.letter_spacing }

    /// Returns the word spacing in layout units.
    #[must_use]
    pub const fn word_spacing(&self) -> f32 { self.word_spacing }

    /// Returns the effective line height (falls back to `size` if 0.0).
    #[must_use]
    pub const fn effective_line_height(&self) -> f32 {
        if self.line_height == 0.0 {
            self.size
        } else {
            self.line_height
        }
    }

    /// Sets the font identifier.
    #[must_use]
    pub const fn with_font(mut self, font_id: u16) -> Self {
        self.font_id = font_id;
        self
    }

    /// Sets the font size in layout units.
    #[must_use]
    pub const fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Sets the font weight.
    #[must_use]
    pub const fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Shorthand for `with_weight(FontWeight::BOLD)`.
    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.weight = FontWeight::BOLD;
        self
    }

    /// Sets the font slant.
    #[must_use]
    pub const fn with_slant(mut self, slant: FontSlant) -> Self {
        self.slant = slant;
        self
    }

    /// Shorthand for `with_slant(FontSlant::Italic)`.
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.slant = FontSlant::Italic;
        self
    }

    /// Sets the line height in layout units. `0.0` = use `size`.
    #[must_use]
    pub const fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// Sets extra spacing between characters in layout units.
    #[must_use]
    pub const fn with_letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Sets extra spacing between words in layout units.
    #[must_use]
    pub const fn with_word_spacing(mut self, spacing: f32) -> Self {
        self.word_spacing = spacing;
        self
    }

    /// Returns the text color.
    #[must_use]
    pub const fn color(&self) -> Color { self.color }

    /// Sets the text color (mutable reference variant).
    pub const fn set_color(&mut self, color: Color) { self.color = color; }

    /// Sets the text color.
    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Returns the glyph render mode.
    #[must_use]
    pub const fn render_mode(&self) -> GlyphRenderMode { self.render_mode }

    /// Sets the glyph render mode.
    #[must_use]
    pub const fn with_render_mode(mut self, mode: GlyphRenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Returns the glyph shadow mode.
    #[must_use]
    pub const fn shadow_mode(&self) -> GlyphShadowMode { self.shadow_mode }

    /// Sets the glyph shadow mode.
    #[must_use]
    pub const fn with_shadow_mode(mut self, mode: GlyphShadowMode) -> Self {
        self.shadow_mode = mode;
        self
    }

    /// Hashes all layout-affecting fields into `hasher`, excluding color.
    ///
    /// Uses exhaustive destructuring so that adding a new field to
    /// [`TextProps`] without updating this method is a compiler error.
    pub fn hash_layout(&self, hasher: &mut impl std::hash::Hasher) {
        use std::hash::Hash;

        // Destructure exhaustively — compiler error if a field is added.
        let Self {
            font_id,
            size,
            weight,
            slant,
            line_height,
            letter_spacing,
            word_spacing,
            wrap,
            align,
            anchor,
            // Render-only — explicitly skipped.
            color: _,
            render_mode: _,
            shadow_mode: _,
            _context: _,
        } = self;

        font_id.hash(hasher);
        size.to_bits().hash(hasher);
        weight.0.to_bits().hash(hasher);
        (*slant as u8).hash(hasher);
        line_height.to_bits().hash(hasher);
        letter_spacing.to_bits().hash(hasher);
        word_spacing.to_bits().hash(hasher);
        (*wrap as u8).hash(hasher);
        (*align as u8).hash(hasher);
        (*anchor as u8).hash(hasher);
    }

    /// Extracts measurement-relevant fields as a [`TextMeasure`].
    ///
    /// Used by [`MeasureTextFn`](super::engine::MeasureTextFn) — no generic
    /// parameter, no infection into the layout engine.
    #[must_use]
    pub const fn as_measure(&self) -> TextMeasure {
        TextMeasure {
            font_id:        self.font_id,
            size:           self.size,
            weight:         self.weight,
            slant:          self.slant,
            line_height:    self.line_height,
            letter_spacing: self.letter_spacing,
            word_spacing:   self.word_spacing,
        }
    }
}

// ── Layout-only methods ──────────────────────────────────────────────────────

impl TextProps<ForLayout> {
    /// Creates a new layout config with the given font size.
    ///
    /// Defaults to word wrapping, normal weight, normal slant.
    #[must_use]
    pub const fn new(size: f32) -> Self {
        Self {
            font_id: 0,
            size,
            weight: FontWeight::NORMAL,
            slant: FontSlant::Normal,
            line_height: 0.0,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            wrap: TextWrap::Words,
            color: Color::WHITE,
            align: TextAlign::Left,
            anchor: TextAnchor::Center,
            render_mode: GlyphRenderMode::Text,
            shadow_mode: GlyphShadowMode::SolidQuad,
            _context: PhantomData,
        }
    }

    /// Returns the text wrapping mode.
    #[must_use]
    pub const fn wrap_mode(&self) -> TextWrap { self.wrap }

    /// Sets the text wrapping mode.
    #[must_use]
    pub const fn wrap(mut self, mode: TextWrap) -> Self {
        self.wrap = mode;
        self
    }

    /// Disables text wrapping (text may overflow the element).
    #[must_use]
    pub const fn no_wrap(mut self) -> Self {
        self.wrap = TextWrap::None;
        self
    }
}

impl Default for TextProps<ForLayout> {
    fn default() -> Self { Self::new(DEFAULT_FONT_SIZE) }
}

// ── Standalone-only methods ──────────────────────────────────────────────────

impl TextProps<ForStandalone> {
    /// Creates a new style with all defaults (16-unit white monospace, centered anchor).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            font_id:        0,
            size:           DEFAULT_FONT_SIZE,
            weight:         FontWeight::NORMAL,
            slant:          FontSlant::Normal,
            line_height:    0.0,
            letter_spacing: 0.0,
            word_spacing:   0.0,
            wrap:           TextWrap::None,
            color:          Color::WHITE,
            align:          TextAlign::Left,
            anchor:         TextAnchor::Center,
            render_mode:    GlyphRenderMode::Text,
            shadow_mode:    GlyphShadowMode::SolidQuad,
            _context:       PhantomData,
        }
    }

    /// Returns the text alignment.
    #[must_use]
    pub const fn text_align(&self) -> TextAlign { self.align }

    /// Returns the anchor point.
    #[must_use]
    pub const fn anchor(&self) -> TextAnchor { self.anchor }

    /// Sets horizontal text alignment within bounds.
    #[must_use]
    pub const fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Sets the anchor point within the text block's bounding box.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: TextAnchor) -> Self {
        self.anchor = anchor;
        self
    }
}

impl Default for TextProps<ForStandalone> {
    fn default() -> Self { Self::new() }
}

impl TextProps<ForStandalone> {
    /// Converts to a [`TextConfig`] for use with the shaping/rendering pipeline.
    ///
    /// Copies all shared measurement fields and color. The layout-specific
    /// `wrap` field is set to [`TextWrap::None`] since standalone text does
    /// not word-wrap by default.
    #[must_use]
    pub fn as_layout_config(&self) -> TextProps<ForLayout> {
        TextProps::<ForLayout>::new(self.size)
            .with_font(self.font_id)
            .with_weight(self.weight)
            .with_slant(self.slant)
            .with_line_height(self.line_height)
            .with_letter_spacing(self.letter_spacing)
            .with_word_spacing(self.word_spacing)
            .with_color(self.color)
            .with_render_mode(self.render_mode)
            .with_shadow_mode(self.shadow_mode)
            .no_wrap()
    }
}

// ── TextMeasure ──────────────────────────────────────────────────────────────

/// The subset of text properties needed for measurement.
///
/// Extracted from [`TextProps<C>`] via [`as_measure()`](TextProps::as_measure).
/// This is what [`MeasureTextFn`](super::engine::MeasureTextFn) receives — no
/// generic parameter, no infection into the layout engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasure {
    /// Font identifier.
    pub font_id:        u16,
    /// Font size in layout units.
    pub size:           f32,
    /// Font weight.
    pub weight:         FontWeight,
    /// Font slant.
    pub slant:          FontSlant,
    /// Line height (0.0 = use `size`).
    pub line_height:    f32,
    /// Letter spacing in layout units.
    pub letter_spacing: f32,
    /// Word spacing in layout units.
    pub word_spacing:   f32,
}

impl TextMeasure {
    /// Returns the effective line height (falls back to `size` if 0.0).
    #[must_use]
    pub const fn effective_line_height(&self) -> f32 {
        if self.line_height == 0.0 {
            self.size
        } else {
            self.line_height
        }
    }
}

/// Measured dimensions of a text string.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextDimensions {
    /// Width in layout units.
    pub width:  f32,
    /// Height in layout units.
    pub height: f32,
}

/// Border widths for an element.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Border {
    /// Left border width.
    pub left:             f32,
    /// Right border width.
    pub right:            f32,
    /// Top border width.
    pub top:              f32,
    /// Bottom border width.
    pub bottom:           f32,
    /// Color of the border.
    pub color:            Color,
    /// Width of lines drawn between children (0 = none).
    pub between_children: f32,
}

impl Border {
    /// Creates a border with all widths at zero and default color.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            left:             0.0,
            right:            0.0,
            top:              0.0,
            bottom:           0.0,
            color:            Color::BLACK,
            between_children: 0.0,
        }
    }

    /// Uniform border on all sides.
    #[must_use]
    pub const fn all(width: f32, color: Color) -> Self {
        Self {
            left: width,
            right: width,
            top: width,
            bottom: width,
            color,
            between_children: 0.0,
        }
    }

    /// Sets the left border width.
    #[must_use]
    pub const fn left(mut self, width: f32) -> Self {
        self.left = width;
        self
    }

    /// Sets the right border width.
    #[must_use]
    pub const fn right(mut self, width: f32) -> Self {
        self.right = width;
        self
    }

    /// Sets the top border width.
    #[must_use]
    pub const fn top(mut self, width: f32) -> Self {
        self.top = width;
        self
    }

    /// Sets the bottom border width.
    #[must_use]
    pub const fn bottom(mut self, width: f32) -> Self {
        self.bottom = width;
        self
    }

    /// Sets the border color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the width of lines drawn between children.
    #[must_use]
    pub const fn between_children(mut self, width: f32) -> Self {
        self.between_children = width;
        self
    }
}

// ── Shader discriminant assertions ──────────────────────────────────────────
//
// These compile-time assertions ensure that `GlyphRenderMode` discriminants
// stay in sync with the constants in `assets/shaders/msdf_text.wgsl`.
// If you add or reorder variants, update the shader constants to match
// and adjust these assertions.

const _: () = assert!(GlyphRenderMode::Invisible as u32 == 0);
const _: () = assert!(GlyphRenderMode::Text as u32 == 1);
const _: () = assert!(GlyphRenderMode::PunchOut as u32 == 2);
const _: () = assert!(GlyphRenderMode::SolidQuad as u32 == 3);

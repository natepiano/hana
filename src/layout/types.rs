//! Core layout types shared across the layout engine.
//!
//! This module defines the fundamental building blocks for layout configuration:
//! [`Sizing`], [`Direction`], [`AlignX`]/[`AlignY`], [`Padding`], [`Border`],
//! [`TextConfig`], and [`Culling`].

use bevy::color::Color;

/// Controls whether the layout engine culls off-screen render commands.
///
/// When enabled (the default), elements whose bounding box lies entirely
/// outside the viewport are omitted from the render command list. This
/// matches Clay's default behavior and avoids unnecessary draw calls.
///
/// Disable culling when you need the full command list regardless of
/// viewport position — for example, when pre-computing a layout that
/// will be scrolled into view later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Culling {
    /// Skip render commands for elements fully outside the viewport.
    #[default]
    Enabled,
    /// Emit render commands for all elements regardless of position.
    Disabled,
}

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// Configuration for how text is measured and rendered.
///
/// Text color is not configured here — it is handled by the rendering layer.
///
/// Per-element text alignment (left/center/right) is not currently supported.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextConfig {
    /// Font identifier (application-defined).
    ///
    /// The layout engine does not manage fonts. The application must assign a
    /// unique ID to each font and provide matching measurements via the
    /// [`MeasureTextFn`](super::engine::MeasureTextFn) callback.
    pub font_id:        u16,
    /// Font size in layout units.
    pub font_size:      u16,
    /// Line height in layout units (0 = use `font_size`).
    pub line_height:    u16,
    /// Letter spacing in layout units.
    pub letter_spacing: u16,
    /// Text wrapping mode.
    ///
    /// Controls whether and how the layout engine breaks text across
    /// multiple lines. Defaults to [`TextWrap::Words`].
    pub wrap:           TextWrap,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            font_id:        0,
            font_size:      16,
            line_height:    0,
            letter_spacing: 0,
            wrap:           TextWrap::Words,
        }
    }
}

impl TextConfig {
    /// Creates a new text config with the given font size.
    pub const fn new(font_size: u16) -> Self {
        Self {
            font_id: 0,
            font_size,
            line_height: 0,
            letter_spacing: 0,
            wrap: TextWrap::Words,
        }
    }

    /// Sets the font identifier.
    pub const fn with_font_id(mut self, font_id: u16) -> Self {
        self.font_id = font_id;
        self
    }

    /// Sets the line height in layout units.
    pub const fn with_line_height(mut self, line_height: u16) -> Self {
        self.line_height = line_height;
        self
    }

    /// Sets the letter spacing in layout units.
    pub const fn with_letter_spacing(mut self, letter_spacing: u16) -> Self {
        self.letter_spacing = letter_spacing;
        self
    }

    /// Disables text wrapping (text may overflow the element).
    pub const fn no_wrap(mut self) -> Self {
        self.wrap = TextWrap::None;
        self
    }

    /// Sets the text wrapping mode.
    pub const fn wrap_mode(mut self, mode: TextWrap) -> Self {
        self.wrap = mode;
        self
    }

    /// Returns the effective line height (falls back to `font_size` if 0).
    #[must_use]
    pub const fn effective_line_height(&self) -> f32 {
        if self.line_height == 0 {
            self.font_size as f32
        } else {
            self.line_height as f32
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

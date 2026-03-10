//! Core layout types for the diegetic UI layout engine.

use bevy::color::Color;

/// Axis-specific sizing rule for a layout element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    /// Shrink-wrap to content, clamped to `[min, max]`.
    Fit {
        /// Minimum size in layout units.
        min: f32,
        /// Maximum size in layout units.
        max: f32,
    },
    /// Expand to fill available space, clamped to `[min, max]`.
    Grow {
        /// Minimum size in layout units.
        min: f32,
        /// Maximum size in layout units.
        max: f32,
    },
    /// Exact size in layout units.
    Fixed(f32),
    /// Fraction of parent's available space (0.0–1.0).
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
    /// `Fit` with no bounds.
    pub const FIT: Self = Self::Fit {
        min: 0.0,
        max: f32::MAX,
    };

    /// `Grow` with no bounds.
    pub const GROW: Self = Self::Grow {
        min: 0.0,
        max: f32::MAX,
    };

    /// `Fit` with a minimum.
    #[must_use]
    pub const fn fit_min(min: f32) -> Self {
        Self::Fit { min, max: f32::MAX }
    }

    /// `Fit` with a minimum and maximum.
    #[must_use]
    pub const fn fit_range(min: f32, max: f32) -> Self {
        Self::Fit { min, max }
    }

    /// `Grow` with a minimum.
    #[must_use]
    pub const fn grow_min(min: f32) -> Self {
        Self::Grow { min, max: f32::MAX }
    }

    /// `Grow` with a minimum and maximum.
    #[must_use]
    pub const fn grow_range(min: f32, max: f32) -> Self {
        Self::Grow { min, max }
    }

    /// `Fixed` size.
    #[must_use]
    pub const fn fixed(size: f32) -> Self {
        Self::Fixed(size)
    }

    /// `Percent` of parent (0.0–1.0).
    #[must_use]
    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

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
    pub const fn is_grow(&self) -> bool {
        matches!(self, Self::Grow { .. })
    }

    /// Returns `true` if this is a `Fit` variant.
    #[must_use]
    pub const fn is_fit(&self) -> bool {
        matches!(self, Self::Fit { .. })
    }

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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    /// Left padding in layout units.
    pub left: f32,
    /// Right padding in layout units.
    pub right: f32,
    /// Top padding in layout units.
    pub top: f32,
    /// Bottom padding in layout units.
    pub bottom: f32,
}

impl Padding {
    /// Uniform padding on all sides.
    #[must_use]
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    /// Symmetric padding: `x` for left/right, `y` for top/bottom.
    #[must_use]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            left: x,
            right: x,
            top: y,
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
    pub const fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    /// Total vertical padding (top + bottom).
    #[must_use]
    pub const fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Computed axis-aligned bounding box in layout coordinates (top-left origin).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoundingBox {
    /// X position of the top-left corner.
    pub x: f32,
    /// Y position of the top-left corner.
    pub y: f32,
    /// Width of the bounding box.
    pub width: f32,
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

/// Configuration for how text is measured and rendered.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextConfig {
    /// Font identifier (application-defined).
    pub font_id: u16,
    /// Font size in layout units.
    pub font_size: u16,
    /// Line height in layout units (0 = use `font_size`).
    pub line_height: u16,
    /// Letter spacing in layout units.
    pub letter_spacing: u16,
    /// Whether to wrap text at element boundaries.
    pub wrap: bool,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            font_id: 0,
            font_size: 16,
            line_height: 0,
            letter_spacing: 0,
            wrap: true,
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
            wrap: true,
        }
    }

    /// Sets the font id.
    pub const fn with_font_id(mut self, font_id: u16) -> Self {
        self.font_id = font_id;
        self
    }

    /// Sets the line height.
    pub const fn with_line_height(mut self, line_height: u16) -> Self {
        self.line_height = line_height;
        self
    }

    /// Sets the letter spacing.
    pub const fn with_letter_spacing(mut self, letter_spacing: u16) -> Self {
        self.letter_spacing = letter_spacing;
        self
    }

    /// Disables text wrapping.
    pub const fn no_wrap(mut self) -> Self {
        self.wrap = false;
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
    pub width: f32,
    /// Height in layout units.
    pub height: f32,
}

/// Border widths for an element.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Border {
    /// Left border width.
    pub left: f32,
    /// Right border width.
    pub right: f32,
    /// Top border width.
    pub top: f32,
    /// Bottom border width.
    pub bottom: f32,
    /// Color of the border.
    pub color: Color,
    /// Width of lines drawn between children (0 = none).
    pub between_children: f32,
}

impl Border {
    /// Creates a border with all widths at zero and default color.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
            color: Color::BLACK,
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

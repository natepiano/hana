//! Geometry primitives shared by layout and rendering.

use bevy::color::Color;

use super::Dimension;

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

    /// Returns the intersection of two bounding boxes, or `None` if they
    /// don't overlap. Both boxes use top-left origin coordinates.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.width).min(other.x + other.width);
        let y1 = (self.y + self.height).min(other.y + other.height);
        if x1 > x0 && y1 > y0 {
            Some(Self {
                x:      x0,
                y:      y0,
                width:  x1 - x0,
                height: y1 - y0,
            })
        } else {
            None
        }
    }
}

/// Per-corner radius for rounded rectangles.
///
/// Each corner can have an independent radius. Values use [`Dimension`],
/// so units like `Mm(3.0)` or `Pt(8.0)` work the same as `Padding` and
/// `Border`. A value of `0.0` produces a sharp corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerRadius {
    /// Top-left corner radius.
    pub top_left:     Dimension,
    /// Top-right corner radius.
    pub top_right:    Dimension,
    /// Bottom-right corner radius.
    pub bottom_right: Dimension,
    /// Bottom-left corner radius.
    pub bottom_left:  Dimension,
}

impl CornerRadius {
    /// All corners sharp (zero radius).
    pub const ZERO: Self = Self {
        top_left:     Dimension {
            value: 0.0,
            unit:  None,
        },
        top_right:    Dimension {
            value: 0.0,
            unit:  None,
        },
        bottom_right: Dimension {
            value: 0.0,
            unit:  None,
        },
        bottom_left:  Dimension {
            value: 0.0,
            unit:  None,
        },
    };

    /// Uniform radius on all corners.
    #[must_use]
    pub fn all(radius: impl Into<Dimension>) -> Self {
        let radius = radius.into();
        Self {
            top_left:     radius,
            top_right:    radius,
            bottom_right: radius,
            bottom_left:  radius,
        }
    }

    /// Per-corner radii: top-left, top-right, bottom-right, bottom-left.
    #[must_use]
    pub fn new(
        top_left: impl Into<Dimension>,
        top_right: impl Into<Dimension>,
        bottom_right: impl Into<Dimension>,
        bottom_left: impl Into<Dimension>,
    ) -> Self {
        Self {
            top_left:     top_left.into(),
            top_right:    top_right.into(),
            bottom_right: bottom_right.into(),
            bottom_left:  bottom_left.into(),
        }
    }

    /// Returns `true` if all corners are sharp (zero radius).
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.top_left.value == 0.0
            && self.top_right.value == 0.0
            && self.bottom_right.value == 0.0
            && self.bottom_left.value == 0.0
    }

    /// Returns the four resolved radii as an array: `[TL, TR, BR, BL]`.
    ///
    /// Values are in layout points (after unit conversion by `scaled()`).
    #[must_use]
    pub const fn to_array(&self) -> [f32; 4] {
        [
            self.top_left.value,
            self.top_right.value,
            self.bottom_right.value,
            self.bottom_left.value,
        ]
    }

    /// Returns the four radii converted to world meters.
    ///
    /// `default_meters_per_unit` is used for bare `f32` values (no unit).
    #[must_use]
    pub fn to_meters_array(&self, default_meters_per_unit: f32) -> [f32; 4] {
        [
            self.top_left.to_meters(default_meters_per_unit),
            self.top_right.to_meters(default_meters_per_unit),
            self.bottom_right.to_meters(default_meters_per_unit),
            self.bottom_left.to_meters(default_meters_per_unit),
        ]
    }

    /// Returns a copy with all radii converted to points using `scale`.
    #[must_use]
    pub fn resolved(&self, scale: f32) -> Self {
        Self {
            top_left:     Dimension {
                value: self.top_left.to_points(scale),
                unit:  None,
            },
            top_right:    Dimension {
                value: self.top_right.to_points(scale),
                unit:  None,
            },
            bottom_right: Dimension {
                value: self.bottom_right.to_points(scale),
                unit:  None,
            },
            bottom_left:  Dimension {
                value: self.bottom_left.to_points(scale),
                unit:  None,
            },
        }
    }
}

impl From<f32> for CornerRadius {
    fn from(radius: f32) -> Self { Self::all(radius) }
}

/// Border widths for an element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    /// Left border width.
    pub left:             Dimension,
    /// Right border width.
    pub right:            Dimension,
    /// Top border width.
    pub top:              Dimension,
    /// Bottom border width.
    pub bottom:           Dimension,
    /// Color of the border.
    pub color:            Color,
    /// Width of lines drawn between children (0 = none).
    pub between_children: Dimension,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            left:             Dimension {
                value: 0.0,
                unit:  None,
            },
            right:            Dimension {
                value: 0.0,
                unit:  None,
            },
            top:              Dimension {
                value: 0.0,
                unit:  None,
            },
            bottom:           Dimension {
                value: 0.0,
                unit:  None,
            },
            color:            Color::BLACK,
            between_children: Dimension {
                value: 0.0,
                unit:  None,
            },
        }
    }
}

impl Border {
    /// Creates a border with all widths at zero and default color.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            left:             Dimension {
                value: 0.0,
                unit:  None,
            },
            right:            Dimension {
                value: 0.0,
                unit:  None,
            },
            top:              Dimension {
                value: 0.0,
                unit:  None,
            },
            bottom:           Dimension {
                value: 0.0,
                unit:  None,
            },
            color:            Color::BLACK,
            between_children: Dimension {
                value: 0.0,
                unit:  None,
            },
        }
    }

    /// Uniform border on all sides.
    #[must_use]
    pub fn all(width: impl Into<Dimension>, color: Color) -> Self {
        let width = width.into();
        Self {
            left: width,
            right: width,
            top: width,
            bottom: width,
            color,
            between_children: Dimension {
                value: 0.0,
                unit:  None,
            },
        }
    }

    /// Sets the left border width.
    #[must_use]
    pub fn left(mut self, width: impl Into<Dimension>) -> Self {
        self.left = width.into();
        self
    }

    /// Sets the right border width.
    #[must_use]
    pub fn right(mut self, width: impl Into<Dimension>) -> Self {
        self.right = width.into();
        self
    }

    /// Sets the top border width.
    #[must_use]
    pub fn top(mut self, width: impl Into<Dimension>) -> Self {
        self.top = width.into();
        self
    }

    /// Sets the bottom border width.
    #[must_use]
    pub fn bottom(mut self, width: impl Into<Dimension>) -> Self {
        self.bottom = width.into();
        self
    }

    /// Sets the border color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Total horizontal border width (left + right) in resolved units.
    #[must_use]
    pub const fn horizontal(&self) -> f32 { self.left.value + self.right.value }

    /// Total vertical border width (top + bottom) in resolved units.
    #[must_use]
    pub const fn vertical(&self) -> f32 { self.top.value + self.bottom.value }

    /// Sets the width of lines drawn between children.
    #[must_use]
    pub fn between_children(mut self, width: impl Into<Dimension>) -> Self {
        self.between_children = width.into();
        self
    }

    /// Resolves all dimensions to points and returns a copy with plain values.
    ///
    /// Dimensions with an explicit unit convert via `unit.to_points()`.
    /// Dimensions without a unit (bare `f32`) use `default_scale`.
    /// Color is preserved.
    #[must_use]
    pub fn resolved(self, default_scale: f32) -> Self {
        Self {
            left:             Dimension {
                value: self.left.to_points(default_scale),
                unit:  None,
            },
            right:            Dimension {
                value: self.right.to_points(default_scale),
                unit:  None,
            },
            top:              Dimension {
                value: self.top.to_points(default_scale),
                unit:  None,
            },
            bottom:           Dimension {
                value: self.bottom.to_points(default_scale),
                unit:  None,
            },
            color:            self.color,
            between_children: Dimension {
                value: self.between_children.to_points(default_scale),
                unit:  None,
            },
        }
    }
}

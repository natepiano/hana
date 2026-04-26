use bevy::color::Color;

/// Visual style for arrow end caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowStyle {
    /// Open chevron made from two line segments.
    Open,
    /// Solid triangular arrowhead with a sharp point.
    Solid,
}

/// Arrow-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ArrowCap {
    pub(super) style:  ArrowStyle,
    pub(super) length: Option<f32>,
    pub(super) width:  Option<f32>,
    pub(super) color:  Option<Color>,
}

impl ArrowCap {
    /// Creates a default arrow cap.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            style:  ArrowStyle::Open,
            length: None,
            width:  None,
            color:  None,
        }
    }

    /// Uses the open chevron arrow style.
    #[must_use]
    pub(super) const fn open(mut self) -> Self {
        self.style = ArrowStyle::Open;
        self
    }

    /// Uses the solid triangular arrow style.
    #[must_use]
    pub(super) const fn solid(mut self) -> Self {
        self.style = ArrowStyle::Solid;
        self
    }

    /// Sets the cap length along the line direction.
    #[must_use]
    pub(super) const fn length(mut self, length: f32) -> Self {
        self.length = Some(length);
        self
    }

    /// Sets the cap width across the line direction.
    #[must_use]
    pub(super) const fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub(super) const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Circle-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CircleCap {
    pub(super) radius: Option<f32>,
    pub(super) color:  Option<Color>,
}

impl CircleCap {
    /// Creates a default circle cap.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            radius: None,
            color:  None,
        }
    }

    /// Sets the circle radius.
    #[must_use]
    pub(super) const fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub(super) const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Square-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SquareCap {
    pub(super) size:  Option<f32>,
    pub(super) color: Option<Color>,
}

impl SquareCap {
    /// Creates a default square cap.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            size:  None,
            color: None,
        }
    }

    /// Sets the full square size.
    #[must_use]
    pub(super) const fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub(super) const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Diamond-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DiamondCap {
    pub(super) width:  Option<f32>,
    pub(super) height: Option<f32>,
    pub(super) color:  Option<Color>,
}

impl DiamondCap {
    /// Creates a default diamond cap.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            width:  None,
            height: None,
            color:  None,
        }
    }

    /// Sets the full diamond width.
    #[must_use]
    pub(super) const fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Sets the full diamond height.
    #[must_use]
    pub(super) const fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub(super) const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Decoration that can appear at either end of a [`CalloutLine`](super::CalloutLine).
#[derive(Clone, Copy, Debug, PartialEq)]
#[expect(
    private_interfaces,
    reason = "variant payload structs are crate-internal; users configure caps via CalloutCap builder methods"
)]
pub enum CalloutCap {
    /// No end cap.
    None,
    /// Arrow end cap with the given style.
    Arrow(ArrowCap),
    /// Filled circular cap.
    Circle(CircleCap),
    /// Filled square cap.
    Square(SquareCap),
    /// Filled diamond cap.
    Diamond(DiamondCap),
}

impl CalloutCap {
    /// Creates an arrow cap with default open styling.
    #[must_use]
    pub const fn arrow() -> Self { Self::Arrow(ArrowCap::new()) }

    /// Creates a circular cap.
    #[must_use]
    pub const fn circle() -> Self { Self::Circle(CircleCap::new()) }

    /// Creates a square cap.
    #[must_use]
    pub const fn square() -> Self { Self::Square(SquareCap::new()) }

    /// Creates a diamond cap.
    #[must_use]
    pub const fn diamond() -> Self { Self::Diamond(DiamondCap::new()) }

    /// Sets an arrow cap to the open chevron style.
    #[must_use]
    pub const fn open(self) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.open()),
            other => other,
        }
    }

    /// Sets an arrow cap to the solid triangular style.
    #[must_use]
    pub const fn solid(self) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.solid()),
            other => other,
        }
    }

    /// Sets the cap length along the line direction.
    #[must_use]
    pub const fn length(self, length: f32) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.length(length)),
            other => other,
        }
    }

    /// Sets the cap width across the line direction.
    #[must_use]
    pub const fn width(self, width: f32) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.width(width)),
            Self::Diamond(cap) => Self::Diamond(cap.width(width)),
            other => other,
        }
    }

    /// Sets the cap height for shapes that support an explicit height.
    #[must_use]
    pub const fn height(self, height: f32) -> Self {
        match self {
            Self::Diamond(cap) => Self::Diamond(cap.height(height)),
            other => other,
        }
    }

    /// Sets the cap radius for circular caps.
    #[must_use]
    pub const fn radius(self, radius: f32) -> Self {
        match self {
            Self::Circle(cap) => Self::Circle(cap.radius(radius)),
            other => other,
        }
    }

    /// Sets the cap size for square caps.
    #[must_use]
    pub const fn size(self, size: f32) -> Self {
        match self {
            Self::Square(cap) => Self::Square(cap.size(size)),
            other => other,
        }
    }

    /// Overrides the cap color. Defaults to the callout line color.
    #[must_use]
    pub const fn color(self, color: Color) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.color(color)),
            Self::Circle(cap) => Self::Circle(cap.color(color)),
            Self::Square(cap) => Self::Square(cap.color(color)),
            Self::Diamond(cap) => Self::Diamond(cap.color(color)),
            Self::None => Self::None,
        }
    }

    pub(super) fn shaft_inset(self, default_size: f32) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Arrow(cap) => cap.length.unwrap_or(default_size),
            Self::Circle(cap) => cap.radius.unwrap_or(default_size * 0.5),
            Self::Square(cap) => cap.size.unwrap_or(default_size) * 0.5,
            Self::Diamond(cap) => cap.width.unwrap_or(default_size) * 0.5,
        }
    }

    pub(super) fn resolved_color(self, fallback: Color) -> Color {
        match self {
            Self::None => fallback,
            Self::Arrow(cap) => cap.color.unwrap_or(fallback),
            Self::Circle(cap) => cap.color.unwrap_or(fallback),
            Self::Square(cap) => cap.color.unwrap_or(fallback),
            Self::Diamond(cap) => cap.color.unwrap_or(fallback),
        }
    }
}

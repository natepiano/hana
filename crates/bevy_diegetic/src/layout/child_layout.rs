use super::AlignX;
use super::AlignY;
use super::Dimension;
use super::Direction;

/// Internal child layout mode stored on [`Element`](super::element::Element).
#[derive(Clone, Copy, Debug)]
pub(crate) enum ChildLayout {
    /// Children are laid out from left to right.
    Row {
        /// Main-axis spacing between adjacent children.
        gap:     Dimension,
        /// Horizontal child alignment.
        align_x: AlignX,
        /// Vertical child alignment.
        align_y: AlignY,
    },
    /// Children are laid out from top to bottom.
    Column {
        /// Main-axis spacing between adjacent children.
        gap:     Dimension,
        /// Horizontal child alignment.
        align_x: AlignX,
        /// Vertical child alignment.
        align_y: AlignY,
    },
}

impl ChildLayout {
    /// Builds internal child layout storage from the compatibility builder
    /// fields on [`El`](super::builder::El).
    #[must_use]
    pub(crate) const fn for_direction(
        direction: Direction,
        gap: Dimension,
        align_x: AlignX,
        align_y: AlignY,
    ) -> Self {
        match direction {
            Direction::LeftToRight => Self::Row {
                gap,
                align_x,
                align_y,
            },
            Direction::TopToBottom => Self::Column {
                gap,
                align_x,
                align_y,
            },
        }
    }

    /// Returns the compatibility direction represented by this layout.
    #[must_use]
    pub(crate) const fn direction(&self) -> Direction {
        match self {
            Self::Row { .. } => Direction::LeftToRight,
            Self::Column { .. } => Direction::TopToBottom,
        }
    }

    /// Returns the main-axis gap between adjacent children.
    #[must_use]
    pub(crate) const fn gap(&self) -> Dimension {
        match self {
            Self::Row { gap, .. } | Self::Column { gap, .. } => *gap,
        }
    }

    /// Returns horizontal child alignment.
    #[must_use]
    pub(crate) const fn align_x(&self) -> AlignX {
        match self {
            Self::Row { align_x, .. } | Self::Column { align_x, .. } => *align_x,
        }
    }

    /// Returns vertical child alignment.
    #[must_use]
    pub(crate) const fn align_y(&self) -> AlignY {
        match self {
            Self::Row { align_y, .. } | Self::Column { align_y, .. } => *align_y,
        }
    }

    /// Returns whether children are laid out from left to right.
    #[must_use]
    pub(crate) const fn is_row(&self) -> bool { matches!(self, Self::Row { .. }) }

    /// Returns whether children are laid out from top to bottom.
    #[must_use]
    pub(crate) const fn is_column(&self) -> bool { matches!(self, Self::Column { .. }) }

    /// Resolves the gap to points while preserving direction and alignment.
    #[must_use]
    pub(crate) fn to_points(self, layout_scale: f32) -> Self {
        match self {
            Self::Row {
                gap,
                align_x,
                align_y,
            } => Self::Row {
                gap: Dimension {
                    value: gap.to_points(layout_scale),
                    unit:  None,
                },
                align_x,
                align_y,
            },
            Self::Column {
                gap,
                align_x,
                align_y,
            } => Self::Column {
                gap: Dimension {
                    value: gap.to_points(layout_scale),
                    unit:  None,
                },
                align_x,
                align_y,
            },
        }
    }
}

impl Default for ChildLayout {
    fn default() -> Self {
        Self::Row {
            gap:     Dimension::default(),
            align_x: AlignX::default(),
            align_y: AlignY::default(),
        }
    }
}

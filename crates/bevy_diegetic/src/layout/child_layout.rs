use super::AlignX;
use super::AlignY;
use super::ChildDivider;
use super::Dimension;

/// Internal child layout mode stored on [`Element`](super::element::Element).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ChildLayout {
    /// Children are laid out from left to right.
    Row {
        /// Main-axis spacing between adjacent children.
        gap:     Dimension,
        /// Horizontal child alignment.
        align_x: AlignX,
        /// Vertical child alignment.
        align_y: AlignY,
        /// Optional separator between adjacent child slots.
        divider: Option<ChildDivider>,
    },
    /// Children are laid out from top to bottom.
    Column {
        /// Main-axis spacing between adjacent children.
        gap:     Dimension,
        /// Horizontal child alignment.
        align_x: AlignX,
        /// Vertical child alignment.
        align_y: AlignY,
        /// Optional separator between adjacent child slots.
        divider: Option<ChildDivider>,
    },
}

impl ChildLayout {
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

    /// Returns the row or column child divider.
    #[must_use]
    pub(crate) const fn divider(&self) -> Option<ChildDivider> {
        match self {
            Self::Row { divider, .. } | Self::Column { divider, .. } => *divider,
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
                divider,
            } => Self::Row {
                gap: Dimension {
                    value: gap.to_points(layout_scale),
                    unit:  None,
                },
                align_x,
                align_y,
                divider: divider.map(|divider| divider.to_points(layout_scale)),
            },
            Self::Column {
                gap,
                align_x,
                align_y,
                divider,
            } => Self::Column {
                gap: Dimension {
                    value: gap.to_points(layout_scale),
                    unit:  None,
                },
                align_x,
                align_y,
                divider: divider.map(|divider| divider.to_points(layout_scale)),
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
            divider: None,
        }
    }
}

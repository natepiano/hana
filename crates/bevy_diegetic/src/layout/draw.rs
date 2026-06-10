//! Paint-only visual layers authored on layout elements.

use super::PanelLine;

/// Paint-only primitives owned by one layout element.
///
/// `PanelDraw` does not participate in intrinsic measurement. Later layout
/// phases resolve its coordinates against the owning element's computed box.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelDraw {
    kind:     PanelDrawKind,
    overflow: DrawOverflow,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum PanelDrawKind {
    Lines(Vec<PanelLine>),
}

/// Whether a `PanelDraw` is clipped to the owning element.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DrawOverflow {
    /// Clip the draw output to the owning element's box.
    #[default]
    Clipped,
    /// Allow the draw output to overflow the owning element's box.
    Visible,
}

impl PanelDraw {
    /// Creates a line draw layer.
    #[must_use]
    pub fn lines(lines: impl IntoIterator<Item = PanelLine>) -> Self {
        Self {
            kind:     PanelDrawKind::Lines(lines.into_iter().collect()),
            overflow: DrawOverflow::Clipped,
        }
    }

    /// Sets how this draw layer handles output outside the owning element.
    #[must_use]
    pub const fn overflow(mut self, overflow: DrawOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// Returns this draw layer's overflow policy.
    #[must_use]
    pub const fn overflow_policy(&self) -> DrawOverflow { self.overflow }

    /// Returns the lines stored by this draw layer.
    #[must_use]
    pub fn lines_ref(&self) -> &[PanelLine] {
        match &self.kind {
            PanelDrawKind::Lines(lines) => lines,
        }
    }

    pub(crate) fn scaled(&self, default_scale: f32) -> Self {
        Self {
            kind:     self.kind.scaled(default_scale),
            overflow: self.overflow,
        }
    }
}

impl Default for PanelDraw {
    fn default() -> Self { Self::lines([]) }
}

impl PanelDrawKind {
    fn scaled(&self, default_scale: f32) -> Self {
        match self {
            Self::Lines(lines) => Self::Lines(
                lines
                    .iter()
                    .map(|line| line.scaled(default_scale))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DrawOverflow;
    use super::PanelDraw;
    use crate::PanelLine;
    use crate::PanelPoint;

    #[test]
    fn line_draw_defaults_to_clipped_overflow() {
        let panel_draw = PanelDraw::lines([PanelLine::new(
            PanelPoint::new(0.0, 0.0),
            PanelPoint::new(1.0, 1.0),
        )]);

        assert_eq!(panel_draw.overflow_policy(), DrawOverflow::Clipped);
        assert_eq!(panel_draw.lines_ref().len(), 1);
    }

    #[test]
    fn line_draw_accepts_visible_overflow() {
        let panel_draw = PanelDraw::default().overflow(DrawOverflow::Visible);

        assert_eq!(panel_draw.overflow_policy(), DrawOverflow::Visible);
        assert!(panel_draw.lines_ref().is_empty());
    }
}

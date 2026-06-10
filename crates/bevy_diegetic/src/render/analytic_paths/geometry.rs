//! Renderer-owned quadratic path geometry.

use bevy::math::Vec2;

/// A single quadratic Bezier segment in path design-space units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct QuadraticSegment {
    /// Segment start point.
    pub start:   Vec2,
    /// Quadratic control point.
    pub control: Vec2,
    /// Segment end point.
    pub end:     Vec2,
}

/// Axis-aligned path bounds in design-space units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    /// Minimum corner.
    pub min: Vec2,
    /// Maximum corner.
    pub max: Vec2,
}

impl Bounds {
    /// Width of the bounds.
    #[must_use]
    pub fn width(self) -> f32 { self.max.x - self.min.x }

    /// Height of the bounds.
    #[must_use]
    pub fn height(self) -> f32 { self.max.y - self.min.y }
}

/// One closed analytic path contour expressed as quadratic segments.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathContour {
    /// Quadratic segments in contour order.
    pub segments:    Vec<QuadraticSegment>,
    /// Narrowest stroke of this contour in design-space units, packed per
    /// curve for hairline dilation. `0.0` disables — text glyph contours
    /// stay undilated.
    pub min_feature: f32,
}

/// Renderer-owned quadratic outline representation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathOutline {
    /// Outline bounds in local design-space units.
    pub bounds:   Bounds,
    /// Closed contours that make up the filled path.
    pub contours: Vec<PathContour>,
}

impl PathOutline {
    /// Narrowest dilating stroke across all contours, forwarded to
    /// `GlyphRecord::min_feature` so the shader sizes its distance scan for
    /// the largest dilation in the path. `0.0` when no contour dilates.
    #[must_use]
    pub fn min_feature(&self) -> f32 {
        let narrowest = self
            .contours
            .iter()
            .map(|contour| contour.min_feature)
            .filter(|min_feature| *min_feature > 0.0)
            .fold(f32::INFINITY, f32::min);
        if narrowest.is_finite() {
            narrowest
        } else {
            0.0
        }
    }
}

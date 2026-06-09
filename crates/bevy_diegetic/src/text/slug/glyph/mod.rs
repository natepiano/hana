#[cfg(test)]
mod coverage_probe;
mod outline;

pub(crate) use outline::Glyph;
pub(super) use outline::OutlineError;
pub(super) use outline::font_glyph_id_has_visible_outline;
pub(super) use outline::load_glyph_by_id_from_face;

pub(crate) use crate::render::Bounds;
pub(crate) use crate::render::PathContour;
pub(crate) use crate::render::PathOutline;

/// Text bridge from font-extracted glyphs to renderer-owned analytic paths.
#[must_use]
pub(super) fn build_packed_glyph(glyph: Glyph, band_count: usize) -> crate::render::GlyphOutline {
    let path = PathOutline {
        bounds:   glyph.bounds,
        contours: glyph
            .contours
            .into_iter()
            .map(|contour| PathContour {
                segments: contour.segments,
            })
            .collect(),
    };
    crate::render::build_packed_path(path, band_count)
}

#[cfg(test)]
mod coverage_probe;
mod outline;
mod packing;

pub(super) use outline::Bounds;
pub(super) use outline::Glyph;
pub(super) use outline::OutlineError;
pub(super) use outline::QuadraticSegment;
pub(super) use outline::font_glyph_id_has_visible_outline;
pub(super) use outline::load_glyph_by_id_from_face;
pub(super) use packing::BandRecord;
pub(super) use packing::CurveRecord;
pub(crate) use packing::DEFAULT_BAND_COUNT;
pub(super) use packing::GlyphOutline;
pub(super) use packing::GlyphRecord;
pub(super) use packing::build_packed_glyph;

use super::geometry::SlugGlyph;
use super::geometry::SlugOutlineError;
use super::geometry::load_glyph;

/// Deterministic quadratic fixtures used by the first Slug shader spike.
pub const FIXTURE_TEXT: &str = "Typography";

/// Loads the deterministic fixture glyphs from `font_data`.
pub fn load_fixture_glyphs(font_data: &[u8]) -> Result<Vec<SlugGlyph>, SlugOutlineError> {
    FIXTURE_TEXT
        .chars()
        .map(|character| load_glyph(font_data, character))
        .collect()
}

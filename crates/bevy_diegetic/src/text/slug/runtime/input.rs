use crate::layout::ShapedGlyph;
use crate::text::Font;

#[derive(Clone, Copy)]
pub(crate) struct PositionedGlyph<'a> {
    pub glyph:            &'a ShapedGlyph,
    pub font:             &'a Font,
    pub collection_index: u32,
}

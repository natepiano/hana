use bevy::math::UVec4;
use bevy::math::Vec4;
use bevy::render::render_resource::ShaderType;
use bevy_kana::ToF32;
use bevy_kana::ToU32;

use super::geometry::QuadraticSegment;
use super::geometry::SlugBounds;
use super::geometry::SlugGlyph;

/// Default number of horizontal bands in the first Slug feasibility packing.
pub const DEFAULT_BAND_COUNT: usize = 32;

/// GPU curve record for a quadratic Bezier segment.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct SlugCurveRecord {
    /// Segment start point in `.xy`, quadratic control point in `.zw`.
    pub start_control: Vec4,
    /// Segment end point in `.xy`; `.zw` is reserved for later packing.
    pub end:           Vec4,
}

impl From<&QuadraticSegment> for SlugCurveRecord {
    fn from(segment: &QuadraticSegment) -> Self {
        Self {
            start_control: Vec4::new(
                segment.start.x,
                segment.start.y,
                segment.control.x,
                segment.control.y,
            ),
            end:           Vec4::new(segment.end.x, segment.end.y, 0.0, 0.0),
        }
    }
}

/// GPU band record pointing at a contiguous curve range.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct SlugBandRecord {
    /// First curve record for this band.
    pub start: u32,
    /// Number of curve records for this band.
    pub count: u32,
    /// Lower band edge in font design-space units.
    pub y_min: f32,
    /// Upper band edge in font design-space units.
    pub y_max: f32,
}

/// GPU glyph record for one unique glyph in a packed Slug text run.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct SlugGlyphRecord {
    /// Bounds minimum in `.xy`, bounds size in `.zw`, in font design-space units.
    pub bounds_min_size: Vec4,
    /// First band in `.x`, number of bands in `.y`; `.zw` are reserved.
    pub band_range:      UVec4,
}

impl SlugGlyphRecord {
    /// Creates a glyph record that points into the combined run band buffer.
    #[must_use]
    pub fn new(bounds: SlugBounds, band_start: u32, band_count: u32) -> Self {
        Self {
            bounds_min_size: Vec4::new(bounds.min.x, bounds.min.y, bounds.width(), bounds.height()),
            band_range:      UVec4::new(band_start, band_count, 0, 0),
        }
    }
}

/// One glyph's reference-like curve and band data for the shader spike.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugPackedGlyph {
    glyph:             SlugGlyph,
    curves:            Vec<SlugCurveRecord>,
    bands:             Vec<SlugBandRecord>,
    outline_segments:  usize,
    duplicated_curves: usize,
}

impl SlugPackedGlyph {
    /// Source glyph for this packed data.
    #[must_use]
    pub const fn glyph(&self) -> &SlugGlyph { &self.glyph }

    /// Glyph bounds in font design-space units.
    #[must_use]
    pub const fn bounds(&self) -> SlugBounds { self.glyph.bounds }

    /// Band-packed curve records.
    #[must_use]
    pub fn curves(&self) -> &[SlugCurveRecord] { &self.curves }

    /// Band records.
    #[must_use]
    pub fn bands(&self) -> &[SlugBandRecord] { &self.bands }

    /// Number of original outline segments before band duplication.
    #[must_use]
    pub const fn outline_segments(&self) -> usize { self.outline_segments }

    /// Number of curve records after per-band duplication.
    #[must_use]
    pub const fn duplicated_curves(&self) -> usize { self.duplicated_curves }

    /// Approximate bytes used by band-packed curve records before final Slug packing.
    #[must_use]
    pub const fn curve_bytes(&self) -> usize {
        self.curves.len() * std::mem::size_of::<SlugCurveRecord>()
    }

    /// Approximate bytes used by band records before final Slug packing.
    #[must_use]
    pub const fn band_bytes(&self) -> usize {
        self.bands.len() * std::mem::size_of::<SlugBandRecord>()
    }
}

/// Builds horizontal band data for one quadratic glyph.
#[must_use]
pub fn build_packed_glyph(glyph: SlugGlyph, band_count: usize) -> SlugPackedGlyph {
    let band_count = band_count.max(1);
    let outline_segments = glyph.segment_count();
    let mut curves = Vec::new();
    let mut bands = Vec::with_capacity(band_count);
    let bounds = glyph.bounds;
    let band_height = bounds.height().max(1.0) / band_count.to_f32();

    for band_index in 0..band_count {
        let y_min = bounds.min.y + band_height * band_index.to_f32();
        let y_max = if band_index + 1 == band_count {
            bounds.max.y
        } else {
            y_min + band_height
        };
        let start = curves.len().to_u32();
        append_band_curves(&glyph, y_min, y_max, &mut curves);
        bands.push(SlugBandRecord {
            start,
            count: curves.len().to_u32() - start,
            y_min,
            y_max,
        });
    }

    let duplicated_curves = curves.len();
    SlugPackedGlyph {
        glyph,
        curves,
        bands,
        outline_segments,
        duplicated_curves,
    }
}

fn append_band_curves(
    glyph: &SlugGlyph,
    y_min: f32,
    y_max: f32,
    curves: &mut Vec<SlugCurveRecord>,
) {
    for contour in &glyph.contours {
        curves.extend(
            contour
                .segments
                .iter()
                .filter(|segment| overlaps_y_band(segment, y_min, y_max))
                .map(SlugCurveRecord::from),
        );
    }
}

fn overlaps_y_band(segment: &QuadraticSegment, y_min: f32, y_max: f32) -> bool {
    let segment_min = segment.start.y.min(segment.control.y).min(segment.end.y);
    let segment_max = segment.start.y.max(segment.control.y).max(segment.end.y);
    segment_min <= y_max && segment_max >= y_min
}

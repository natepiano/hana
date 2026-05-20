use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use bevy::math::Vec2;
use ttf_parser::Face;
use ttf_parser::GlyphId;
use ttf_parser::OutlineBuilder;

/// A single quadratic Bezier segment in font design-space units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuadraticSegment {
    /// Segment start point.
    pub start:   Vec2,
    /// Quadratic control point.
    pub control: Vec2,
    /// Segment end point.
    pub end:     Vec2,
}

/// Axis-aligned glyph bounds in font design-space units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlugBounds {
    /// Minimum corner.
    pub min: Vec2,
    /// Maximum corner.
    pub max: Vec2,
}

impl SlugBounds {
    /// Width of the bounds.
    #[must_use]
    pub fn width(self) -> f32 { self.max.x - self.min.x }

    /// Height of the bounds.
    #[must_use]
    pub fn height(self) -> f32 { self.max.y - self.min.y }
}

/// One closed glyph contour expressed as quadratic segments.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugContour {
    /// Quadratic segments for this contour.
    pub segments: Vec<QuadraticSegment>,
}

/// Quadratic-only Slug feasibility representation for one glyph.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugGlyph {
    /// Unicode scalar used to request this glyph.
    pub character: char,
    /// Font glyph ID selected for `character`.
    pub glyph_id:  u16,
    /// Glyph bounds in font design-space units.
    pub bounds:    SlugBounds,
    /// Quadratic contours extracted from the font.
    pub contours:  Vec<SlugContour>,
}

impl SlugGlyph {
    /// Number of contours in this glyph outline.
    #[must_use]
    pub const fn contour_count(&self) -> usize { self.contours.len() }

    /// Number of quadratic segments in this glyph outline.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.contours
            .iter()
            .map(|contour| contour.segments.len())
            .sum()
    }

    /// Approximate unpacked curve bytes before Slug-specific packing.
    #[must_use]
    pub fn curve_bytes(&self) -> usize {
        self.segment_count() * std::mem::size_of::<QuadraticSegment>()
    }
}

/// Errors produced by the Slug feasibility outline loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlugOutlineError {
    /// Font bytes could not be parsed.
    InvalidFont,
    /// The requested character has no glyph in the font.
    MissingGlyph(char),
    /// The requested glyph ID has no outline entry in the font.
    MissingGlyphId(u16),
    /// The glyph has no outline.
    MissingOutline(char),
    /// The glyph uses cubic curves, which the first spike rejects.
    CubicOutline {
        character:      char,
        cubic_segments: usize,
    },
}

impl Display for SlugOutlineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFont => formatter.write_str("font bytes could not be parsed"),
            Self::MissingGlyph(character) => {
                write!(formatter, "font does not contain glyph for '{character}'")
            },
            Self::MissingGlyphId(glyph_id) => {
                write!(formatter, "font does not contain glyph id {glyph_id}")
            },
            Self::MissingOutline(character) => {
                write!(formatter, "glyph for '{character}' has no outline")
            },
            Self::CubicOutline {
                character,
                cubic_segments,
            } => write!(
                formatter,
                "glyph for '{character}' uses {cubic_segments} cubic outline segments"
            ),
        }
    }
}

impl Error for SlugOutlineError {}

/// Loads one glyph from `font_data` and converts lines/quadratics into the
/// first Slug feasibility representation.
///
/// This intentionally rejects cubic outlines so the first spike stays
/// TrueType/quadratic-only.
pub fn load_glyph(font_data: &[u8], character: char) -> Result<SlugGlyph, SlugOutlineError> {
    let face = Face::parse(font_data, 0).map_err(|_| SlugOutlineError::InvalidFont)?;
    let glyph_id = face
        .glyph_index(character)
        .ok_or(SlugOutlineError::MissingGlyph(character))?;
    load_glyph_from_face(&face, glyph_id, character)
}

/// Loads one glyph by resolved font glyph ID.
///
/// Parley shaping already resolves text into glyph IDs. This path keeps the
/// Slug preprocessor from repeating Unicode character lookup.
pub fn load_glyph_by_id(
    font_data: &[u8],
    glyph_id: u16,
    character: char,
) -> Result<SlugGlyph, SlugOutlineError> {
    let face = Face::parse(font_data, 0).map_err(|_| SlugOutlineError::InvalidFont)?;
    let glyph_id = GlyphId(glyph_id);
    load_glyph_from_face(&face, glyph_id, character)
}

fn load_glyph_from_face(
    face: &Face<'_>,
    glyph_id: GlyphId,
    character: char,
) -> Result<SlugGlyph, SlugOutlineError> {
    let mut builder = QuadraticOutlineBuilder::new(character);
    let rect = face
        .outline_glyph(glyph_id, &mut builder)
        .ok_or(SlugOutlineError::MissingGlyphId(glyph_id.0))?;
    builder.finish(
        glyph_id,
        SlugBounds {
            min: Vec2::new(f32::from(rect.x_min), f32::from(rect.y_min)),
            max: Vec2::new(f32::from(rect.x_max), f32::from(rect.y_max)),
        },
    )
}

#[derive(Debug)]
struct QuadraticOutlineBuilder {
    character:      char,
    contours:       Vec<SlugContour>,
    current:        Vec<QuadraticSegment>,
    contour_start:  Option<Vec2>,
    current_point:  Option<Vec2>,
    cubic_segments: usize,
}

impl QuadraticOutlineBuilder {
    const fn new(character: char) -> Self {
        Self {
            character,
            contours: Vec::new(),
            current: Vec::new(),
            contour_start: None,
            current_point: None,
            cubic_segments: 0,
        }
    }

    fn finish(
        mut self,
        glyph_id: GlyphId,
        bounds: SlugBounds,
    ) -> Result<SlugGlyph, SlugOutlineError> {
        self.finish_contour();
        if self.cubic_segments > 0 {
            return Err(SlugOutlineError::CubicOutline {
                character:      self.character,
                cubic_segments: self.cubic_segments,
            });
        }
        Ok(SlugGlyph {
            character: self.character,
            glyph_id: glyph_id.0,
            bounds,
            contours: self.contours,
        })
    }

    fn finish_contour(&mut self) {
        if self.current.is_empty() {
            self.contour_start = None;
            self.current_point = None;
            return;
        }
        self.contours.push(SlugContour {
            segments: std::mem::take(&mut self.current),
        });
        self.contour_start = None;
        self.current_point = None;
    }

    fn push_line(&mut self, end: Vec2) {
        if let Some(start) = self.current_point {
            let control = start.midpoint(end);
            self.current.push(QuadraticSegment {
                start,
                control,
                end,
            });
        }
        self.current_point = Some(end);
    }

    fn push_quadratic(&mut self, control: Vec2, end: Vec2) {
        if let Some(start) = self.current_point {
            self.current.push(QuadraticSegment {
                start,
                control,
                end,
            });
        }
        self.current_point = Some(end);
    }
}

impl OutlineBuilder for QuadraticOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish_contour();
        let point = Vec2::new(x, y);
        self.contour_start = Some(point);
        self.current_point = Some(point);
    }

    fn line_to(&mut self, x: f32, y: f32) { self.push_line(Vec2::new(x, y)); }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.push_quadratic(Vec2::new(x1, y1), Vec2::new(x, y));
    }

    fn curve_to(&mut self, _x1: f32, _y1: f32, _x2: f32, _y2: f32, _x: f32, _y: f32) {
        self.cubic_segments += 1;
    }

    fn close(&mut self) {
        if let (Some(start), Some(current_point)) = (self.contour_start, self.current_point)
            && current_point != start
        {
            self.push_line(start);
        }
        self.finish_contour();
    }
}

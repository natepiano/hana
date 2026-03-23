//! Font-level typographic metrics, parsed from font tables via `ttf_parser`.
//!
//! [`Font`] pre-parses raw design-unit metrics at creation time. Call
//! [`Font::metrics`] to get scaled [`FontMetrics`] at any font size —
//! pure arithmetic, no re-parsing.

use std::sync::Arc;

use bevy::asset::Asset;
use bevy::reflect::TypePath;
use ttf_parser::Face;

/// Pre-parsed font with design-unit metrics.
///
/// Created via [`Font::from_bytes`]. All raw values are in the font's
/// design units (`units_per_em`). Call [`Font::metrics`] to get values
/// scaled to a specific font size.
///
/// Also a Bevy [`Asset`] — load `.ttf`/`.otf` files via `AssetServer`:
///
/// ```ignore
/// let handle: Handle<Font> = asset_server.load("fonts/MyFont.ttf");
/// ```
///
/// When the asset loads, the plugin automatically registers it with
/// [`FontRegistry`](crate::FontRegistry) and fires a
/// [`FontRegistered`](crate::FontRegistered) event.
#[derive(Asset, TypePath)]
pub struct Font {
    name:                    String,
    units_per_em:            u16,
    raw_ascent:              i16,
    raw_descent:             i16,
    raw_line_gap:            i16,
    raw_cap_height:          i16,
    raw_x_height:            i16,
    raw_italic_angle:        f32,
    raw_underline_position:  Option<i16>,
    raw_underline_thickness: Option<i16>,
    raw_strikeout_position:  Option<i16>,
    raw_strikeout_thickness: Option<i16>,
    /// Raw font bytes, retained for MSDF rasterization and per-glyph queries.
    data:                    Arc<[u8]>,
}

/// Font-level typographic metrics, scaled to a specific font size.
///
/// Returned by [`Font::metrics`]. All distance values are in layout units,
/// scaled from the font's design units by `font_size / units_per_em`.
///
/// Vertical distances are positive in both directions from the baseline:
/// - `ascent` extends **above** the baseline.
/// - `descent` extends **below** the baseline.
pub struct FontMetrics {
    /// Distance from the baseline to the ascender line. This is the font's
    /// full ascender — it includes room for accented characters like `Â` and
    /// `É`, so it is always >= [`cap_height`](Self::cap_height).
    pub ascent:              f32,
    /// Distance from the baseline to the descender line (positive = below
    /// baseline). Covers the lowest descenders like `p`, `g`, `y`.
    pub descent:             f32,
    /// Font-recommended inter-line spacing, also called "leading" in
    /// traditional typography. In parley's half-leading model this value
    /// is split in half and absorbed into the `top` and `bottom` of
    /// each line box — see [`LineMetricsSnapshot`](crate::LineMetricsSnapshot).
    pub line_gap:            f32,
    /// Total line height: `ascent + descent + line_gap`.
    pub line_height:         f32,
    /// Height of lowercase letters like `x` (baseline to mean line).
    /// Also called the "mean line" in some references.
    pub x_height:            f32,
    /// Height of uppercase letters like `H` (baseline to cap line).
    /// Always <= [`ascent`](Self::ascent) because ascent includes room
    /// for diacritics above capitals.
    pub cap_height:          f32,
    /// Italic angle in degrees from vertical. `0.0` for upright fonts.
    pub italic_angle:        f32,
    /// Distance below the baseline for underline placement. `None` if the
    /// font's post table does not specify underline metrics — there is no
    /// meaningful fallback.
    pub underline_position:  Option<f32>,
    /// Underline stroke thickness. `None` if the font's post table does not
    /// specify underline metrics.
    pub underline_thickness: Option<f32>,
    /// Distance above the baseline for strikeout placement. `None` if the
    /// font's OS/2 table does not specify strikeout metrics — there is no
    /// meaningful fallback.
    pub strikeout_position:  Option<f32>,
    /// Strikeout stroke thickness. `None` if the font's OS/2 table does not
    /// specify strikeout metrics.
    pub strikeout_thickness: Option<f32>,
    /// The font size these metrics were computed for.
    pub font_size:           f32,
    /// Number of design units per em in the original font.
    pub units_per_em:        u16,
}

/// Bounding rectangle for a single glyph, in scaled layout units.
#[cfg(feature = "typography_overlay")]
pub struct GlyphBounds {
    /// Left edge of the glyph bounding box.
    pub min_x: f32,
    /// Bottom edge of the glyph bounding box.
    pub min_y: f32,
    /// Right edge of the glyph bounding box.
    pub max_x: f32,
    /// Top edge of the glyph bounding box.
    pub max_y: f32,
}

/// Per-glyph typographic metrics, scaled to a specific font size.
///
/// Computed on the fly by [`Font::glyph_metrics`] — never stored
/// persistently. Only available when the `typography_overlay` feature
/// is enabled.
#[cfg(feature = "typography_overlay")]
pub struct GlyphTypographyMetrics {
    /// Horizontal advance width (Apple's "Advancement").
    pub advance_width: f32,
    /// Glyph bounding rectangle.
    pub bounds:        GlyphBounds,
    /// Left side bearing — horizontal distance from the origin to the
    /// left edge of the glyph bounding box.
    pub bearing_x:     f32,
    /// Top side bearing — vertical distance from the baseline to the
    /// top edge of the glyph bounding box.
    pub bearing_y:     f32,
}

impl Font {
    /// Parses font-level metrics from raw TTF/OTF bytes.
    ///
    /// Reads the OS/2, hhea, and post tables to extract ascent, descent,
    /// line gap, cap height, x-height, italic angle, and underline/strikeout
    /// metrics. When `cap_height` or `x_height` are not in the OS/2 table,
    /// they are derived from the bounding box of the `H` or `x` glyph.
    ///
    /// Returns `None` if the font data cannot be parsed.
    #[must_use]
    pub fn from_bytes(name: &str, data: &[u8]) -> Option<Self> {
        let face = Face::parse(data, 0).ok()?;
        let units_per_em = face.units_per_em();

        let raw_ascent = face.ascender();
        // ttf-parser returns descender as negative; we store the absolute value
        // so `descent` is always positive (distance below baseline).
        let raw_descent = face.descender().abs();
        let raw_line_gap = face.line_gap();

        // Cap height: prefer OS/2 table, fall back to 'H' glyph bbox.
        let raw_cap_height = face
            .capital_height()
            .unwrap_or_else(|| glyph_top(&face, 'H').unwrap_or(raw_ascent));

        // X-height: prefer OS/2 table, fall back to 'x' glyph bbox.
        let raw_x_height = face
            .x_height()
            .unwrap_or_else(|| glyph_top(&face, 'x').unwrap_or(raw_ascent / 2));

        let raw_italic_angle = face.italic_angle();

        let (raw_underline_position, raw_underline_thickness) = match face.underline_metrics() {
            Some(m) => (Some(m.position.abs()), Some(m.thickness)),
            None => (None, None),
        };

        let (raw_strikeout_position, raw_strikeout_thickness) = match face.strikeout_metrics() {
            Some(m) => (Some(m.position), Some(m.thickness)),
            None => (None, None),
        };

        Some(Self {
            name: (*name).to_string(),
            units_per_em,
            raw_ascent,
            raw_descent,
            raw_line_gap,
            raw_cap_height,
            raw_x_height,
            raw_italic_angle,
            raw_underline_position,
            raw_underline_thickness,
            raw_strikeout_position,
            raw_strikeout_thickness,
            data: Arc::from(data),
        })
    }

    /// Returns the font family name.
    #[must_use]
    pub fn name(&self) -> &str { &self.name }

    /// Returns the raw TTF/OTF font bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] { &self.data }

    /// Returns font-level metrics scaled to `size` layout units.
    ///
    /// Pure arithmetic — no parsing, no allocation. The raw design-unit
    /// values are multiplied by `size / units_per_em`.
    #[must_use]
    pub fn metrics(&self, size: f32) -> FontMetrics {
        let scale = size / f32::from(self.units_per_em);

        let ascent = f32::from(self.raw_ascent) * scale;
        let descent = f32::from(self.raw_descent) * scale;
        let line_gap = f32::from(self.raw_line_gap) * scale;

        FontMetrics {
            ascent,
            descent,
            line_gap,
            line_height: ascent + descent + line_gap,
            x_height: f32::from(self.raw_x_height) * scale,
            cap_height: f32::from(self.raw_cap_height) * scale,
            italic_angle: self.raw_italic_angle,
            underline_position: self.raw_underline_position.map(|v| f32::from(v) * scale),
            underline_thickness: self.raw_underline_thickness.map(|v| f32::from(v) * scale),
            strikeout_position: self.raw_strikeout_position.map(|v| f32::from(v) * scale),
            strikeout_thickness: self.raw_strikeout_thickness.map(|v| f32::from(v) * scale),
            font_size: size,
            units_per_em: self.units_per_em,
        }
    }

    /// Returns per-glyph typographic metrics for `ch` at `size`.
    ///
    /// Parses the glyph on demand from stored font data. Returns `None`
    /// if the character has no glyph in this font.
    #[cfg(feature = "typography_overlay")]
    #[must_use]
    pub fn glyph_metrics(&self, ch: char, size: f32) -> Option<GlyphTypographyMetrics> {
        let face = Face::parse(&self.data, 0).ok()?;
        let glyph_id = face.glyph_index(ch)?;

        let scale = size / f32::from(self.units_per_em);

        let advance_width = face
            .glyph_hor_advance(glyph_id)
            .map_or(0.0, |a| f32::from(a) * scale);

        let rect = face.glyph_bounding_box(glyph_id)?;

        let bounds = GlyphBounds {
            min_x: f32::from(rect.x_min) * scale,
            min_y: f32::from(rect.y_min) * scale,
            max_x: f32::from(rect.x_max) * scale,
            max_y: f32::from(rect.y_max) * scale,
        };

        let bearing_x = bounds.min_x;
        let bearing_y = bounds.max_y;

        Some(GlyphTypographyMetrics {
            advance_width,
            bounds,
            bearing_x,
            bearing_y,
        })
    }

    /// Returns per-glyph typographic metrics by glyph ID at `size`.
    ///
    /// Like [`glyph_metrics`](Self::glyph_metrics) but takes a glyph index
    /// directly (as stored in [`ShapedGlyph`](crate::render::text_renderer::ShapedGlyph))
    /// rather than a character.
    #[cfg(feature = "typography_overlay")]
    #[must_use]
    pub fn glyph_metrics_by_id(&self, glyph_id: u16, size: f32) -> Option<GlyphTypographyMetrics> {
        let face = Face::parse(&self.data, 0).ok()?;
        let gid = ttf_parser::GlyphId(glyph_id);

        let scale = size / f32::from(self.units_per_em);

        let advance_width = face
            .glyph_hor_advance(gid)
            .map_or(0.0, |a| f32::from(a) * scale);

        let rect = face.glyph_bounding_box(gid)?;

        let bounds = GlyphBounds {
            min_x: f32::from(rect.x_min) * scale,
            min_y: f32::from(rect.y_min) * scale,
            max_x: f32::from(rect.x_max) * scale,
            max_y: f32::from(rect.y_max) * scale,
        };

        let bearing_x = bounds.min_x;
        let bearing_y = bounds.max_y;

        Some(GlyphTypographyMetrics {
            advance_width,
            bounds,
            bearing_x,
            bearing_y,
        })
    }
}

/// Returns the top of a glyph's bounding box in design units, or `None`.
fn glyph_top(face: &Face<'_>, ch: char) -> Option<i16> {
    let glyph_id = face.glyph_index(ch)?;
    face.glyph_bounding_box(glyph_id).map(|r| r.y_max)
}

//! Single-glyph MSDF rasterization via `fdsm` + `ttf-parser`.

use fdsm::bezier::scanline::FillRule;
use fdsm::generate::generate_msdf;
use fdsm::render::correct_sign_msdf;
use fdsm::shape::Shape;
use fdsm::transform::Transform;
use image::RgbImage;
use nalgebra::Affine2;
use nalgebra::Matrix3;
use ttf_parser::Face;
use ttf_parser::GlyphId;

/// Angle threshold for edge coloring (3 degrees, as recommended by Chlumsky).
const EDGE_COLORING_ANGLE: f64 = 3.0;

/// Seed for deterministic edge coloring.
const EDGE_COLORING_SEED: u64 = 0;

/// Default SDF range in pixels.
///
/// Higher values = smoother edges at extreme zoom but less precision.
pub const DEFAULT_SDF_RANGE: f64 = 4.0;

/// Default canonical pixel size for MSDF generation.
///
/// MSDF is resolution-independent, so all glyphs are generated at this
/// single size. The shader handles scaling.
pub const DEFAULT_CANONICAL_SIZE: u32 = 128;

/// Default padding around each glyph in pixels.
pub const DEFAULT_GLYPH_PADDING: u32 = 2;

/// Raw MSDF bitmap output from rasterization.
#[derive(Clone, Debug)]
pub struct MsdfBitmap {
    /// Pixel data in RGB format (3 bytes per pixel, row-major).
    pub data:      Vec<u8>,
    /// Width in pixels.
    pub width:     u32,
    /// Height in pixels.
    pub height:    u32,
    /// Horizontal bearing offset in em units (glyph origin to bitmap left).
    pub bearing_x: f64,
    /// Vertical bearing offset in em units (glyph origin to bitmap top).
    pub bearing_y: f64,
}

/// Rasterizes a single glyph to a 3-channel MSDF bitmap.
///
/// Uses `fdsm` with `ttf-parser` glyph outlines. Returns raw pixel data
/// (3 bytes per pixel: R, G, B distance channels) and the glyph's bearing
/// offsets in em units.
///
/// Returns `None` if the glyph has no outline (e.g., space character).
#[must_use]
pub fn rasterize_glyph(
    font_data: &[u8],
    glyph_index: u16,
    px_size: u32,
    sdf_range: f64,
    padding: u32,
) -> Option<MsdfBitmap> {
    let face = Face::parse(font_data, 0).ok()?;
    let glyph_id = GlyphId(glyph_index);

    // Load glyph shape from font.
    let shape = fdsm_ttf_parser::load_shape_from_face(&face, glyph_id)?;

    // Get glyph bounding box in font units.
    let bbox = face.glyph_bounding_box(glyph_id)?;
    let units_per_em = f64::from(face.units_per_em());
    let scale = f64::from(px_size) / units_per_em;

    // Compute bitmap dimensions with padding.
    let total_pad = f64::from(padding) + sdf_range;
    let glyph_w = f64::from(bbox.x_max - bbox.x_min) * scale;
    let glyph_h = f64::from(bbox.y_max - bbox.y_min) * scale;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let img_w = total_pad.mul_add(2.0, glyph_w).ceil() as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let img_h = total_pad.mul_add(2.0, glyph_h).ceil() as u32;

    if img_w == 0 || img_h == 0 {
        return None;
    }

    // The ceil() may add fractional pixels. Compute the actual padding
    // used on each side so the glyph outline is centered in the bitmap.
    // This ensures the bearing accounts for the ceiled bitmap size.
    let actual_pad_x = (f64::from(img_w) - glyph_w) / 2.0;
    let actual_pad_y = (f64::from(img_h) - glyph_h) / 2.0;

    // Color edges for multi-channel generation.
    let sin_alpha = EDGE_COLORING_ANGLE.to_radians().sin();
    let colored = Shape::edge_coloring_simple(shape, sin_alpha, EDGE_COLORING_SEED);

    // Build transform: font units → pixel coordinates.
    // Origin in font space is at (bbox.x_min, bbox.y_min).
    // In image space, we offset by actual_pad (centered).
    // Y axis is flipped (font: Y-up, image: Y-down).
    let tx = actual_pad_x - f64::from(bbox.x_min) * scale;
    let ty = actual_pad_y + f64::from(bbox.y_max) * scale;

    let transform = Affine2::from_matrix_unchecked(Matrix3::new(
        scale, 0.0, tx, 0.0, -scale, ty, 0.0, 0.0, 1.0,
    ));

    let mut colored = colored;
    colored.transform(&transform);
    let prepared = colored.prepare();

    // Generate MSDF into an RGB image.
    let mut image = RgbImage::new(img_w, img_h);
    generate_msdf(&prepared, sdf_range, &mut image);
    correct_sign_msdf(&mut image, &prepared, FillRule::Nonzero);

    // Bearing offsets in em units (fraction of units_per_em).
    // Use `actual_pad` (which accounts for ceil() rounding) so the
    // glyph outline is centered in the bitmap and positioned correctly.
    let bearing_x = f64::from(bbox.x_min) / units_per_em - actual_pad_x / f64::from(px_size);
    let bearing_y = f64::from(bbox.y_max) / units_per_em + actual_pad_y / f64::from(px_size);

    // Debug: dump median values at the outline boundary rows.
    // The outline top should be at pixel row actual_pad_y (from top).
    // The outline bottom should be at pixel row (img_h - actual_pad_y).
    // At those rows, median should transition from <0.5 (outside) to >0.5 (inside).
    {
        let raw = image.as_raw();
        let outline_top_row = actual_pad_y.round() as u32;
        let outline_bot_row = img_h - actual_pad_y.round() as u32 - 1;
        // Sample vertical strips at multiple columns
        for sample_col in [6_u32, img_w / 4, img_w / 2, img_w * 3 / 4, img_w - 7] {
            let mut col_medians = Vec::with_capacity(img_h as usize);
            for row in 0..img_h {
                let idx = ((row * img_w + sample_col) * 3) as usize;
                let r = raw[idx];
                let g = raw[idx + 1];
                let b = raw[idx + 2];
                let med = r.max(g).min(b).max(r.min(g));
                col_medians.push(med);
            }

            bevy::log::info!(
                "MSDF_DUMP gid={glyph_index} img={}x{} pad_y={actual_pad_y:.1} top_row={outline_top_row} bot_row={outline_bot_row} col={sample_col} medians={col_medians:?}",
                img_w, img_h,
            );
        }
    }

    Some(MsdfBitmap {
        data: image.into_raw(),
        width: img_w,
        height: img_h,
        bearing_x,
        bearing_y,
    })
}

//! Single-glyph MSDF rasterization via `fdsm` + `ttf-parser`.

use bevy_kana::ToU8;
use bevy_kana::ToU32;
use fdsm::bezier::scanline::FillRule;
use fdsm::correct_error;
use fdsm::correct_error::ErrorCorrectionConfig;
use fdsm::generate;
use fdsm::render;
use fdsm::shape::Shape;
use fdsm::transform::Transform;
use image::Rgb32FImage;
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
pub(super) const DEFAULT_SDF_RANGE: f64 = 4.0;

/// Default canonical pixel size for MSDF generation.
///
/// MSDF is resolution-independent, so all glyphs are generated at this
/// single size. The shader handles scaling.
pub(super) const DEFAULT_CANONICAL_SIZE: u32 = 64;

/// Default padding around each glyph in pixels.
pub(super) const DEFAULT_GLYPH_PADDING: u32 = 2;

/// Raw MSDF bitmap output from rasterization.
#[derive(Clone, Debug)]
pub(super) struct MsdfBitmap {
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
pub(super) fn rasterize_glyph(
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

    let img_w = total_pad.mul_add(2.0, glyph_w).ceil().to_u32();
    let img_h = total_pad.mul_add(2.0, glyph_h).ceil().to_u32();

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

    // Generate MSDF into a float image, apply error correction, then
    // convert to u8. Error correction fixes artifacts at sharp corners
    // where false edges in the multi-channel distance field produce
    // visible spikes.
    let mut image_f32 = Rgb32FImage::new(img_w, img_h);
    generate::generate_msdf(&prepared, sdf_range, &mut image_f32);
    render::correct_sign_msdf(&mut image_f32, &prepared, FillRule::Nonzero);
    {
        let ec_config = ErrorCorrectionConfig::default();
        correct_error::correct_error_msdf(
            &mut image_f32,
            &colored,
            &prepared,
            sdf_range,
            &ec_config,
        );
    }

    // Convert f32 [0.0, 1.0] to u8 [0, 255].
    let image = RgbImage::from_fn(img_w, img_h, |x, y| {
        let p = image_f32.get_pixel(x, y);
        image::Rgb([
            (p[0].clamp(0.0, 1.0) * 255.0).to_u8(),
            (p[1].clamp(0.0, 1.0) * 255.0).to_u8(),
            (p[2].clamp(0.0, 1.0) * 255.0).to_u8(),
        ])
    });

    // Bearing offsets in em units (fraction of units_per_em).
    // Use `actual_pad` (which accounts for ceil() rounding) so the
    // glyph outline is centered in the bitmap and positioned correctly.
    let bearing_x = f64::from(bbox.x_min) / units_per_em - actual_pad_x / f64::from(px_size);
    let bearing_y = f64::from(bbox.y_max) / units_per_em + actual_pad_y / f64::from(px_size);

    Some(MsdfBitmap {
        data: image.into_raw(),
        width: img_w,
        height: img_h,
        bearing_x,
        bearing_y,
    })
}

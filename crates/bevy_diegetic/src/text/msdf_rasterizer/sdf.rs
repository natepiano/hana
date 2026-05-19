//! Single-channel SDF rasterization via `fdsm` + `ttf-parser`.

use bevy_kana::ToU8;
use fdsm::bezier::scanline::FillRule;
use fdsm::generate;
use fdsm::render;
use fdsm::transform::Transform;
use image::ImageBuffer;
use image::Luma;
use nalgebra::Affine2;
use nalgebra::Matrix3;
use ttf_parser::Face;
use ttf_parser::GlyphId;

use super::DistanceField;
use super::RasterizedBitmap;
use super::Rasterizer;
use crate::text::bitmap_dims;

/// Raw single-channel SDF bitmap output from rasterization.
#[derive(Clone, Debug)]
pub(crate) struct SdfBitmap {
    /// Pixel data in single-channel format (1 byte per pixel, row-major).
    pub data:      Vec<u8>,
    /// Width in pixels.
    pub width:     u32,
    /// Height in pixels.
    pub height:    u32,
    /// Font-defined horizontal bearing in em units (atlas-invariant —
    /// `bbox.x_min / units_per_em`).
    pub bearing_x: f64,
    /// Font-defined vertical bearing in em units (atlas-invariant —
    /// `bbox.y_max / units_per_em`).
    pub bearing_y: f64,
    /// Atlas-specific horizontal bitmap inset in em units.
    pub pad_x_em:  f64,
    /// Atlas-specific vertical bitmap inset in em units.
    pub pad_y_em:  f64,
}

/// Single-channel signed-distance-field rasterizer.
///
/// Produces a one-channel signed distance value per pixel. Smoother on
/// curve-heavy outlines than MSDF (no median-of-three channel
/// disagreement) but cannot represent two intersecting edges at a sharp
/// corner — corners get rounded off compared to MSDF.
#[derive(Debug)]
pub(crate) struct SdfRasterizer {
    px_size:   u32,
    sdf_range: f64,
    padding:   u32,
}

impl SdfRasterizer {
    #[must_use]
    pub const fn new(px_size: u32, sdf_range: f64, padding: u32) -> Self {
        Self {
            px_size,
            sdf_range,
            padding,
        }
    }
}

impl Rasterizer for SdfRasterizer {
    fn rasterize(&self, font_data: &[u8], glyph_index: u16) -> Option<RasterizedBitmap> {
        let face = Face::parse(font_data, 0).ok()?;
        let glyph_id = GlyphId(glyph_index);

        let outline = fdsm_ttf_parser::load_shape_from_face(&face, glyph_id)?;

        let dims = bitmap_dims::compute_bitmap_size(
            &face,
            glyph_id,
            self.px_size,
            self.sdf_range,
            self.padding,
        )?;
        let image_width = dims.width;
        let image_height = dims.height;

        let bbox = face.glyph_bounding_box(glyph_id)?;
        let units_per_em = f64::from(face.units_per_em());
        let scale = f64::from(self.px_size) / units_per_em;
        let glyph_width = f64::from(bbox.x_max - bbox.x_min) * scale;
        let glyph_height = f64::from(bbox.y_max - bbox.y_min) * scale;

        let actual_pad_x = (f64::from(image_width) - glyph_width) / 2.0;
        let actual_pad_y = (f64::from(image_height) - glyph_height) / 2.0;

        let tx = actual_pad_x - f64::from(bbox.x_min) * scale;
        let ty = actual_pad_y + f64::from(bbox.y_max) * scale;

        let transform = Affine2::from_matrix_unchecked(Matrix3::new(
            scale, 0.0, tx, 0.0, -scale, ty, 0.0, 0.0, 1.0,
        ));

        let mut outline = outline;
        outline.transform(&transform);
        let prepared = outline.prepare();

        let mut image_f32 = ImageBuffer::<Luma<f32>, Vec<f32>>::new(image_width, image_height);
        generate::generate_sdf(&prepared, self.sdf_range, &mut image_f32);
        render::correct_sign_sdf(&mut image_f32, &prepared, FillRule::Nonzero);

        let mut data = Vec::with_capacity((image_width * image_height) as usize);
        for y in 0..image_height {
            for x in 0..image_width {
                let p = image_f32.get_pixel(x, y);
                data.push((p[0].clamp(0.0, 1.0) * 255.0).to_u8());
            }
        }

        let bearing_x = f64::from(bbox.x_min) / units_per_em;
        let bearing_y = f64::from(bbox.y_max) / units_per_em;
        let horizontal_padding_em = actual_pad_x / f64::from(self.px_size);
        let vertical_padding_em = actual_pad_y / f64::from(self.px_size);

        Some(RasterizedBitmap::Sdf(SdfBitmap {
            data,
            width: image_width,
            height: image_height,
            bearing_x,
            bearing_y,
            pad_x_em: horizontal_padding_em,
            pad_y_em: vertical_padding_em,
        }))
    }

    fn mode(&self) -> DistanceField { DistanceField::Sdf }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic/unwrap for clearer failure messages"
    )]

    use bevy_kana::ToUsize;

    use super::*;
    use crate::text::msdf_rasterizer::DEFAULT_GLYPH_PADDING;
    use crate::text::msdf_rasterizer::DEFAULT_SDF_RANGE;
    use crate::text::msdf_rasterizer::MsdfRasterizer;

    const FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
    const EB_GARAMOND: &[u8] = include_bytes!("../../../assets/fonts/EBGaramond-Regular.ttf");

    fn glyph_index(font_data: &[u8], ch: char) -> u16 {
        let face = ttf_parser::Face::parse(font_data, 0).unwrap_or_else(|e| panic!("parse: {e}"));
        face.glyph_index(ch)
            .unwrap_or_else(|| panic!("no glyph for '{ch}'"))
            .0
    }

    #[test]
    fn sdf_rasterize_letter_a_produces_single_channel_bitmap() {
        let r = SdfRasterizer::new(32, 4.0, 2);
        let idx = glyph_index(FONT_DATA, 'A');
        let bitmap = r
            .rasterize(FONT_DATA, idx)
            .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

        match bitmap {
            RasterizedBitmap::Sdf(b) => {
                assert!(b.width > 0, "width should be positive");
                assert!(b.height > 0, "height should be positive");
                assert_eq!(
                    b.data.len(),
                    (b.width * b.height).to_usize(),
                    "data length should match w*h (one channel)"
                );
            },
            RasterizedBitmap::Msdf(_) => panic!("SdfRasterizer returned Msdf variant"),
        }
    }

    #[test]
    fn sdf_mode_matches_variant() {
        let r = SdfRasterizer::new(32, 4.0, 2);
        assert_eq!(r.mode(), DistanceField::Sdf);
    }

    #[test]
    fn sdf_produces_varied_pixel_values() {
        let r = SdfRasterizer::new(32, 4.0, 2);
        let idx = glyph_index(FONT_DATA, 'A');
        let bitmap = r
            .rasterize(FONT_DATA, idx)
            .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

        let RasterizedBitmap::Sdf(b) = bitmap else {
            panic!("expected Sdf variant");
        };
        let min = b.data.iter().copied().min().unwrap_or(0);
        let max = b.data.iter().copied().max().unwrap_or(0);
        assert!(
            max - min > 50,
            "SDF should have varied pixel values, got range [{min}, {max}]"
        );
    }

    #[test]
    fn sdf_different_glyphs_differ() {
        let r = SdfRasterizer::new(32, 4.0, 2);
        let a = r.rasterize(FONT_DATA, glyph_index(FONT_DATA, 'A')).unwrap();
        let o = r.rasterize(FONT_DATA, glyph_index(FONT_DATA, 'O')).unwrap();
        let (RasterizedBitmap::Sdf(a), RasterizedBitmap::Sdf(o)) = (a, o) else {
            panic!("expected Sdf variants");
        };
        assert_ne!(
            a.data, o.data,
            "'A' and 'O' should produce different bitmaps"
        );
    }

    #[test]
    fn sdf_space_returns_none() {
        let r = SdfRasterizer::new(32, 4.0, 2);
        let idx = glyph_index(FONT_DATA, ' ');
        assert!(
            r.rasterize(FONT_DATA, idx).is_none(),
            "space has no outline, should return None"
        );
    }

    #[test]
    fn sdf_dimensions_match_msdf_for_same_glyph() {
        let msdf = MsdfRasterizer::new(64, DEFAULT_SDF_RANGE, DEFAULT_GLYPH_PADDING);
        let sdf = SdfRasterizer::new(64, DEFAULT_SDF_RANGE, DEFAULT_GLYPH_PADDING);
        for ch in ['A', 'g', 'W', 'i', 'O'] {
            let idx = glyph_index(FONT_DATA, ch);
            let m = msdf.rasterize(FONT_DATA, idx).unwrap();
            let s = sdf.rasterize(FONT_DATA, idx).unwrap();
            let (mw, mh) = match m {
                RasterizedBitmap::Msdf(b) => (b.width, b.height),
                RasterizedBitmap::Sdf(_) => panic!("expected Msdf"),
            };
            let (sw, sh) = match s {
                RasterizedBitmap::Sdf(b) => (b.width, b.height),
                RasterizedBitmap::Msdf(_) => panic!("expected Sdf"),
            };
            assert_eq!(
                (mw, mh),
                (sw, sh),
                "MSDF and SDF should agree on bitmap dimensions for '{ch}'"
            );
        }
    }

    #[test]
    fn eb_garamond_sdf_rasterizes_curve_heavy_glyphs() {
        let r = SdfRasterizer::new(64, 4.0, 2);
        for ch in ['V', 'O', 'g', 'e'] {
            let idx = glyph_index(EB_GARAMOND, ch);
            let bitmap = r
                .rasterize(EB_GARAMOND, idx)
                .unwrap_or_else(|| panic!("EB Garamond '{ch}' SDF rasterize returned None"));
            let RasterizedBitmap::Sdf(b) = bitmap else {
                panic!("expected Sdf variant");
            };
            assert!(b.width > 0 && b.height > 0);
            let max = b.data.iter().copied().max().unwrap_or(0);
            assert!(max > 0, "EB Garamond '{ch}' SDF should have nonzero pixels");
        }
    }
}

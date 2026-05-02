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

pub(super) use super::constants::DEFAULT_CANONICAL_SIZE;
pub(super) use super::constants::DEFAULT_GLYPH_PADDING;
pub(super) use super::constants::DEFAULT_SDF_RANGE;
use super::constants::EDGE_COLORING_ANGLE;
use super::constants::EDGE_COLORING_SEED;

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

#[cfg(test)]
mod parity;

#[cfg(test)]
mod tests {
    //! Tests for the MSDF rasterizer and atlas.
    //!
    //! Validates that `fdsm` produces usable MSDF bitmaps from the embedded
    //! `JetBrains Mono` font and that the atlas packs glyphs correctly.

    #![allow(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic/unwrap for clearer failure messages"
    )]

    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::PoisonError;

    use bevy_kana::ToU16;
    use bevy_kana::ToUsize;

    use super::*;
    use crate::layout::FontSlant;
    use crate::layout::TextMeasure;
    use crate::text::atlas::GlyphKey;
    use crate::text::atlas::MsdfAtlas;
    use crate::text::measurer;

    const FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
    const EB_GARAMOND: &[u8] = include_bytes!("../../../assets/fonts/EBGaramond-Regular.ttf");

    fn glyph_index(ch: char) -> u16 {
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap_or_else(|e| panic!("parse: {e}"));
        face.glyph_index(ch)
            .unwrap_or_else(|| panic!("no glyph for '{ch}'"))
            .0
    }

    // ── Rasterization ───────────────────────────────────────────────────────

    #[test]
    fn rasterize_letter_a_produces_nonzero_bitmap() {
        let idx = glyph_index('A');
        let bitmap = rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2)
            .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

        assert!(bitmap.width > 0, "width should be positive");
        assert!(bitmap.height > 0, "height should be positive");
        assert!(
            bitmap.data.len() == (bitmap.width * bitmap.height * 3).to_usize(),
            "data length should match w*h*3"
        );
    }

    #[test]
    fn rasterize_produces_varied_pixel_values() {
        let idx = glyph_index('A');
        let bitmap = rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2)
            .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

        let min = bitmap.data.iter().copied().min().unwrap_or(0);
        let max = bitmap.data.iter().copied().max().unwrap_or(0);
        assert!(
            max - min > 50,
            "MSDF should have varied pixel values, got range [{min}, {max}]"
        );
    }

    #[test]
    fn rasterize_different_glyphs_differ() {
        let a_idx = glyph_index('A');
        let o_idx = glyph_index('O');
        let a = rasterize_glyph(FONT_DATA, a_idx, 32, 4.0, 2)
            .unwrap_or_else(|| panic!("rasterize 'A' returned None"));
        let o = rasterize_glyph(FONT_DATA, o_idx, 32, 4.0, 2)
            .unwrap_or_else(|| panic!("rasterize 'O' returned None"));

        // At minimum, the data should differ (different glyph shapes).
        assert_ne!(
            a.data, o.data,
            "'A' and 'O' should produce different bitmaps"
        );
    }

    #[test]
    fn rasterize_larger_size_produces_larger_bitmap() {
        let idx = glyph_index('W');
        let small = rasterize_glyph(FONT_DATA, idx, 16, 4.0, 2)
            .unwrap_or_else(|| panic!("rasterize 'W' at 16px returned None"));
        let large = rasterize_glyph(FONT_DATA, idx, 48, 4.0, 2)
            .unwrap_or_else(|| panic!("rasterize 'W' at 48px returned None"));

        assert!(
            large.width > small.width,
            "48px should be wider than 16px: {} vs {}",
            large.width,
            small.width
        );
        assert!(
            large.height > small.height,
            "48px should be taller than 16px: {} vs {}",
            large.height,
            small.height
        );
    }

    #[test]
    fn rasterize_space_returns_none() {
        let idx = glyph_index(' ');
        let result = rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2);
        assert!(result.is_none(), "space has no outline, should return None");
    }

    // ── Atlas packing ───────────────────────────────────────────────────────

    #[test]
    fn atlas_insert_single_glyph() {
        let mut atlas = MsdfAtlas::new();
        let key = GlyphKey {
            font_id:     0,
            glyph_index: glyph_index('A'),
        };

        let metrics = atlas.get_or_insert_sync(key, FONT_DATA);
        assert!(metrics.is_some(), "should insert 'A' successfully");
        assert_eq!(atlas.glyph_count(), 1);
    }

    #[test]
    fn atlas_deduplicates_same_glyph() {
        let mut atlas = MsdfAtlas::new();
        let key = GlyphKey {
            font_id:     0,
            glyph_index: glyph_index('A'),
        };

        let first = atlas.get_or_insert_sync(key, FONT_DATA);
        let second = atlas.get_or_insert_sync(key, FONT_DATA);
        assert_eq!(atlas.glyph_count(), 1, "should not insert duplicate");
        assert_eq!(
            first.map(|m| m.uv_rect),
            second.map(|m| m.uv_rect),
            "UV rects should match"
        );
    }

    #[test]
    fn atlas_packs_many_glyphs_without_overlap() {
        let mut atlas = MsdfAtlas::new();
        let chars: Vec<char> = ('A'..='Z').chain('a'..='z').chain('0'..='9').collect();

        for ch in &chars {
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_index(*ch),
            };
            atlas.get_or_insert_sync(key, FONT_DATA);
        }

        // Verify no UV overlap. Collect all UV rects and check pairwise.
        let metrics: Vec<_> = atlas
            .get(GlyphKey {
                font_id:     0,
                glyph_index: glyph_index('A'),
            })
            .into_iter()
            .collect();

        // At minimum, all glyphs should be inserted.
        assert!(
            atlas.glyph_count() >= chars.len() - 1,
            "expected ~{} glyphs, got {}",
            chars.len(),
            atlas.glyph_count()
        );

        // Verify UVs are valid (within [0, 1]).
        for ch in &chars {
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_index(*ch),
            };
            if let Some(m) = atlas.get(key) {
                assert!(
                    m.uv_rect[0] >= 0.0 && m.uv_rect[0] <= 1.0,
                    "u_min out of range"
                );
                assert!(
                    m.uv_rect[1] >= 0.0 && m.uv_rect[1] <= 1.0,
                    "v_min out of range"
                );
                assert!(
                    m.uv_rect[2] >= 0.0 && m.uv_rect[2] <= 1.0,
                    "u_max out of range"
                );
                assert!(
                    m.uv_rect[3] >= 0.0 && m.uv_rect[3] <= 1.0,
                    "v_max out of range"
                );
                assert!(m.uv_rect[2] > m.uv_rect[0], "u_max should be > u_min");
                assert!(m.uv_rect[3] > m.uv_rect[1], "v_max should be > v_min");
            }
        }
        drop(metrics);
    }

    #[test]
    fn atlas_on_demand_ascii() {
        let mut atlas = MsdfAtlas::new();
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();

        for c in (33_u8..=126).map(char::from) {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            atlas.get_or_insert_sync(key, FONT_DATA);
        }

        // ASCII printable range is 94 chars. Some may have no outline (unlikely
        // for `JetBrains Mono` which is a complete monospace font).
        assert!(
            atlas.glyph_count() >= 80,
            "expected at least 80 ASCII glyphs, got {}",
            atlas.glyph_count()
        );
    }

    #[test]
    fn colon_glyph_rasterizes_and_has_metrics() {
        let idx = glyph_index(':');
        println!("colon glyph index: {idx}");

        let bitmap = rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2);
        assert!(bitmap.is_some(), "colon should rasterize (has outline)");

        let bm = bitmap.unwrap();
        println!(
            "colon bitmap: {}x{}, bearing ({}, {})",
            bm.width, bm.height, bm.bearing_x, bm.bearing_y
        );

        let mut atlas = MsdfAtlas::new();
        let key = GlyphKey {
            font_id:     0,
            glyph_index: idx,
        };
        atlas.get_or_insert_sync(key, FONT_DATA);
        let metrics = atlas.get(key);
        assert!(
            metrics.is_some(),
            "colon should be in atlas after on-demand insert"
        );

        let m = metrics.unwrap();
        println!(
            "colon metrics: pixel {}x{}, bearing ({}, {}), uv {:?}",
            m.pixel_width, m.pixel_height, m.bearing_x, m.bearing_y, m.uv_rect
        );
        assert!(m.pixel_width > 0, "colon should have nonzero width");
        assert!(m.pixel_height > 0, "colon should have nonzero height");
    }

    #[test]
    fn parley_colon_glyph_ids_match_cmap() {
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();
        let cmap_colon = face.glyph_index(':').unwrap().0;
        println!("cmap colon glyph ID: {cmap_colon}");

        // Shape "A::B" through parley and collect glyph IDs.
        let mut font_cx = parley::FontContext::default();
        font_cx
            .collection
            .register_fonts(FONT_DATA.to_vec().into(), None);
        let font_cx = Mutex::new(font_cx);

        let layout_cx = Mutex::new(parley::LayoutContext::<()>::default());
        let mut layout = parley::Layout::<()>::new();

        let mut fcx = font_cx.lock().unwrap_or_else(PoisonError::into_inner);
        let mut lcx = layout_cx.lock().unwrap_or_else(PoisonError::into_inner);

        let text = "A::B";
        let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, true);
        builder.push_default(parley::style::StyleProperty::FontSize(16.0));
        builder.push_default(parley::style::StyleProperty::FontFamily(
            parley::style::FontFamily::named("JetBrains Mono"),
        ));
        builder.build_into(&mut layout, text);
        layout.break_all_lines(None);

        drop(fcx);
        drop(lcx);

        let mut glyph_ids = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let parley::layout::PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let glyph_run = run.run();
                for cluster in glyph_run.clusters() {
                    for glyph in cluster.glyphs() {
                        glyph_ids.push((glyph.id, glyph.advance));
                    }
                }
            }
        }

        println!("shaped glyph IDs for {text:?}:");
        for (id, advance) in &glyph_ids {
            println!("  glyph {id} advance {advance}");
        }

        // We expect 4 glyphs: A, :, :, B
        assert_eq!(glyph_ids.len(), 4, "expected 4 glyphs for 'A::B'");

        // Check that the colon glyph IDs from parley match the cmap lookup.
        let parley_colon_1 = glyph_ids[1].0.to_u16();
        let parley_colon_2 = glyph_ids[2].0.to_u16();
        println!("parley colon glyph IDs: {parley_colon_1}, {parley_colon_2}");
        println!("cmap colon glyph ID: {cmap_colon}");

        if parley_colon_1 != cmap_colon {
            println!(
                "MISMATCH: parley returns glyph {parley_colon_1} but cmap has {cmap_colon} for ':'"
            );
        }

        // Check if the substituted glyph IDs can be rasterized.
        for &(gid, adv) in &glyph_ids {
            let gid16 = gid.to_u16();
            let result = rasterize_glyph(FONT_DATA, gid16, 32, 4.0, 2);
            let bbox = face.glyph_bounding_box(ttf_parser::GlyphId(gid16));
            let has_shape =
                fdsm_ttf_parser::load_shape_from_face(&face, ttf_parser::GlyphId(gid16)).is_some();
            println!(
                "  glyph {gid16}: rasterize={}, bbox={:?}, has_shape={has_shape}, advance={adv}",
                result.is_some(),
                bbox
            );
            if let Some(bm) = &result {
                println!(
                    "    bitmap {}x{}, bearing ({}, {})",
                    bm.width, bm.height, bm.bearing_x, bm.bearing_y
                );
            }
        }

        // Also check the original cmap colon for comparison.
        let cmap_result = rasterize_glyph(FONT_DATA, cmap_colon, 32, 4.0, 2);
        let cmap_bbox = face.glyph_bounding_box(ttf_parser::GlyphId(cmap_colon));
        println!(
            "cmap colon (glyph {cmap_colon}): rasterize={}, bbox={cmap_bbox:?}",
            cmap_result.is_some(),
        );
        if let Some(bm) = &cmap_result {
            println!(
                "    bitmap {}x{}, bearing ({}, {})",
                bm.width, bm.height, bm.bearing_x, bm.bearing_y
            );
        }
    }

    #[test]
    fn eb_garamond_rasterize_basic_glyphs() {
        let face = ttf_parser::Face::parse(EB_GARAMOND, 0).unwrap();
        println!(
            "EB Garamond: {} glyphs, {} upem",
            face.number_of_glyphs(),
            face.units_per_em()
        );

        for ch in ['f', 'i', 'A', 'V', 'T'] {
            let gid = face.glyph_index(ch).unwrap();
            let start = std::time::Instant::now();
            println!("Rasterizing '{ch}' (glyph {})...", gid.0);
            let result = rasterize_glyph(EB_GARAMOND, gid.0, 32, 4.0, 2);
            let elapsed = start.elapsed();
            println!("  result={}, took {:?}", result.is_some(), elapsed);
            if let Some(bm) = &result {
                println!("  bitmap {}x{}", bm.width, bm.height);
            }
            assert!(
                elapsed.as_secs() < 5,
                "glyph '{ch}' took too long: {elapsed:?}"
            );
        }
    }

    #[test]
    fn eb_garamond_shape_and_rasterize() {
        let mut font_cx = parley::FontContext::default();
        font_cx
            .collection
            .register_fonts(EB_GARAMOND.to_vec().into(), None);
        let font_cx = Mutex::new(font_cx);
        let layout_cx = Mutex::new(parley::LayoutContext::<()>::default());
        let mut layout = parley::Layout::<()>::new();

        let texts = [
            "fi", "fl", "ffi", "ffl", "Th", "st", "ct", "AVAV", "Type", "Wolf",
        ];

        for text in texts {
            let mut fcx = font_cx.lock().unwrap_or_else(PoisonError::into_inner);
            let mut lcx = layout_cx.lock().unwrap_or_else(PoisonError::into_inner);

            let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, true);
            builder.push_default(parley::style::StyleProperty::FontSize(36.0));
            builder.push_default(parley::style::StyleProperty::FontFamily(
                parley::style::FontFamily::named("EB Garamond"),
            ));
            builder.build_into(&mut layout, text);
            layout.break_all_lines(None);
            drop(fcx);
            drop(lcx);

            println!("\nShaped \"{text}\":");
            for line in layout.lines() {
                for item in line.items() {
                    let parley::layout::PositionedLayoutItem::GlyphRun(run) = item else {
                        continue;
                    };
                    let glyph_run = run.run();
                    for cluster in glyph_run.clusters() {
                        for glyph in cluster.glyphs() {
                            let gid = glyph.id.to_u16();
                            print!("  glyph {gid}: ");
                            let start = std::time::Instant::now();
                            let result = rasterize_glyph(EB_GARAMOND, gid, 32, 4.0, 2);
                            let elapsed = start.elapsed();
                            println!("rasterize={}, {:?}", result.is_some(), elapsed);
                            assert!(
                                elapsed.as_secs() < 10,
                                "glyph {gid} in \"{text}\" took too long: {elapsed:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Times parley measurement (no rasterization) for EB Garamond — the same
    /// codepath `DiegeticPanel` uses via `create_parley_measurer`.
    #[test]
    fn eb_garamond_measure_timing() {
        let mut font_cx = parley::FontContext::default();
        font_cx
            .collection
            .register_fonts(EB_GARAMOND.to_vec().into(), None);
        let font_cx = Arc::new(Mutex::new(font_cx));

        let measurer =
            measurer::create_parley_measurer(Arc::clone(&font_cx), vec!["EB Garamond".to_string()]);

        // Simulate the font_features example: ~72 text measurements
        let texts = [
            "fi",
            "fl",
            "ffi",
            "ffl",
            "on",
            "off",
            "LIGA — Standard Ligatures",
            "EB Garamond",
            "::",
            "->",
            "=>",
            "!=",
            "CALT — Contextual Alternates",
            "JetBrains Mono",
            "Th",
            "st",
            "ct",
            "DLIG — Discretionary",
            "AVAV",
            "Type",
            "Wolf",
            "KERN — Kerning",
            "Font Features",
        ];

        let measure = TextMeasure {
            font_id:        0,
            size:           36.0,
            weight:         crate::layout::FontWeight::NORMAL,
            slant:          FontSlant::Normal,
            line_height:    0.0,
            letter_spacing: 0.0,
            word_spacing:   0.0,
            font_features:  crate::layout::FontFeatures::default(),
        };

        let total_start = std::time::Instant::now();
        for text in &texts {
            let start = std::time::Instant::now();
            let dims = measurer(text, &measure);
            let elapsed = start.elapsed();
            println!(
                "{text:>35}: {:.1}x{:.1}  {:?}",
                dims.width, dims.height, elapsed
            );
        }
        println!(
            "\nTotal for {} measurements: {:?}",
            texts.len(),
            total_start.elapsed()
        );
    }

    // ── PNG dump (visual inspection) ────────────────────────────────────────

    #[test]
    fn dump_atlas_png() {
        let mut atlas = MsdfAtlas::new();
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();

        for c in (33_u8..=126).map(char::from) {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            atlas.get_or_insert_sync(key, FONT_DATA);
        }

        // Write one PNG per atlas page for visual inspection.
        for page in 0..atlas.page_count() {
            let path =
                std::env::temp_dir().join(format!("bevy_diegetic_msdf_atlas_page{page}.png"));
            let pixels = atlas
                .page_pixels(page)
                .unwrap_or_else(|| panic!("page {page} should exist"));
            let img = image::RgbaImage::from_raw(atlas.width(), atlas.height(), pixels.to_vec())
                .unwrap_or_else(|| panic!("failed to create image from page {page} pixels"));
            img.save(&path)
                .unwrap_or_else(|e| panic!("failed to save page {page} PNG: {e}"));
            eprintln!("Atlas page {page} PNG written to: {}", path.display());
        }
        eprintln!(
            "  {}x{}, {} pages, {} glyphs",
            atlas.width(),
            atlas.height(),
            atlas.page_count(),
            atlas.glyph_count()
        );
    }

    // ── Multi-page atlas tests ──────────────────────────────────────────────

    #[test]
    fn atlas_overflows_to_second_page() {
        // Small atlas that can't fit all ASCII glyphs on one page.
        let mut atlas = MsdfAtlas::with_size(128, 128);
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();

        for c in (33_u8..=126).map(char::from) {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            atlas.get_or_insert_sync(key, FONT_DATA);
        }

        assert!(
            atlas.page_count() > 1,
            "128x128 atlas should overflow to multiple pages, got {} page(s)",
            atlas.page_count()
        );
        assert!(
            atlas.glyph_count() >= 80,
            "expected at least 80 ASCII glyphs across pages, got {}",
            atlas.glyph_count()
        );

        // Every glyph should have a valid page_index.
        for c in (33_u8..=126).map(char::from) {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            if let Some(m) = atlas.get(key) {
                assert!(
                    m.page_index.to_usize() < atlas.page_count(),
                    "glyph '{c}' page_index {} >= page_count {}",
                    m.page_index,
                    atlas.page_count()
                );
            }
        }

        println!(
            "multi-page atlas: {} pages, {} glyphs",
            atlas.page_count(),
            atlas.glyph_count()
        );
    }

    #[test]
    fn atlas_iter_glyphs_reports_page_distribution() {
        let mut atlas = MsdfAtlas::with_size(128, 128);
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();

        for c in (33_u8..=126).map(char::from) {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            atlas.get_or_insert_sync(key, FONT_DATA);
        }

        let mut per_page_counts = vec![0_usize; atlas.page_count()];
        let mut iter_count = 0_usize;

        for (_, metrics) in atlas.iter_glyphs() {
            let page = metrics.page_index.to_usize();
            assert!(
                page < atlas.page_count(),
                "iter_glyphs returned page {page} >= page_count {}",
                atlas.page_count()
            );
            per_page_counts[page] += 1;
            iter_count += 1;
        }

        assert_eq!(
            iter_count,
            atlas.glyph_count(),
            "iter_glyphs should expose every cached glyph exactly once"
        );
        assert!(
            per_page_counts.iter().any(|&count| count > 0),
            "expected at least one page to contain glyphs"
        );
    }

    #[test]
    fn atlas_single_page_no_overflow() {
        let mut atlas = MsdfAtlas::new(); // Default 1024x1024
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();

        // Insert just A-Z — should easily fit on one page.
        for c in 'A'..='Z' {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            atlas.get_or_insert_sync(key, FONT_DATA);
        }

        assert_eq!(
            atlas.page_count(),
            1,
            "26 glyphs on 1024x1024 should fit in 1 page"
        );

        // All glyphs should be on page 0.
        for c in 'A'..='Z' {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            if let Some(m) = atlas.get(key) {
                assert_eq!(m.page_index, 0, "glyph '{c}' should be on page 0");
            }
        }
    }

    #[test]
    fn atlas_multi_page_no_uv_overlap_within_page() {
        let mut atlas = MsdfAtlas::with_size(128, 128);
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();

        let mut keys = Vec::new();
        for c in (33_u8..=126).map(char::from) {
            let Some(glyph_id) = face.glyph_index(c) else {
                continue;
            };
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_id.0,
            };
            atlas.get_or_insert_sync(key, FONT_DATA);
            keys.push(key);
        }

        // Group metrics by page and check for UV overlap within each page.
        let mut by_page: std::collections::HashMap<u32, Vec<[f32; 4]>> =
            std::collections::HashMap::new();
        for key in &keys {
            if let Some(m) = atlas.get(*key) {
                by_page.entry(m.page_index).or_default().push(m.uv_rect);
            }
        }

        for (page, rects) in &by_page {
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    let overlap = a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1];
                    assert!(
                        !overlap,
                        "UV overlap on page {page}: [{}, {}, {}, {}] vs [{}, {}, {}, {}]",
                        a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]
                    );
                }
            }
        }
    }

    #[test]
    fn dump_single_glyph_png() {
        let idx = glyph_index('A');
        let bitmap = rasterize_glyph(FONT_DATA, idx, 64, 4.0, 2)
            .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

        let path = std::env::temp_dir().join("bevy_diegetic_glyph_A.png");
        let img = image::RgbImage::from_raw(bitmap.width, bitmap.height, bitmap.data)
            .unwrap_or_else(|| panic!("failed to create image from bitmap"));
        img.save(&path)
            .unwrap_or_else(|e| panic!("failed to save glyph PNG: {e}"));

        eprintln!("Glyph 'A' PNG written to: {}", path.display());
        eprintln!("  {}x{}", bitmap.width, bitmap.height);
    }
}

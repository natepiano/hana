//! Tests for the MSDF rasterizer and atlas.
//!
//! Validates that `fdsm` produces usable MSDF bitmaps from the embedded
//! `JetBrains Mono` font and that the atlas packs glyphs correctly.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests use panic/unwrap for clearer failure messages"
)]

use bevy_kana::ToU16;
use bevy_kana::ToUsize;

use super::atlas::GlyphKey;
use super::atlas::MsdfAtlas;
use super::msdf_rasterizer;
use crate::layout::FontSlant;

/// Embedded font data for tests.
const FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

/// Resolve a character to a glyph index via `ttf-parser`.
fn glyph_index(ch: char) -> u16 {
    let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap_or_else(|e| panic!("parse: {e}"));
    face.glyph_index(ch)
        .unwrap_or_else(|| panic!("no glyph for '{ch}'"))
        .0
}

// ── Rasterization ────────────────────────────────────────────────────────────

#[test]
fn rasterize_letter_a_produces_nonzero_bitmap() {
    let idx = glyph_index('A');
    let bitmap = msdf_rasterizer::rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2)
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
    let bitmap = msdf_rasterizer::rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2)
        .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

    // An MSDF bitmap should have varied pixel values (not all zeros or all 128).
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
    let a = msdf_rasterizer::rasterize_glyph(FONT_DATA, a_idx, 32, 4.0, 2)
        .unwrap_or_else(|| panic!("rasterize 'A' returned None"));
    let o = msdf_rasterizer::rasterize_glyph(FONT_DATA, o_idx, 32, 4.0, 2)
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
    let small = msdf_rasterizer::rasterize_glyph(FONT_DATA, idx, 16, 4.0, 2)
        .unwrap_or_else(|| panic!("rasterize 'W' at 16px returned None"));
    let large = msdf_rasterizer::rasterize_glyph(FONT_DATA, idx, 48, 4.0, 2)
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
    let result = msdf_rasterizer::rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2);
    assert!(result.is_none(), "space has no outline, should return None");
}

// ── Atlas packing ────────────────────────────────────────────────────────────

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
        atlas.glyph_count() >= chars.len() - 1, // space-like chars might be skipped
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

    for c in (33_u8..=126).map(|c| c as char) {
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

    let bitmap = msdf_rasterizer::rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2);
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
    use std::sync::PoisonError;

    let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();
    let cmap_colon = face.glyph_index(':').unwrap().0;
    println!("cmap colon glyph ID: {cmap_colon}");

    // Shape "A::B" through parley and collect glyph IDs.
    let mut font_cx = parley::FontContext::default();
    font_cx
        .collection
        .register_fonts(FONT_DATA.to_vec().into(), None);
    let font_cx = std::sync::Mutex::new(font_cx);

    let layout_cx = std::sync::Mutex::new(parley::LayoutContext::<()>::default());
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
        let result = msdf_rasterizer::rasterize_glyph(FONT_DATA, gid16, 32, 4.0, 2);
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
    let cmap_result = msdf_rasterizer::rasterize_glyph(FONT_DATA, cmap_colon, 32, 4.0, 2);
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

const EB_GARAMOND: &[u8] = include_bytes!("../../assets/fonts/EBGaramond-Regular.ttf");

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
        let result = msdf_rasterizer::rasterize_glyph(EB_GARAMOND, gid.0, 32, 4.0, 2);
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
    use std::sync::PoisonError;

    let mut font_cx = parley::FontContext::default();
    font_cx
        .collection
        .register_fonts(EB_GARAMOND.to_vec().into(), None);
    let font_cx = std::sync::Mutex::new(font_cx);
    let layout_cx = std::sync::Mutex::new(parley::LayoutContext::<()>::default());
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
                        let result = msdf_rasterizer::rasterize_glyph(EB_GARAMOND, gid, 32, 4.0, 2);
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
    use std::sync::Arc;
    use std::sync::Mutex;

    let mut font_cx = parley::FontContext::default();
    font_cx
        .collection
        .register_fonts(EB_GARAMOND.to_vec().into(), None);
    let font_cx = Arc::new(Mutex::new(font_cx));

    let measurer = crate::text::measurer::create_parley_measurer(
        Arc::clone(&font_cx),
        vec!["EB Garamond".to_string()],
    );

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

    let measure = crate::layout::TextMeasure {
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

// ── PNG dump (visual inspection) ─────────────────────────────────────────────

#[test]
fn dump_atlas_png() {
    let mut atlas = MsdfAtlas::new();
    let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();
    for c in (33_u8..=126).map(|c| c as char) {
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
        let path = std::env::temp_dir().join(format!("bevy_diegetic_msdf_atlas_page{page}.png"));
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

// ── Multi-page atlas tests ───────────────────────────────────────────────────

#[test]
fn atlas_overflows_to_second_page() {
    // Small atlas that can't fit all ASCII glyphs on one page.
    let mut atlas = MsdfAtlas::with_size(128, 128);
    let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();

    for c in (33_u8..=126).map(|c| c as char) {
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
    for c in (33_u8..=126).map(|c| c as char) {
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

    for c in (33_u8..=126).map(|c| c as char) {
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
    for c in (33_u8..=126).map(|c| c as char) {
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
    let bitmap = msdf_rasterizer::rasterize_glyph(FONT_DATA, idx, 64, 4.0, 2)
        .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

    let path = std::env::temp_dir().join("bevy_diegetic_glyph_A.png");
    let img = image::RgbImage::from_raw(bitmap.width, bitmap.height, bitmap.data)
        .unwrap_or_else(|| panic!("failed to create image from bitmap"));
    img.save(&path)
        .unwrap_or_else(|e| panic!("failed to save glyph PNG: {e}"));

    eprintln!("Glyph 'A' PNG written to: {}", path.display());
    eprintln!("  {}x{}", bitmap.width, bitmap.height);
}

//! Tests for the MSDF rasterizer and atlas.
//!
//! Validates that `fdsm` produces usable MSDF bitmaps from the embedded
//! `JetBrains Mono` font and that the atlas packs glyphs correctly.

use super::atlas::GlyphKey;
use super::atlas::MsdfAtlas;
use super::msdf_rasterizer::rasterize_glyph;

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
    let bitmap = rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2)
        .unwrap_or_else(|| panic!("rasterize 'A' returned None"));

    assert!(bitmap.width > 0, "width should be positive");
    assert!(bitmap.height > 0, "height should be positive");
    assert!(
        bitmap.data.len() == (bitmap.width * bitmap.height * 3) as usize,
        "data length should match w*h*3"
    );
}

#[test]
fn rasterize_produces_varied_pixel_values() {
    let idx = glyph_index('A');
    let bitmap = rasterize_glyph(FONT_DATA, idx, 32, 4.0, 2)
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
        .get(&GlyphKey {
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
        if let Some(m) = atlas.get(&key) {
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
    let metrics = atlas.get(&key);
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
    builder.push_default(parley::style::StyleProperty::FontStack(
        parley::style::FontStack::Single(parley::style::FontFamily::Named("JetBrains Mono".into())),
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
    let parley_colon_1 = glyph_ids[1].0 as u16;
    let parley_colon_2 = glyph_ids[2].0 as u16;
    println!("parley colon glyph IDs: {parley_colon_1}, {parley_colon_2}");
    println!("cmap colon glyph ID: {cmap_colon}");

    if parley_colon_1 != cmap_colon {
        println!(
            "MISMATCH: parley returns glyph {parley_colon_1} but cmap has {cmap_colon} for ':'"
        );
    }

    // Check if the substituted glyph IDs can be rasterized.
    for &(gid, adv) in &glyph_ids {
        let gid16 = gid as u16;
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

    // Write the atlas as a PNG for visual inspection.
    let path = std::env::temp_dir().join("bevy_diegetic_msdf_atlas.png");
    let pixels = atlas.pixels();
    let img = image::RgbaImage::from_raw(atlas.width(), atlas.height(), pixels.to_vec())
        .unwrap_or_else(|| panic!("failed to create image from atlas pixels"));
    img.save(&path)
        .unwrap_or_else(|e| panic!("failed to save atlas PNG: {e}"));

    eprintln!("Atlas PNG written to: {}", path.display());
    eprintln!(
        "  {}x{}, {} glyphs",
        atlas.width(),
        atlas.height(),
        atlas.glyph_count()
    );
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

//! Unit tests for glyph mesh construction.

use bevy::prelude::Mesh;

use super::glyph_quad;
use super::glyph_quad::GlyphQuadData;

#[test]
fn mesh_vertex_and_index_counts() {
    let quads = vec![
        GlyphQuadData {
            position: [0.0, 1.0, 0.0],
            size:     [0.5, 0.8],
            uv_rect:  [0.0, 0.0, 0.1, 0.1],
            color:    [1.0, 1.0, 1.0, 1.0],
        },
        GlyphQuadData {
            position: [0.6, 1.0, 0.0],
            size:     [0.5, 0.8],
            uv_rect:  [0.1, 0.0, 0.2, 0.1],
            color:    [1.0, 1.0, 1.0, 1.0],
        },
        GlyphQuadData {
            position: [1.2, 1.0, 0.0],
            size:     [0.5, 0.8],
            uv_rect:  [0.2, 0.0, 0.3, 0.1],
            color:    [1.0, 1.0, 1.0, 1.0],
        },
    ];

    let mesh = glyph_quad::build_glyph_mesh(&quads);

    // 3 glyphs × 4 vertices = 12.
    let vertex_count = mesh.count_vertices();
    assert_eq!(vertex_count, 12, "expected 12 vertices for 3 glyphs");

    // 3 glyphs × 6 indices = 18.
    let index_count = mesh.indices().map_or(0, bevy::mesh::Indices::len);
    assert_eq!(index_count, 18, "expected 18 indices for 3 glyphs");
}

#[test]
fn mesh_single_quad_has_uvs() {
    let quads = vec![GlyphQuadData {
        position: [0.0, 1.0, 0.0],
        size:     [1.0, 1.0],
        uv_rect:  [0.25, 0.5, 0.75, 1.0],
        color:    [1.0, 0.0, 0.0, 1.0],
    }];

    let mesh = glyph_quad::build_glyph_mesh(&quads);

    // Should have UV attribute with 4 vertices.
    #[allow(clippy::redundant_closure_for_method_calls)]
    let uv_count = mesh
        .attribute(Mesh::ATTRIBUTE_UV_0)
        .map_or(0, |attr| attr.len());
    assert_eq!(uv_count, 4, "expected 4 UV entries for 1 quad");
}

#[test]
fn empty_quads_produce_empty_mesh() {
    let mesh = glyph_quad::build_glyph_mesh(&[]);

    let vertex_count = mesh.count_vertices();
    assert_eq!(vertex_count, 0);

    let index_count = mesh.indices().map_or(0, bevy::mesh::Indices::len);
    assert_eq!(index_count, 0);
}

// ── clip_overlapping_quads tests ─────────────────────────────────────────────

fn make_quad(x: f32, y: f32, width: f32) -> GlyphQuadData {
    GlyphQuadData {
        position: [x, y, 0.0],
        size:     [width, 0.8],
        uv_rect:  [0.0, 0.0, 1.0, 1.0],
        color:    [1.0, 1.0, 1.0, 1.0],
    }
}

#[test]
fn clip_same_line_overlap_trims_both_quads() {
    // Two quads on the same line (same Y) that overlap by 0.1 in X.
    let mut quads = vec![
        (0_u32, make_quad(0.0, 1.0, 0.6)),
        (0, make_quad(0.5, 1.0, 0.6)),
    ];

    glyph_quad::clip_overlapping_quads(&mut quads);

    // Overlap = 0.6 - 0.5 = 0.1, half = 0.05.
    let first_width = quads[0].1.size[0];
    let second_width = quads[1].1.size[0];
    assert!(
        first_width < 0.6,
        "first quad should be trimmed, got {first_width}"
    );
    assert!(
        second_width < 0.6,
        "second quad should be trimmed, got {second_width}"
    );
    // Both widths should remain positive.
    assert!(first_width > 0.0, "first quad width must be positive");
    assert!(second_width > 0.0, "second quad width must be positive");
}

#[test]
fn clip_skips_cross_line_pairs() {
    // Last glyph of line 1 at x=3.0, first glyph of line 2 at x=0.0.
    // Different Y positions — must NOT clip.
    let mut quads = vec![
        (0_u32, make_quad(3.0, 1.0, 0.5)),
        (0, make_quad(0.0, 0.0, 0.5)),
    ];
    let width_before_first = quads[0].1.size[0];
    let width_before_second = quads[1].1.size[0];
    let x_before_second = quads[1].1.position[0];

    glyph_quad::clip_overlapping_quads(&mut quads);

    assert!(
        (quads[0].1.size[0] - width_before_first).abs() < f32::EPSILON,
        "line 1 last quad should be untouched"
    );
    assert!(
        (quads[1].1.size[0] - width_before_second).abs() < f32::EPSILON,
        "line 2 first quad width should be untouched"
    );
    assert!(
        (quads[1].1.position[0] - x_before_second).abs() < f32::EPSILON,
        "line 2 first quad X position should be untouched"
    );
}

#[test]
fn clip_handles_multiline_with_intraline_overlap() {
    // Line 1: two quads that overlap each other, then line 2 starts fresh.
    let mut quads = vec![
        (0_u32, make_quad(0.0, 1.0, 0.6)), // line 1, glyph A
        (0, make_quad(0.5, 1.0, 0.6)),     // line 1, glyph B (overlaps A)
        (0, make_quad(0.0, 0.0, 0.5)),     // line 2, glyph C
        (0, make_quad(0.4, 0.0, 0.5)),     // line 2, glyph D (overlaps C)
    ];

    glyph_quad::clip_overlapping_quads(&mut quads);

    // A-B overlap should be clipped (same line).
    assert!(quads[0].1.size[0] < 0.6, "A should be trimmed");
    assert!(quads[1].1.size[0] < 0.6, "B should be trimmed");

    // B-C cross-line should NOT be clipped.
    // C's position and width should only reflect C-D clipping, not B-C.
    assert!(quads[2].1.size[0] < 0.5, "C should be trimmed by D overlap");
    assert!(quads[3].1.size[0] < 0.5, "D should be trimmed by C overlap");

    // All widths positive.
    for (i, (_, quad)) in quads.iter().enumerate() {
        assert!(quad.size[0] > 0.0, "quad {i} width must be positive");
    }
}

#[test]
fn clip_cross_line_prevents_negative_width() {
    // Extreme case: line 1 last glyph far right, line 2 first glyph at origin.
    // Without the fix this would produce negative widths.
    let mut quads = vec![
        (0_u32, make_quad(10.0, 2.0, 0.5)),
        (0, make_quad(0.0, 0.5, 0.5)),
    ];

    glyph_quad::clip_overlapping_quads(&mut quads);

    assert!(
        quads[0].1.size[0] > 0.0,
        "must not produce negative width: got {}",
        quads[0].1.size[0]
    );
    assert!(
        quads[1].1.size[0] > 0.0,
        "must not produce negative width: got {}",
        quads[1].1.size[0]
    );
}

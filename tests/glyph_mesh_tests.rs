#![allow(clippy::float_cmp)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::unwrap_used)]

//! Unit tests for glyph mesh construction.

use bevy::prelude::Mesh;
use bevy_diegetic::GlyphQuadData;
use bevy_diegetic::build_glyph_mesh;

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

    let mesh = build_glyph_mesh(&quads);

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

    let mesh = build_glyph_mesh(&quads);

    // Should have UV attribute with 4 vertices.
    let uv_count = mesh
        .attribute(Mesh::ATTRIBUTE_UV_0)
        .map_or(0, |attr| attr.len());
    assert_eq!(uv_count, 4, "expected 4 UV entries for 1 quad");
}

#[test]
fn empty_quads_produce_empty_mesh() {
    let mesh = build_glyph_mesh(&[]);

    let vertex_count = mesh.count_vertices();
    assert_eq!(vertex_count, 0);

    let index_count = mesh.indices().map_or(0, bevy::mesh::Indices::len);
    assert_eq!(index_count, 0);
}

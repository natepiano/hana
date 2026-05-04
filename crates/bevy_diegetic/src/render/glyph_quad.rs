//! Per-glyph quad data for mesh construction.
//!
//! # MSDF seam artifact prevention
//!
//! MSDF text rendering has two distinct seam artifact mechanisms, each
//! addressed by a different fix that must work together:
//!
//! 1. **Quad overlap (geometry)** — SDF padding extends glyph quads beyond their advance width,
//!    causing adjacent quads to overlap in world space. With `AlphaMode::Blend`, overlapping
//!    semi-transparent edge ramps composite twice, producing visible vertical lines. **Fix:**
//!    [`clip_overlapping_quads`] trims overlapping quads at their midpoint and adjusts UV
//!    coordinates. Applied CPU-side after quad construction.
//!
//! 2. **Atlas texture bleed (sampling)** — Glyphs packed edge-to-edge in the atlas texture cause
//!    linear filtering at UV boundaries to sample into adjacent glyph regions. The MSDF
//!    median-of-three decode amplifies even tiny bleed into visible lines. **Fix:**
//!    [`ATLAS_GUTTER`](crate::text::atlas) adds a 1-texel gutter around each glyph with replicated
//!    border texels, and UV coordinates are inset by half a texel so the sampler hits texel
//!    centers.
//!
//! Both fixes are required — overlap clipping alone misses non-overlapping
//! glyph pairs (like `g`/`r` in a monospace font), and atlas guttering
//! alone doesn't prevent double-compositing from overlapping geometry.

use bevy::mesh::Indices;
use bevy::prelude::Mesh;
use bevy::render::render_resource::PrimitiveTopology;
use bevy_kana::ToU32;

use super::constants::GLYPH_QUAD_LINE_TOLERANCE;

/// Per-glyph data used when building batched quad meshes.
///
/// This struct is used on the CPU to build per-glyph quad vertices within
/// a batched `Mesh`. Each glyph becomes 4 vertices + 6 indices (two triangles).
#[derive(Clone, Copy, Debug)]
pub(super) struct GlyphQuadData {
    /// Position of the glyph quad's top-left corner in panel-local space.
    pub position: [f32; 3],
    /// Size of the glyph quad in panel-local units (width, height).
    pub size:     [f32; 2],
    /// UV rectangle in the atlas: `[u_min, v_min, u_max, v_max]`.
    pub uv_rect:  [f32; 4],
    /// RGBA color for this glyph (written as vertex color).
    pub color:    [f32; 4],
}

/// Clips overlapping adjacent glyph quads to eliminate double-compositing
/// artifacts at quad seams.
///
/// MSDF glyph quads include SDF padding that extends beyond the glyph's
/// advance width, causing adjacent quads to overlap. When rendered with
/// `AlphaMode::Blend`, the overlapping semi-transparent edge ramps
/// composite twice, producing visible vertical line artifacts.
///
/// This function splits each overlap at its midpoint — the left half goes
/// to the earlier glyph, the right half to the later glyph — and adjusts
/// UV coordinates proportionally. O(n) linear pass over sorted quads.
///
/// Only clips pairs on the **same line** (matching Y position within
/// tolerance). Cross-line pairs are skipped to prevent the last glyph of
/// one line from destroying the first glyph of the next line.
pub(super) fn clip_overlapping_quads(quads: &mut [(u32, GlyphQuadData)]) {
    if quads.len() < 2 {
        return;
    }

    for i in 0..quads.len() - 1 {
        // Skip pairs on different lines — they cannot meaningfully overlap.
        let y_delta = (quads[i].1.position[1] - quads[i + 1].1.position[1]).abs();
        if y_delta > GLYPH_QUAD_LINE_TOLERANCE {
            continue;
        }

        let right_edge_i = quads[i].1.position[0] + quads[i].1.size[0];
        let left_edge_next = quads[i + 1].1.position[0];

        // Only clip if quads actually overlap in X.
        if right_edge_i <= left_edge_next {
            continue;
        }

        let overlap = right_edge_i - left_edge_next;
        let half_overlap = overlap * 0.5;

        // Trim the right side of quad i.
        let old_width_i = quads[i].1.size[0];
        let new_width_i = old_width_i - half_overlap;
        let u_min_i = quads[i].1.uv_rect[0];
        let u_max_i = quads[i].1.uv_rect[2];
        let new_u_max_i = (u_max_i - u_min_i).mul_add(new_width_i / old_width_i, u_min_i);
        quads[i].1.size[0] = new_width_i;
        quads[i].1.uv_rect[2] = new_u_max_i;

        // Trim the left side of quad i+1.
        let old_width_next = quads[i + 1].1.size[0];
        let new_width_next = old_width_next - half_overlap;
        let u_min_next = quads[i + 1].1.uv_rect[0];
        let u_max_next = quads[i + 1].1.uv_rect[2];
        let new_u_min_next =
            (u_max_next - u_min_next).mul_add(half_overlap / old_width_next, u_min_next);
        quads[i + 1].1.position[0] += half_overlap;
        quads[i + 1].1.size[0] = new_width_next;
        quads[i + 1].1.uv_rect[0] = new_u_min_next;
    }
}

/// Clips a glyph quad to an axis-aligned clip rect in panel-local Y-up
/// coordinates. Returns `None` if the quad is entirely outside the clip rect.
///
/// `clip` is `[left, bottom, right, top]` in the same Y-up space as the
/// quad's position. UV coordinates are adjusted proportionally so MSDF
/// sampling remains correct for the visible portion.
#[must_use]
pub(super) fn clip_quad_to_rect(quad: &GlyphQuadData, clip: [f32; 4]) -> Option<GlyphQuadData> {
    let [clip_left, clip_bottom, clip_right, clip_top] = clip;
    let [qx, qy, qz] = quad.position;
    let [qw, qh] = quad.size;

    let quad_left = qx;
    let quad_right = qx + qw;
    let quad_top = qy;
    let quad_bottom = qy - qh;

    // Entirely outside.
    if quad_right <= clip_left
        || quad_left >= clip_right
        || quad_top <= clip_bottom
        || quad_bottom >= clip_top
    {
        return None;
    }

    let new_left = quad_left.max(clip_left);
    let new_right = quad_right.min(clip_right);
    let new_top = quad_top.min(clip_top);
    let new_bottom = quad_bottom.max(clip_bottom);

    let new_w = new_right - new_left;
    let new_h = new_top - new_bottom;

    // Proportional UV adjustment.
    let [u_min, v_min, u_max, v_max] = quad.uv_rect;
    let u_span = u_max - u_min;
    let v_span = v_max - v_min;

    let left_fraction = (new_left - quad_left) / qw;
    let right_fraction = (new_right - quad_left) / qw;
    let top_fraction = (quad_top - new_top) / qh;
    let bottom_fraction = (quad_top - new_bottom) / qh;

    Some(GlyphQuadData {
        position: [new_left, new_top, qz],
        size:     [new_w, new_h],
        uv_rect:  [
            u_span.mul_add(left_fraction, u_min),
            v_span.mul_add(top_fraction, v_min),
            u_span.mul_add(right_fraction, u_min),
            v_span.mul_add(bottom_fraction, v_min),
        ],
        color:    quad.color,
    })
}

/// Builds a `Mesh` from a list of glyph quads.
///
/// Each glyph produces 4 vertices (quad corners) and 6 indices (two triangles).
/// UV coordinates map into the MSDF atlas texture.
#[must_use]
pub(super) fn build_glyph_mesh(quads: &[GlyphQuadData]) -> Mesh {
    let vertex_count = quads.len() * 4;
    let index_count = quads.len() * 6;

    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut panel_local_positions = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);
    let mut indices = Vec::with_capacity(index_count);

    for (idx, quad) in quads.iter().enumerate() {
        let [qx, qy, qz] = quad.position;
        let [qw, qh] = quad.size;
        let [u_min, v_min, u_max, v_max] = quad.uv_rect;

        let base = (idx * 4).to_u32();

        // Quad vertices: TL, TR, BR, BL (Y-up coordinate system).
        positions.push([qx, qy, qz]); // TL
        positions.push([qx + qw, qy, qz]); // TR
        positions.push([qx + qw, qy - qh, qz]); // BR
        positions.push([qx, qy - qh, qz]); // BL

        // All normals point toward camera (+Z).
        normals.push([0.0, 0.0, 1.0]);
        normals.push([0.0, 0.0, 1.0]);
        normals.push([0.0, 0.0, 1.0]);
        normals.push([0.0, 0.0, 1.0]);

        // UV mapping: image origin is top-left, so TL gets (u_min, v_min).
        uvs.push([u_min, v_min]); // TL
        uvs.push([u_max, v_min]); // TR
        uvs.push([u_max, v_max]); // BR
        uvs.push([u_min, v_max]); // BL

        // Panel-local XY for shader-side clipping.
        panel_local_positions.push([qx, qy]); // TL
        panel_local_positions.push([qx + qw, qy]); // TR
        panel_local_positions.push([qx + qw, qy - qh]); // BR
        panel_local_positions.push([qx, qy - qh]); // BL

        // Per-glyph vertex color.
        colors.push(quad.color);
        colors.push(quad.color);
        colors.push(quad.color);
        colors.push(quad.color);

        // Two triangles (CCW winding for front-face toward +Z):
        // TL-BL-BR and TL-BR-TR.
        indices.push(base);
        indices.push(base + 3);
        indices.push(base + 2);
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 1);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, panel_local_positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests use expect for clearer failure messages"
)]
mod tests {
    use bevy::prelude::Mesh;

    use super::*;

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

        let vertex_count = mesh.count_vertices();
        assert_eq!(vertex_count, 12, "expected 12 vertices for 3 glyphs");

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

        let uv = mesh
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .expect("mesh should have UV_0");
        assert_eq!(uv.len(), 4, "expected 4 UV entries for 1 quad");

        let clip_uv = mesh
            .attribute(Mesh::ATTRIBUTE_UV_1)
            .expect("mesh should have UV_1");
        assert_eq!(
            clip_uv.len(),
            4,
            "expected 4 panel-local UV_1 entries for 1 quad"
        );
    }

    #[test]
    fn empty_quads_produce_empty_mesh() {
        let mesh = build_glyph_mesh(&[]);

        let vertex_count = mesh.count_vertices();
        assert_eq!(vertex_count, 0);

        let index_count = mesh.indices().map_or(0, bevy::mesh::Indices::len);
        assert_eq!(index_count, 0);
    }

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
        let mut quads = vec![
            (0_u32, make_quad(0.0, 1.0, 0.6)),
            (0, make_quad(0.5, 1.0, 0.6)),
        ];

        clip_overlapping_quads(&mut quads);

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
        assert!(first_width > 0.0, "first quad width must be positive");
        assert!(second_width > 0.0, "second quad width must be positive");
    }

    #[test]
    fn clip_skips_cross_line_pairs() {
        let mut quads = vec![
            (0_u32, make_quad(3.0, 1.0, 0.5)),
            (0, make_quad(0.0, 0.0, 0.5)),
        ];
        let width_before_first = quads[0].1.size[0];
        let width_before_second = quads[1].1.size[0];
        let x_before_second = quads[1].1.position[0];

        clip_overlapping_quads(&mut quads);

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
        let mut quads = vec![
            (0_u32, make_quad(0.0, 1.0, 0.6)),
            (0, make_quad(0.5, 1.0, 0.6)),
            (0, make_quad(0.0, 0.0, 0.5)),
            (0, make_quad(0.4, 0.0, 0.5)),
        ];

        clip_overlapping_quads(&mut quads);

        assert!(quads[0].1.size[0] < 0.6, "A should be trimmed");
        assert!(quads[1].1.size[0] < 0.6, "B should be trimmed");
        assert!(quads[2].1.size[0] < 0.5, "C should be trimmed by D overlap");
        assert!(quads[3].1.size[0] < 0.5, "D should be trimmed by C overlap");

        for (i, (_, quad)) in quads.iter().enumerate() {
            assert!(quad.size[0] > 0.0, "quad {i} width must be positive");
        }
    }

    #[test]
    fn clip_cross_line_prevents_negative_width() {
        let mut quads = vec![
            (0_u32, make_quad(10.0, 2.0, 0.5)),
            (0, make_quad(0.0, 0.5, 0.5)),
        ];

        clip_overlapping_quads(&mut quads);

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

    fn make_clip_quad(x: f32, y: f32, w: f32, h: f32) -> GlyphQuadData {
        GlyphQuadData {
            position: [x, y, 0.0],
            size:     [w, h],
            uv_rect:  [0.0, 0.0, 1.0, 1.0],
            color:    [1.0, 1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn clip_quad_fully_inside() {
        let quad = make_clip_quad(2.0, 8.0, 4.0, 4.0);
        let result = clip_quad_to_rect(&quad, [0.0, 0.0, 10.0, 10.0]);
        let clipped = result.expect("should be inside");
        assert!((clipped.position[0] - 2.0).abs() < f32::EPSILON);
        assert!((clipped.size[0] - 4.0).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[0] - 0.0).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[2] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn clip_quad_fully_outside() {
        let quad = make_clip_quad(20.0, 30.0, 4.0, 4.0);
        let result = clip_quad_to_rect(&quad, [0.0, 0.0, 10.0, 10.0]);
        assert!(result.is_none());
    }

    #[test]
    fn clip_quad_left_edge() {
        let quad = make_clip_quad(-2.0, 5.0, 4.0, 4.0);
        let result = clip_quad_to_rect(&quad, [0.0, 0.0, 10.0, 10.0]);
        let clipped = result.expect("should be partially visible");
        assert!((clipped.position[0] - 0.0).abs() < f32::EPSILON);
        assert!((clipped.size[0] - 2.0).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[0] - 0.5).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[2] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn clip_quad_right_edge() {
        let quad = make_clip_quad(8.0, 5.0, 4.0, 4.0);
        let result = clip_quad_to_rect(&quad, [0.0, 0.0, 10.0, 10.0]);
        let clipped = result.expect("should be partially visible");
        assert!((clipped.size[0] - 2.0).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[2] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn clip_quad_top_edge() {
        let quad = make_clip_quad(2.0, 12.0, 4.0, 4.0);
        let result = clip_quad_to_rect(&quad, [0.0, 0.0, 10.0, 10.0]);
        let clipped = result.expect("should be partially visible");
        assert!((clipped.position[1] - 10.0).abs() < f32::EPSILON);
        assert!((clipped.size[1] - 2.0).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[1] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn clip_quad_bottom_edge() {
        let quad = make_clip_quad(2.0, 2.0, 4.0, 4.0);
        let result = clip_quad_to_rect(&quad, [0.0, 0.0, 10.0, 10.0]);
        let clipped = result.expect("should be partially visible");
        assert!((clipped.size[1] - 2.0).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[3] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn clip_quad_corner_two_edges() {
        let quad = make_clip_quad(-2.0, 12.0, 4.0, 4.0);
        let result = clip_quad_to_rect(&quad, [0.0, 0.0, 10.0, 10.0]);
        let clipped = result.expect("should be partially visible");
        assert!((clipped.position[0] - 0.0).abs() < f32::EPSILON);
        assert!((clipped.position[1] - 10.0).abs() < f32::EPSILON);
        assert!((clipped.size[0] - 2.0).abs() < f32::EPSILON);
        assert!((clipped.size[1] - 2.0).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[0] - 0.5).abs() < f32::EPSILON);
        assert!((clipped.uv_rect[1] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn clip_quad_uv_proportionality() {
        let quad = GlyphQuadData {
            position: [0.0, 10.0, 0.0],
            size:     [8.0, 8.0],
            uv_rect:  [0.25, 0.1, 0.75, 0.9],
            color:    [1.0; 4],
        };
        let result = clip_quad_to_rect(&quad, [2.0, 4.0, 6.0, 8.0]);
        let clipped = result.expect("should be partially visible");
        let u_span = 0.75 - 0.25;
        let v_span = 0.9 - 0.1;
        assert!((clipped.uv_rect[0] - 0.25_f32.mul_add(u_span, 0.25)).abs() < 1e-6);
        assert!((clipped.uv_rect[2] - 0.75_f32.mul_add(u_span, 0.25)).abs() < 1e-6);
        assert!((clipped.uv_rect[1] - 0.25_f32.mul_add(v_span, 0.1)).abs() < 1e-6);
        assert!((clipped.uv_rect[3] - 0.75_f32.mul_add(v_span, 0.1)).abs() < 1e-6);
    }
}

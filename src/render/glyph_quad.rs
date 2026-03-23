//! Per-glyph quad data for mesh construction.

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
pub(super) fn clip_overlapping_quads(quads: &mut [GlyphQuadData]) {
    if quads.len() < 2 {
        return;
    }

    for i in 0..quads.len() - 1 {
        let right_edge_i = quads[i].position[0] + quads[i].size[0];
        let left_edge_next = quads[i + 1].position[0];

        // Only clip if quads actually overlap in X.
        if right_edge_i <= left_edge_next {
            continue;
        }

        let overlap = right_edge_i - left_edge_next;
        let half_overlap = overlap * 0.5;

        // Trim the right side of quad i.
        let old_width_i = quads[i].size[0];
        let new_width_i = old_width_i - half_overlap;
        let u_min_i = quads[i].uv_rect[0];
        let u_max_i = quads[i].uv_rect[2];
        let new_u_max_i = u_min_i + (u_max_i - u_min_i) * (new_width_i / old_width_i);
        quads[i].size[0] = new_width_i;
        quads[i].uv_rect[2] = new_u_max_i;

        // Trim the left side of quad i+1.
        let old_width_next = quads[i + 1].size[0];
        let new_width_next = old_width_next - half_overlap;
        let u_min_next = quads[i + 1].uv_rect[0];
        let u_max_next = quads[i + 1].uv_rect[2];
        let new_u_min_next =
            u_min_next + (u_max_next - u_min_next) * (half_overlap / old_width_next);
        quads[i + 1].position[0] += half_overlap;
        quads[i + 1].size[0] = new_width_next;
        quads[i + 1].uv_rect[0] = new_u_min_next;
    }
}

/// Builds a `Mesh` from a list of glyph quads.
///
/// Each glyph produces 4 vertices (quad corners) and 6 indices (two triangles).
/// UV coordinates map into the MSDF atlas texture.
#[must_use]
pub(super) fn build_glyph_mesh(quads: &[GlyphQuadData]) -> bevy::prelude::Mesh {
    use bevy::mesh::Indices;
    use bevy::prelude::Mesh;
    use bevy::render::render_resource::PrimitiveTopology;

    let vertex_count = quads.len() * 4;
    let index_count = quads.len() * 6;

    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);
    let mut indices = Vec::with_capacity(index_count);

    for (idx, quad) in quads.iter().enumerate() {
        let [qx, qy, qz] = quad.position;
        let [qw, qh] = quad.size;
        let [u_min, v_min, u_max, v_max] = quad.uv_rect;

        #[allow(clippy::cast_possible_truncation)]
        let base = (idx * 4) as u32;

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
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

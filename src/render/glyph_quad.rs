//! Per-glyph quad data for mesh construction.

/// Per-glyph data used when building batched quad meshes.
///
/// This struct is used on the CPU to build per-glyph quad vertices within
/// a batched `Mesh`. Each glyph becomes 4 vertices + 6 indices (two triangles).
#[derive(Clone, Copy, Debug)]
pub struct GlyphQuadData {
    /// Position of the glyph quad's top-left corner in panel-local space.
    pub position: [f32; 3],
    /// Size of the glyph quad in panel-local units (width, height).
    pub size:     [f32; 2],
    /// UV rectangle in the atlas: `[u_min, v_min, u_max, v_max]`.
    pub uv_rect:  [f32; 4],
}

/// Builds a `Mesh` from a list of glyph quads.
///
/// Each glyph produces 4 vertices (quad corners) and 6 indices (two triangles).
/// UV coordinates map into the MSDF atlas texture.
#[must_use]
pub fn build_glyph_mesh(quads: &[GlyphQuadData]) -> bevy::prelude::Mesh {
    use bevy::mesh::Indices;
    use bevy::prelude::Mesh;
    use bevy::render::render_resource::PrimitiveTopology;

    let vertex_count = quads.len() * 4;
    let index_count = quads.len() * 6;

    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
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
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

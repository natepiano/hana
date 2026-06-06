/// Controls triangle winding order for mesh face generation.
#[derive(Clone, Copy, Debug)]
pub(super) enum WindingOrder {
    /// Standard winding, so normals point outward.
    Standard,
    /// Reversed winding, so normals point inward.
    Reversed,
}

/// Push a triangle with the requested winding.
pub(super) fn push_triangle(
    indices: &mut Vec<u32>,
    a: u32,
    b: u32,
    c: u32,
    winding_order: WindingOrder,
) {
    indices.push(a);
    if matches!(winding_order, WindingOrder::Reversed) {
        indices.push(c);
        indices.push(b);
    } else {
        indices.push(b);
        indices.push(c);
    }
}

/// Push a quad (two triangles) with the requested winding.
pub(super) fn push_quad(
    indices: &mut Vec<u32>,
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    winding_order: WindingOrder,
) {
    push_triangle(indices, a, b, c, winding_order);
    push_triangle(indices, b, d, c, winding_order);
}

/// Mutable references to the mesh attribute buffers being built.
pub(super) struct MeshBuffers<'a> {
    pub(super) positions: &'a mut Vec<[f32; 3]>,
    pub(super) normals:   &'a mut Vec<[f32; 3]>,
    pub(super) uvs:       &'a mut Vec<[f32; 2]>,
    pub(super) indices:   &'a mut Vec<u32>,
}

/// Extended mesh buffers that also track inside-face indices.
pub(super) struct TubeMeshBuffers<'a> {
    pub(super) mesh_buffers:   MeshBuffers<'a>,
    pub(super) inside_indices: &'a mut Vec<u32>,
}

use bevy::mesh::Indices;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::buffers;
use super::buffers::MeshBuffers;
use super::buffers::TubeMeshBuffers;
use super::buffers::WindingOrder;
use super::caps;
use super::config::CableMeshConfig;
use super::config::FaceSides;
use super::constants::MIN_TUBE_SIDES;
use super::elbows;
use super::frames;
use super::path;
use crate::routing::CableGeometry;
use crate::routing::MIN_SEGMENT_LENGTH;

/// Immutable path data for tube mesh generation.
struct TubePathData<'a> {
    points:      &'a [Vec3],
    tangents:    &'a [Vec3],
    arc_lengths: &'a [f32],
    frames:      &'a [(Vec3, Vec3)],
}

/// Generate cross-section rings along the path and connect them with triangles.
fn generate_tube_rings(
    path: &TubePathData,
    config: &CableMeshConfig,
    sides: u32,
    total_length: f32,
    out: &mut TubeMeshBuffers,
) {
    for (i, ((point, ..), (frame_normal, binormal))) in path
        .points
        .iter()
        .zip(path.tangents)
        .zip(path.frames)
        .enumerate()
    {
        let arc_u = path.arc_lengths[i] / total_length;

        for j in 0..sides {
            let angle = (j.to_f32() / sides.to_f32()) * std::f32::consts::TAU;
            let (sin_angle, cos_angle) = angle.sin_cos();
            let offset = *frame_normal * cos_angle * config.tube.radius
                + *binormal * sin_angle * config.tube.radius;
            let vertex_position = *point + offset;
            let vertex_normal = offset.normalize_or_zero();

            out.buffers.positions.push(vertex_position.to_array());
            out.buffers.normals.push(vertex_normal.to_array());
            out.buffers.uvs.push([arc_u, j.to_f32() / sides.to_f32()]);
        }

        if i > 0 {
            let base = ((i - 1) * sides.to_usize()).to_u32();
            let next_base = (i * sides.to_usize()).to_u32();

            for j in 0..sides {
                let next = (j + 1) % sides;
                let current = base + j;
                let current_next = base + next;
                let upcoming = next_base + j;
                let upcoming_next = next_base + next;

                match config.tube.faces {
                    FaceSides::Outside => buffers::push_quad(
                        out.buffers.indices,
                        current,
                        current_next,
                        upcoming,
                        upcoming_next,
                        WindingOrder::Standard,
                    ),
                    FaceSides::Inside => buffers::push_quad(
                        out.buffers.indices,
                        current,
                        current_next,
                        upcoming,
                        upcoming_next,
                        WindingOrder::Reversed,
                    ),
                    FaceSides::Both => {
                        buffers::push_quad(
                            out.buffers.indices,
                            current,
                            current_next,
                            upcoming,
                            upcoming_next,
                            WindingOrder::Standard,
                        );
                        buffers::push_quad(
                            out.inside_indices,
                            current,
                            current_next,
                            upcoming,
                            upcoming_next,
                            WindingOrder::Reversed,
                        );
                    },
                }
            }
        }
    }
}

/// For `Inside` or `Both` face sides, adjust normals for interior lighting.
fn apply_inside_normals(
    faces: &FaceSides,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    inside_indices: &mut Vec<u32>,
) {
    match faces {
        FaceSides::Outside => {},
        FaceSides::Inside => {
            for normal in &mut *normals {
                normal[0] = -normal[0];
                normal[1] = -normal[1];
                normal[2] = -normal[2];
            }
        },
        FaceSides::Both => {
            let original_vertex_count = positions.len().to_u32();
            let duplicated_positions = positions.clone();
            let duplicated_normals = normals
                .iter()
                .map(|n| [-n[0], -n[1], -n[2]])
                .collect::<Vec<_>>();
            let duplicated_uvs = uvs.clone();

            positions.extend(duplicated_positions);
            normals.extend(duplicated_normals);
            uvs.extend(duplicated_uvs);

            for index in &mut *inside_indices {
                *index += original_vertex_count;
            }
            indices.extend(inside_indices.iter());
        },
    }
}

/// All segments are flattened into a single continuous polyline.
#[must_use]
pub fn generate_tube_mesh(geometry: &CableGeometry, config: &CableMeshConfig) -> Mesh {
    let sides = config.tube.sides.max(MIN_TUBE_SIDES);
    let total_length = geometry.total_length.max(MIN_SEGMENT_LENGTH);

    let flat = path::flatten_geometry(geometry);
    let mut all_points = flat.points;
    let mut all_tangents = flat.tangents;
    let mut all_arc_lengths = flat.arc_lengths;

    if all_points.len() < 2 {
        return Mesh::new(PrimitiveTopology::TriangleList, default());
    }

    if config.trim.start > 0.0 || config.trim.end > 0.0 {
        path::trim_path(
            &mut all_points,
            &mut all_tangents,
            &mut all_arc_lengths,
            config.trim.start,
            config.trim.end,
        );
    }

    if all_points.len() < 2 {
        return Mesh::new(PrimitiveTopology::TriangleList, default());
    }

    let (all_points, all_tangents, all_arc_lengths) =
        elbows::insert_knee_rings(all_points, all_arc_lengths, config);
    let point_count = all_points.len();
    let frames = frames::compute_rmf(&all_points, &all_tangents);

    let mut positions = Vec::with_capacity(point_count * sides.to_usize());
    let mut normals = Vec::with_capacity(point_count * sides.to_usize());
    let mut uvs = Vec::with_capacity(point_count * sides.to_usize());
    let mut indices = Vec::new();
    let mut inside_indices = Vec::new();

    let path = TubePathData {
        points:      &all_points,
        tangents:    &all_tangents,
        arc_lengths: &all_arc_lengths,
        frames:      &frames,
    };
    generate_tube_rings(
        &path,
        config,
        sides,
        total_length,
        &mut TubeMeshBuffers {
            buffers:        MeshBuffers {
                positions: &mut positions,
                normals:   &mut normals,
                uvs:       &mut uvs,
                indices:   &mut indices,
            },
            inside_indices: &mut inside_indices,
        },
    );

    apply_inside_normals(
        &config.tube.faces,
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        &mut inside_indices,
    );

    let mut buffers = MeshBuffers {
        positions: &mut positions,
        normals:   &mut normals,
        uvs:       &mut uvs,
        indices:   &mut indices,
    };
    caps::add_end_caps(
        &all_points,
        &all_tangents,
        &frames,
        config,
        sides,
        point_count,
        &mut buffers,
    );

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

use std::f32::consts::TAU;

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
use super::config::Faces;
use super::constants::MIN_TUBE_SIDES;
use super::elbows;
use super::frames;
use super::path;
use crate::routing::CableGeometry;
use crate::routing::MIN_CABLE_SAMPLE_POINTS;
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
    tube_path_data: &TubePathData,
    cable_mesh_config: &CableMeshConfig,
    sides: u32,
    total_length: f32,
    out: &mut TubeMeshBuffers,
) {
    for (i, ((point, ..), (frame_normal, binormal))) in tube_path_data
        .points
        .iter()
        .zip(tube_path_data.tangents)
        .zip(tube_path_data.frames)
        .enumerate()
    {
        let arc_u = tube_path_data.arc_lengths[i] / total_length;

        for j in 0..sides {
            let angle = (j.to_f32() / sides.to_f32()) * TAU;
            let (sin_angle, cos_angle) = angle.sin_cos();
            let offset = *frame_normal * cos_angle * cable_mesh_config.tube_config.radius
                + *binormal * sin_angle * cable_mesh_config.tube_config.radius;
            let vertex_position = *point + offset;
            let vertex_normal = offset.normalize_or_zero();

            out.mesh_buffers.positions.push(vertex_position.to_array());
            out.mesh_buffers.normals.push(vertex_normal.to_array());
            out.mesh_buffers
                .uvs
                .push([arc_u, j.to_f32() / sides.to_f32()]);
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

                match cable_mesh_config.tube_config.faces {
                    Faces::Outside => buffers::push_quad(
                        out.mesh_buffers.indices,
                        current,
                        current_next,
                        upcoming,
                        upcoming_next,
                        WindingOrder::Standard,
                    ),
                    Faces::Inside => buffers::push_quad(
                        out.mesh_buffers.indices,
                        current,
                        current_next,
                        upcoming,
                        upcoming_next,
                        WindingOrder::Reversed,
                    ),
                    Faces::Both => {
                        buffers::push_quad(
                            out.mesh_buffers.indices,
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
    faces: &Faces,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    inside_indices: &mut Vec<u32>,
) {
    match faces {
        Faces::Outside => {},
        Faces::Inside => {
            for normal in &mut *normals {
                normal[0] = -normal[0];
                normal[1] = -normal[1];
                normal[2] = -normal[2];
            }
        },
        Faces::Both => {
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
pub fn generate_tube_mesh(geometry: &CableGeometry, cable_mesh_config: &CableMeshConfig) -> Mesh {
    let sides = cable_mesh_config.tube_config.sides.max(MIN_TUBE_SIDES);
    let total_length = geometry.total_length.max(MIN_SEGMENT_LENGTH);

    let flattened_geometry = path::flatten_geometry(geometry);
    let mut all_points = flattened_geometry.points;
    let mut all_tangents = flattened_geometry.tangents;
    let mut all_arc_lengths = flattened_geometry.arc_lengths;

    if all_points.len() < MIN_CABLE_SAMPLE_POINTS.to_usize() {
        return Mesh::new(PrimitiveTopology::TriangleList, default());
    }

    if cable_mesh_config.trim_config.start > 0.0 || cable_mesh_config.trim_config.end > 0.0 {
        path::trim_path(
            &mut all_points,
            &mut all_tangents,
            &mut all_arc_lengths,
            cable_mesh_config.trim_config.start,
            cable_mesh_config.trim_config.end,
        );
    }

    if all_points.len() < MIN_CABLE_SAMPLE_POINTS.to_usize() {
        return Mesh::new(PrimitiveTopology::TriangleList, default());
    }

    let (all_points, all_tangents, all_arc_lengths) =
        elbows::insert_knee_rings(all_points, all_arc_lengths, cable_mesh_config);
    let point_count = all_points.len();
    let frames = frames::compute_rotation_minimizing_frames(&all_points, &all_tangents);

    let mut positions = Vec::with_capacity(point_count * sides.to_usize());
    let mut normals = Vec::with_capacity(point_count * sides.to_usize());
    let mut uvs = Vec::with_capacity(point_count * sides.to_usize());
    let mut indices = Vec::new();
    let mut inside_indices = Vec::new();

    let tube_path_data = TubePathData {
        points:      &all_points,
        tangents:    &all_tangents,
        arc_lengths: &all_arc_lengths,
        frames:      &frames,
    };
    generate_tube_rings(
        &tube_path_data,
        cable_mesh_config,
        sides,
        total_length,
        &mut TubeMeshBuffers {
            mesh_buffers:   MeshBuffers {
                positions: &mut positions,
                normals:   &mut normals,
                uvs:       &mut uvs,
                indices:   &mut indices,
            },
            inside_indices: &mut inside_indices,
        },
    );

    apply_inside_normals(
        &cable_mesh_config.tube_config.faces,
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
        cable_mesh_config,
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

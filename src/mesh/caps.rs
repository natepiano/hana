use std::f32::consts::FRAC_PI_2;
use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::buffers;
use super::buffers::MeshBuffers;
use super::buffers::WindingOrder;
use super::config::CableMeshConfig;
use super::config::Capping;
use super::config::Faces;
use super::constants::MIN_CAP_RINGS;

/// Which side of a cap to generate.
#[derive(Clone, Copy, Debug)]
enum CapSide {
    Outside,
    Inside,
}

/// Geometric parameters shared by all cap-generation helpers.
struct CapContext<'a> {
    center:        &'a Vec3,
    direction:     Vec3,
    frame:         (Vec3, Vec3),
    radius:        f32,
    sides:         u32,
    ring_base:     u32,
    faces:         &'a Faces,
    winding_order: WindingOrder,
}

/// Add start and end caps to the tube mesh.
pub(super) fn add_end_caps(
    all_points: &[Vec3],
    all_tangents: &[Vec3],
    frames: &[(Vec3, Vec3)],
    config: &CableMeshConfig,
    sides: u32,
    point_count: usize,
    buffers: &mut MeshBuffers,
) {
    if point_count < 2 {
        return;
    }

    let render_inside = matches!(config.tube.faces, Faces::Inside);
    let start_winding_order = if render_inside {
        WindingOrder::Standard
    } else {
        WindingOrder::Reversed
    };
    let start_context = CapContext {
        center: &all_points[0],
        direction: -all_tangents[0],
        frame: frames[0],
        radius: config.tube.radius,
        sides,
        ring_base: 0,
        faces: &config.tube.faces,
        winding_order: start_winding_order,
    };
    add_single_cap(&config.caps.start, &start_context, buffers);

    let last = point_count - 1;
    let end_winding_order = if render_inside {
        WindingOrder::Reversed
    } else {
        WindingOrder::Standard
    };
    let end_context = CapContext {
        center: &all_points[last],
        direction: all_tangents[last],
        frame: frames[last],
        radius: config.tube.radius,
        sides,
        ring_base: (last * sides.to_usize()).to_u32(),
        faces: &config.tube.faces,
        winding_order: end_winding_order,
    };
    add_single_cap(&config.caps.end, &end_context, buffers);
}

/// Dispatch a single cap (start or end) based on style.
fn add_single_cap(style: &Capping, context: &CapContext, buffers: &mut MeshBuffers) {
    let needs_outside = matches!(context.faces, Faces::Outside | Faces::Both);
    let needs_inside = matches!(context.faces, Faces::Inside | Faces::Both);
    let cap_rings = context.sides.max(MIN_CAP_RINGS);

    match style {
        Capping::Round => {
            for &cap_side in &[CapSide::Outside, CapSide::Inside] {
                let needed = match cap_side {
                    CapSide::Outside => needs_outside,
                    CapSide::Inside => needs_inside,
                };
                if needed {
                    add_hemisphere_cap(context, cap_rings, cap_side, buffers);
                }
            }
        },
        Capping::Flat { normal } => {
            let flat_context = CapContext {
                direction: normal.unwrap_or(context.direction),
                ..*context
            };
            for &cap_side in &[CapSide::Outside, CapSide::Inside] {
                let needed = match cap_side {
                    CapSide::Outside => needs_outside,
                    CapSide::Inside => needs_inside,
                };
                if needed {
                    add_flat_cap(&flat_context, cap_side, buffers);
                }
            }
        },
        Capping::None => {},
    }
}

/// Add a hemisphere cap to the mesh, connecting to an existing tube ring.
fn add_hemisphere_cap(
    context: &CapContext,
    cap_rings: u32,
    cap_side: CapSide,
    buffers: &mut MeshBuffers,
) {
    let (frame_normal, binormal) = context.frame;
    let (normal_sign, winding_order) = match cap_side {
        CapSide::Outside => (1.0_f32, context.winding_order),
        CapSide::Inside => (-1.0_f32, WindingOrder::Standard),
    };

    let mut previous_ring_base = if matches!(cap_side, CapSide::Inside) {
        let base = buffers.positions.len().to_u32();
        for j in 0..context.sides {
            let original_index = (context.ring_base + j).to_usize();
            buffers.positions.push(buffers.positions[original_index]);
            let normal = buffers.normals[original_index];
            buffers.normals.push([-normal[0], -normal[1], -normal[2]]);
            buffers.uvs.push([0.5, 0.5]);
        }
        base
    } else {
        context.ring_base
    };

    for k in 1..cap_rings {
        let phi = (k.to_f32() / cap_rings.to_f32()) * FRAC_PI_2;
        let ring_radius = phi.cos() * context.radius;
        let along_offset = phi.sin() * context.radius;
        let ring_center = *context.center + context.direction * along_offset;
        let ring_base = buffers.positions.len().to_u32();

        for j in 0..context.sides {
            let angle = (j.to_f32() / context.sides.to_f32()) * TAU;
            let (sin_angle, cos_angle) = angle.sin_cos();
            let radial = frame_normal * cos_angle + binormal * sin_angle;
            let vertex_position = ring_center + radial * ring_radius;
            let vertex_normal =
                normal_sign * (radial * phi.cos() + context.direction * phi.sin()).normalize();

            buffers.positions.push(vertex_position.to_array());
            buffers.normals.push(vertex_normal.to_array());
            buffers.uvs.push([0.5, 0.5]);
        }

        for j in 0..context.sides {
            let next = (j + 1) % context.sides;
            buffers::push_quad(
                buffers.indices,
                previous_ring_base + j,
                previous_ring_base + next,
                ring_base + j,
                ring_base + next,
                winding_order,
            );
        }

        previous_ring_base = ring_base;
    }

    let pole_index = buffers.positions.len().to_u32();
    let pole_position = *context.center + context.direction * context.radius;
    buffers.positions.push(pole_position.to_array());
    buffers
        .normals
        .push((normal_sign * context.direction).to_array());
    buffers.uvs.push([0.5, 0.5]);

    for j in 0..context.sides {
        let next = (j + 1) % context.sides;
        buffers::push_triangle(
            buffers.indices,
            previous_ring_base + j,
            previous_ring_base + next,
            pole_index,
            winding_order,
        );
    }
}

/// Add a flat disc cap to the mesh, connecting to an existing tube ring.
fn add_flat_cap(context: &CapContext, cap_side: CapSide, buffers: &mut MeshBuffers) {
    let (cap_normal, winding_order) = match cap_side {
        CapSide::Outside => (context.direction.to_array(), context.winding_order),
        CapSide::Inside => ((-context.direction).to_array(), WindingOrder::Reversed),
    };

    let new_ring_base = buffers.positions.len().to_u32();
    for j in 0..context.sides {
        let original_index = (context.ring_base + j).to_usize();
        buffers.positions.push(buffers.positions[original_index]);
        buffers.normals.push(cap_normal);
        buffers.uvs.push([j.to_f32() / context.sides.to_f32(), 0.0]);
    }

    let center_index = buffers.positions.len().to_u32();
    buffers.positions.push(context.center.to_array());
    buffers.normals.push(cap_normal);
    buffers.uvs.push([0.5, 0.5]);

    for j in 0..context.sides {
        let next = (j + 1) % context.sides;
        buffers::push_triangle(
            buffers.indices,
            new_ring_base + j,
            new_ring_base + next,
            center_index,
            winding_order,
        );
    }
}

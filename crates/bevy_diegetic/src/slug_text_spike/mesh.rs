use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::Mesh;
use bevy::render::render_resource::PrimitiveTopology;
use bevy_kana::ToU32;

use super::geometry::QuadraticSegment;
use super::geometry::SlugGlyph;
use bevy::math::Vec2;

const QUADRATIC_SAMPLE_STEPS: u16 = 10;

/// Builds a debug line mesh from a Slug feasibility glyph.
///
/// This is only a visualization helper. It samples each quadratic segment
/// into line segments so the standalone example can show whether outline
/// extraction and line-to-quadratic conversion are working.
#[must_use]
pub fn build_outline_mesh(glyph: &SlugGlyph, scale: f32) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    for contour in &glyph.contours {
        for segment in &contour.segments {
            append_sampled_segment(segment, scale, &mut positions, &mut indices);
        }
    }

    Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
        .with_inserted_indices(Indices::U32(indices))
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
}

fn append_sampled_segment(
    segment: &QuadraticSegment,
    scale: f32,
    positions: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let mut previous = segment.start;
    for step in 1..=QUADRATIC_SAMPLE_STEPS {
        let t = f32::from(step) / f32::from(QUADRATIC_SAMPLE_STEPS);
        let point = sample_quadratic(segment, t);
        let base = positions.len().to_u32();
        positions.push([previous.x * scale, previous.y * scale, 0.0]);
        positions.push([point.x * scale, point.y * scale, 0.0]);
        indices.push(base);
        indices.push(base + 1);
        previous = point;
    }
}

fn sample_quadratic(segment: &QuadraticSegment, t: f32) -> Vec2 {
    let inverse_t = 1.0 - t;
    let start_weight = inverse_t * inverse_t;
    let control_weight = 2.0 * inverse_t * t;
    let end_weight = t * t;
    start_weight * segment.start + control_weight * segment.control + end_weight * segment.end
}

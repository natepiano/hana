//! Shared geometry fixtures for `hana_valence` examples and tests.
//!
//! Included into each example, the integration test, and the `resolve` unit
//! tests with `#[path = "../fixtures.rs"] mod fixtures;`, so the same anchor
//! geometry is authored once. Paths reference the crate by name
//! (`hana_valence::`), which resolves for the external consumers directly and
//! for the crate's own test build through `extern crate self as hana_valence;`
//! in `lib.rs`.

use bevy_math::Vec3;
use bevy_platform::collections::HashMap;
use hana_valence::AnchorId;
use hana_valence::AnchorPoint;
use hana_valence::Edge;
use hana_valence::ResolvedAnchorGeometry;

const TRIANGLE_HEIGHT: f32 = 0.866_025_4;
const TRIANGLE_HALF_SIDE: f32 = 0.5;
const TRIANGLE_TWO_THIRDS: f32 = 2.0 / 3.0;

/// Top edge of [`quad_geometry`].
pub const QUAD_TOP_EDGE: Edge = Edge {
    start: AnchorId::Vertex(0),
    end:   AnchorId::Vertex(1),
};

/// Right edge of [`quad_geometry`].
pub const QUAD_RIGHT_EDGE: Edge = Edge {
    start: AnchorId::Vertex(1),
    end:   AnchorId::Vertex(2),
};

/// Bottom edge of [`quad_geometry`].
pub const QUAD_BOTTOM_EDGE: Edge = Edge {
    start: AnchorId::Vertex(2),
    end:   AnchorId::Vertex(3),
};

/// Left edge of [`quad_geometry`].
pub const QUAD_LEFT_EDGE: Edge = Edge {
    start: AnchorId::Vertex(3),
    end:   AnchorId::Vertex(0),
};

/// Right edge of [`triangle_geometry`].
pub const TRIANGLE_RIGHT_EDGE: Edge = Edge {
    start: AnchorId::Vertex(0),
    end:   AnchorId::Vertex(1),
};

/// Bottom edge of [`triangle_geometry`].
pub const TRIANGLE_BOTTOM_EDGE: Edge = Edge {
    start: AnchorId::Vertex(1),
    end:   AnchorId::Vertex(2),
};

/// Left edge of [`triangle_geometry`].
pub const TRIANGLE_LEFT_EDGE: Edge = Edge {
    start: AnchorId::Vertex(2),
    end:   AnchorId::Vertex(0),
};

/// Equilateral triangle anchor geometry with unit side length.
#[must_use]
pub fn triangle_geometry() -> ResolvedAnchorGeometry {
    let vertices = triangle_vertices();
    ResolvedAnchorGeometry {
        points: HashMap::from_iter([
            anchor_point(AnchorId::Vertex(0), vertices[0]),
            anchor_point(AnchorId::Vertex(1), vertices[1]),
            anchor_point(AnchorId::Vertex(2), vertices[2]),
            anchor_point(
                AnchorId::EdgeMid(0),
                edge_midpoint(TRIANGLE_RIGHT_EDGE, vertices),
            ),
            anchor_point(
                AnchorId::EdgeMid(1),
                edge_midpoint(TRIANGLE_BOTTOM_EDGE, vertices),
            ),
            anchor_point(
                AnchorId::EdgeMid(2),
                edge_midpoint(TRIANGLE_LEFT_EDGE, vertices),
            ),
            anchor_point(AnchorId::Center, Vec3::ZERO),
        ]),
        edges:  vec![
            TRIANGLE_RIGHT_EDGE,
            TRIANGLE_BOTTOM_EDGE,
            TRIANGLE_LEFT_EDGE,
        ],
    }
}

/// Axis-aligned rectangle anchor geometry centered at the origin in the XY plane.
#[must_use]
pub fn quad_geometry(width: f32, height: f32) -> ResolvedAnchorGeometry {
    let vertices = quad_vertices(width, height);
    ResolvedAnchorGeometry {
        points: HashMap::from_iter([
            anchor_point(AnchorId::Vertex(0), vertices[0]),
            anchor_point(AnchorId::Vertex(1), vertices[1]),
            anchor_point(AnchorId::Vertex(2), vertices[2]),
            anchor_point(AnchorId::Vertex(3), vertices[3]),
            anchor_point(AnchorId::EdgeMid(0), edge_midpoint(QUAD_TOP_EDGE, vertices)),
            anchor_point(
                AnchorId::EdgeMid(1),
                edge_midpoint(QUAD_RIGHT_EDGE, vertices),
            ),
            anchor_point(
                AnchorId::EdgeMid(2),
                edge_midpoint(QUAD_BOTTOM_EDGE, vertices),
            ),
            anchor_point(
                AnchorId::EdgeMid(3),
                edge_midpoint(QUAD_LEFT_EDGE, vertices),
            ),
            anchor_point(AnchorId::Center, Vec3::ZERO),
        ]),
        edges:  vec![
            QUAD_TOP_EDGE,
            QUAD_RIGHT_EDGE,
            QUAD_BOTTOM_EDGE,
            QUAD_LEFT_EDGE,
        ],
    }
}

/// Returns the midpoint anchor id for a quad edge.
#[must_use]
pub fn quad_edge_anchor(edge: Edge) -> Option<AnchorId> {
    if edge == QUAD_TOP_EDGE {
        Some(AnchorId::EdgeMid(0))
    } else if edge == QUAD_RIGHT_EDGE {
        Some(AnchorId::EdgeMid(1))
    } else if edge == QUAD_BOTTOM_EDGE {
        Some(AnchorId::EdgeMid(2))
    } else if edge == QUAD_LEFT_EDGE {
        Some(AnchorId::EdgeMid(3))
    } else {
        None
    }
}

/// Returns the midpoint anchor id for a triangle edge.
#[must_use]
pub fn triangle_edge_anchor(edge: Edge) -> Option<AnchorId> {
    if edge == TRIANGLE_RIGHT_EDGE {
        Some(AnchorId::EdgeMid(0))
    } else if edge == TRIANGLE_BOTTOM_EDGE {
        Some(AnchorId::EdgeMid(1))
    } else if edge == TRIANGLE_LEFT_EDGE {
        Some(AnchorId::EdgeMid(2))
    } else {
        None
    }
}

/// Alternating strip edge selector for triangle tiling.
#[must_use]
pub const fn triangle_edge(index: usize) -> Edge {
    match index % 3 {
        1 => TRIANGLE_BOTTOM_EDGE,
        2 => TRIANGLE_RIGHT_EDGE,
        _ => TRIANGLE_LEFT_EDGE,
    }
}

const fn anchor_point(anchor_id: AnchorId, position: Vec3) -> (AnchorId, AnchorPoint) {
    (
        anchor_id,
        AnchorPoint {
            position,
            frame: None,
        },
    )
}

const fn triangle_vertices() -> [Vec3; 3] {
    [
        Vec3::new(0.0, TRIANGLE_HEIGHT * TRIANGLE_TWO_THIRDS, 0.0),
        Vec3::new(TRIANGLE_HALF_SIDE, -TRIANGLE_HEIGHT / 3.0, 0.0),
        Vec3::new(-TRIANGLE_HALF_SIDE, -TRIANGLE_HEIGHT / 3.0, 0.0),
    ]
}

const fn quad_vertices(width: f32, height: f32) -> [Vec3; 4] {
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    [
        Vec3::new(-half_width, half_height, 0.0),
        Vec3::new(half_width, half_height, 0.0),
        Vec3::new(half_width, -half_height, 0.0),
        Vec3::new(-half_width, -half_height, 0.0),
    ]
}

fn edge_midpoint<const N: usize>(edge: Edge, vertices: [Vec3; N]) -> Vec3 {
    let Some(start) = vertex_position(edge.start, vertices) else {
        return Vec3::ZERO;
    };
    let Some(end) = vertex_position(edge.end, vertices) else {
        return Vec3::ZERO;
    };
    (start + end) / 2.0
}

fn vertex_position<const N: usize>(anchor_id: AnchorId, vertices: [Vec3; N]) -> Option<Vec3> {
    match anchor_id {
        AnchorId::Vertex(index) => usize::try_from(index)
            .ok()
            .and_then(|index| vertices.get(index).copied()),
        _ => None,
    }
}

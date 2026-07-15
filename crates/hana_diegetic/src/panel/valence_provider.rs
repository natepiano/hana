//! Provider that writes `hana_valence` anchor geometry for diegetic panels.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use hana_valence::AnchorId;
use hana_valence::AnchorPoint;
use hana_valence::Edge;
use hana_valence::ResolvedAnchorGeometry;

use super::CoordinateSpace;
use super::DiegeticPanel;
use crate::layout::Anchor;

const BOTTOM_CENTER_EDGE: u32 = 2;
const BOTTOM_LEFT_VERTEX: u32 = 3;
const BOTTOM_RIGHT_VERTEX: u32 = 2;
const CENTER_LEFT_EDGE: u32 = 3;
const CENTER_RIGHT_EDGE: u32 = 1;
const QUAD_ANCHOR_COUNT: usize = 9;
const QUAD_EDGE_COUNT: usize = 4;
const TOP_CENTER_EDGE: u32 = 0;
const TOP_LEFT_VERTEX: u32 = 0;
const TOP_RIGHT_VERTEX: u32 = 1;

impl From<Anchor> for AnchorId {
    fn from(anchor: Anchor) -> Self {
        match anchor {
            Anchor::TopLeft => Self::Vertex(TOP_LEFT_VERTEX),
            Anchor::TopRight => Self::Vertex(TOP_RIGHT_VERTEX),
            Anchor::BottomRight => Self::Vertex(BOTTOM_RIGHT_VERTEX),
            Anchor::BottomLeft => Self::Vertex(BOTTOM_LEFT_VERTEX),
            Anchor::TopCenter => Self::EdgeMid(TOP_CENTER_EDGE),
            Anchor::CenterRight => Self::EdgeMid(CENTER_RIGHT_EDGE),
            Anchor::BottomCenter => Self::EdgeMid(BOTTOM_CENTER_EDGE),
            Anchor::CenterLeft => Self::EdgeMid(CENTER_LEFT_EDGE),
            Anchor::Center => Self::Center,
        }
    }
}

impl TryFrom<AnchorId> for Anchor {
    type Error = AnchorId;

    fn try_from(anchor_id: AnchorId) -> Result<Self, Self::Error> {
        match anchor_id {
            AnchorId::Vertex(TOP_LEFT_VERTEX) => Ok(Self::TopLeft),
            AnchorId::Vertex(TOP_RIGHT_VERTEX) => Ok(Self::TopRight),
            AnchorId::Vertex(BOTTOM_RIGHT_VERTEX) => Ok(Self::BottomRight),
            AnchorId::Vertex(BOTTOM_LEFT_VERTEX) => Ok(Self::BottomLeft),
            AnchorId::EdgeMid(TOP_CENTER_EDGE) => Ok(Self::TopCenter),
            AnchorId::EdgeMid(CENTER_RIGHT_EDGE) => Ok(Self::CenterRight),
            AnchorId::EdgeMid(BOTTOM_CENTER_EDGE) => Ok(Self::BottomCenter),
            AnchorId::EdgeMid(CENTER_LEFT_EDGE) => Ok(Self::CenterLeft),
            AnchorId::Center => Ok(Self::Center),
            unmapped => Err(unmapped),
        }
    }
}

pub(super) fn write_panel_anchor_geometry(
    mut commands: Commands,
    mut panels: Query<
        (Entity, &DiegeticPanel, Option<&mut ResolvedAnchorGeometry>),
        Changed<DiegeticPanel>,
    >,
) {
    for (entity, panel, geometry) in &mut panels {
        if !matches!(panel.coordinate_space(), CoordinateSpace::World { .. }) {
            continue;
        }

        if let Some(mut geometry) = geometry {
            let size = Vec2::new(panel.world_width(), panel.world_height());
            write_geometry(&mut geometry, size, panel.anchor());
        } else {
            commands.entity(entity).insert(panel_anchor_geometry(panel));
        }
    }
}

/// Builds fresh quad anchor geometry for `panel` in its local frame.
pub(super) fn panel_anchor_geometry(panel: &DiegeticPanel) -> ResolvedAnchorGeometry {
    let mut geometry = ResolvedAnchorGeometry {
        points: HashMap::default(),
        edges:  Vec::with_capacity(QUAD_EDGE_COUNT),
    };
    geometry.points.reserve(QUAD_ANCHOR_COUNT);
    write_geometry(
        &mut geometry,
        Vec2::new(panel.world_width(), panel.world_height()),
        panel.anchor(),
    );
    geometry
}

fn write_geometry(geometry: &mut ResolvedAnchorGeometry, size: Vec2, panel_anchor: Anchor) {
    let anchor_ids = quad_anchor_ids();
    geometry
        .points
        .retain(|anchor_id, _| anchor_ids.contains(anchor_id));
    for (anchor, position) in quad_anchor_points(size, panel_anchor) {
        let point = AnchorPoint {
            position,
            frame: None,
        };
        let anchor_id = AnchorId::from(anchor);
        if let Some(existing) = geometry.points.get_mut(&anchor_id) {
            *existing = point;
        } else {
            geometry.points.insert(anchor_id, point);
        }
    }

    for (index, edge) in quad_edges().into_iter().enumerate() {
        if let Some(existing) = geometry.edges.get_mut(index) {
            *existing = edge;
        } else {
            geometry.edges.push(edge);
        }
    }
    geometry.edges.truncate(QUAD_EDGE_COUNT);
}

fn quad_anchor_ids() -> [AnchorId; QUAD_ANCHOR_COUNT] {
    [
        AnchorId::from(Anchor::TopLeft),
        AnchorId::from(Anchor::TopRight),
        AnchorId::from(Anchor::BottomRight),
        AnchorId::from(Anchor::BottomLeft),
        AnchorId::from(Anchor::TopCenter),
        AnchorId::from(Anchor::CenterRight),
        AnchorId::from(Anchor::BottomCenter),
        AnchorId::from(Anchor::CenterLeft),
        AnchorId::from(Anchor::Center),
    ]
}

fn quad_anchor_points(size: Vec2, panel_anchor: Anchor) -> [(Anchor, Vec3); QUAD_ANCHOR_COUNT] {
    let panel_offset = anchor_offset(panel_anchor, size);
    [
        (
            Anchor::TopLeft,
            anchor_position(Anchor::TopLeft, size, panel_offset),
        ),
        (
            Anchor::TopRight,
            anchor_position(Anchor::TopRight, size, panel_offset),
        ),
        (
            Anchor::BottomRight,
            anchor_position(Anchor::BottomRight, size, panel_offset),
        ),
        (
            Anchor::BottomLeft,
            anchor_position(Anchor::BottomLeft, size, panel_offset),
        ),
        (
            Anchor::TopCenter,
            anchor_position(Anchor::TopCenter, size, panel_offset),
        ),
        (
            Anchor::CenterRight,
            anchor_position(Anchor::CenterRight, size, panel_offset),
        ),
        (
            Anchor::BottomCenter,
            anchor_position(Anchor::BottomCenter, size, panel_offset),
        ),
        (
            Anchor::CenterLeft,
            anchor_position(Anchor::CenterLeft, size, panel_offset),
        ),
        (
            Anchor::Center,
            anchor_position(Anchor::Center, size, panel_offset),
        ),
    ]
}

fn anchor_position(anchor: Anchor, size: Vec2, panel_offset: Vec2) -> Vec3 {
    let anchor_offset = anchor_offset(anchor, size);
    Vec3::new(
        anchor_offset.x - panel_offset.x,
        panel_offset.y - anchor_offset.y,
        0.0,
    )
}

fn anchor_offset(anchor: Anchor, size: Vec2) -> Vec2 {
    let (x, y) = anchor.offset(size.x, size.y);
    Vec2::new(x, y)
}

const fn quad_edges() -> [Edge; QUAD_EDGE_COUNT] {
    [
        Edge {
            start: AnchorId::Vertex(TOP_LEFT_VERTEX),
            end:   AnchorId::Vertex(TOP_RIGHT_VERTEX),
        },
        Edge {
            start: AnchorId::Vertex(TOP_RIGHT_VERTEX),
            end:   AnchorId::Vertex(BOTTOM_RIGHT_VERTEX),
        },
        Edge {
            start: AnchorId::Vertex(BOTTOM_RIGHT_VERTEX),
            end:   AnchorId::Vertex(BOTTOM_LEFT_VERTEX),
        },
        Edge {
            start: AnchorId::Vertex(BOTTOM_LEFT_VERTEX),
            end:   AnchorId::Vertex(TOP_LEFT_VERTEX),
        },
    ]
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::ecs::schedule::Schedule;
    use bevy::ecs::world::World;
    use bevy::prelude::Entity;
    use bevy::prelude::Vec3;
    use hana_valence::AnchorId;
    use hana_valence::ResolvedAnchorGeometry;

    use super::quad_edges;
    use super::write_panel_anchor_geometry;
    use crate::layout::Anchor;
    use crate::panel::DiegeticPanel;

    const EXPECTED_EDGE_COUNT: usize = 4;
    const EXPECTED_POINT_COUNT: usize = 9;
    const GEOMETRY_EPSILON: f32 = 1e-5;
    const HALF_EXTENT_FACTOR: f32 = 0.5;
    const PANEL_HEIGHT: f32 = 1.0;
    const PANEL_WIDTH: f32 = 2.0;
    const RESIZED_PANEL_HEIGHT: f32 = 2.0;
    const RESIZED_PANEL_WIDTH: f32 = 4.0;
    const UNMAPPED_VERTEX: u32 = 99;

    #[test]
    fn spawned_world_panel_gets_local_quad_geometry() {
        let mut world = World::new();
        let panel = world.spawn(world_panel()).id();
        let mut schedule = provider_schedule();

        schedule.run(&mut world);

        let geometry = geometry(&world, panel);
        assert_eq!(geometry.points.len(), EXPECTED_POINT_COUNT);
        assert_eq!(geometry.edges.len(), EXPECTED_EDGE_COUNT);
        assert_eq!(geometry.validate(), Ok(()));
        assert_expected_positions(geometry, PANEL_WIDTH, PANEL_HEIGHT);
        for point in geometry.points.values() {
            assert_eq!(point.frame, None);
        }
        assert_eq!(geometry.edges.as_slice(), quad_edges().as_slice());
    }

    #[test]
    fn resizing_panel_updates_existing_geometry() {
        let mut world = World::new();
        let panel = world.spawn(world_panel()).id();
        let mut schedule = provider_schedule();
        schedule.run(&mut world);

        let initial_keys = sorted_anchor_ids(geometry(&world, panel));
        let initial_capacity = geometry(&world, panel).points.capacity();
        let initial_points_address = points_address(&world, panel);
        world
            .get_mut::<DiegeticPanel>(panel)
            .expect("panel exists")
            .set_width(RESIZED_PANEL_WIDTH);
        world
            .get_mut::<DiegeticPanel>(panel)
            .expect("panel exists")
            .set_height(RESIZED_PANEL_HEIGHT);

        schedule.run(&mut world);

        let geometry = geometry(&world, panel);
        assert_eq!(sorted_anchor_ids(geometry), initial_keys);
        assert_eq!(geometry.points.capacity(), initial_capacity);
        assert_eq!(points_address(&world, panel), initial_points_address);
        assert_expected_positions(geometry, RESIZED_PANEL_WIDTH, RESIZED_PANEL_HEIGHT);
        assert_eq!(geometry.validate(), Ok(()));
    }

    #[test]
    fn anchor_id_mapping_round_trips_known_panel_anchors() {
        for anchor in all_anchors() {
            let anchor_id = AnchorId::from(anchor);

            assert_eq!(Anchor::try_from(anchor_id), Ok(anchor));
        }

        let unmapped = AnchorId::Vertex(UNMAPPED_VERTEX);
        assert_eq!(Anchor::try_from(unmapped), Err(unmapped));
    }

    fn provider_schedule() -> Schedule {
        let mut schedule = Schedule::default();
        schedule.add_systems(write_panel_anchor_geometry);
        schedule
    }

    fn world_panel() -> DiegeticPanel {
        DiegeticPanel::world()
            .size(PANEL_WIDTH, PANEL_HEIGHT)
            .anchor(Anchor::TopLeft)
            .layout(|_| {})
            .build()
            .expect("world panel builds")
    }

    fn geometry(world: &World, entity: Entity) -> &ResolvedAnchorGeometry {
        world
            .get::<ResolvedAnchorGeometry>(entity)
            .expect("panel has valence geometry")
    }

    fn points_address(world: &World, entity: Entity) -> usize {
        std::ptr::addr_of!(geometry(world, entity).points) as usize
    }

    fn sorted_anchor_ids(geometry: &ResolvedAnchorGeometry) -> Vec<AnchorId> {
        let mut anchor_ids: Vec<_> = geometry.points.keys().copied().collect();
        anchor_ids.sort_by_key(|anchor_id| anchor_sort_key(*anchor_id));
        anchor_ids
    }

    fn anchor_sort_key(anchor_id: AnchorId) -> (u8, u32) {
        match anchor_id {
            AnchorId::Vertex(index) => (0, index),
            AnchorId::EdgeMid(index) => (1, index),
            AnchorId::Center => (2, 0),
            _ => (3, 0),
        }
    }

    fn assert_expected_positions(
        geometry: &ResolvedAnchorGeometry,
        expected_width: f32,
        expected_height: f32,
    ) {
        let cases = [
            (Anchor::TopLeft, Vec3::ZERO),
            (Anchor::TopRight, Vec3::new(expected_width, 0.0, 0.0)),
            (
                Anchor::BottomRight,
                Vec3::new(expected_width, -expected_height, 0.0),
            ),
            (Anchor::BottomLeft, Vec3::new(0.0, -expected_height, 0.0)),
            (
                Anchor::TopCenter,
                Vec3::new(expected_width * HALF_EXTENT_FACTOR, 0.0, 0.0),
            ),
            (
                Anchor::CenterRight,
                Vec3::new(expected_width, -expected_height * HALF_EXTENT_FACTOR, 0.0),
            ),
            (
                Anchor::BottomCenter,
                Vec3::new(expected_width * HALF_EXTENT_FACTOR, -expected_height, 0.0),
            ),
            (
                Anchor::CenterLeft,
                Vec3::new(0.0, -expected_height * HALF_EXTENT_FACTOR, 0.0),
            ),
            (
                Anchor::Center,
                Vec3::new(
                    expected_width * HALF_EXTENT_FACTOR,
                    -expected_height * HALF_EXTENT_FACTOR,
                    0.0,
                ),
            ),
        ];

        for (anchor, expected) in cases {
            let point = geometry
                .points
                .get(&AnchorId::from(anchor))
                .expect("anchor point exists");
            assert!(
                point.position.abs_diff_eq(expected, GEOMETRY_EPSILON),
                "expected {expected:?}, got {:?}",
                point.position,
            );
        }
    }

    fn all_anchors() -> [Anchor; EXPECTED_POINT_COUNT] {
        [
            Anchor::TopLeft,
            Anchor::TopCenter,
            Anchor::TopRight,
            Anchor::CenterLeft,
            Anchor::Center,
            Anchor::CenterRight,
            Anchor::BottomLeft,
            Anchor::BottomCenter,
            Anchor::BottomRight,
        ]
    }
}

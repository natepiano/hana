use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy_catenary::CatenarySolver;
use bevy_catenary::CurveKind;
use bevy_catenary::DEFAULT_SLACK;
use bevy_catenary::Obstacle;
use bevy_catenary::PathStrategy;
use bevy_catenary::Solver;
use bevy_kana::Position;

use super::constants::ASTAR_OBSTACLE_SIZE_MULTIPLIER;
use super::constants::ASTAR_SECTION_Z;
use crate::constants::ASTAR_SECTION_INDEX;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::NODE_Y;
use crate::constants::OBSTACLE_COLOR;
use crate::constants::OBSTACLE_HALF_EXTENTS;
use crate::constants::SECTION_X;
use crate::constants::SPAN_HALF_X;
use crate::entities;
use crate::input;

/// Section 5: A* pathfinding around an obstacle.
pub(super) fn setup_section_astar(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    cable_material: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[ASTAR_SECTION_INDEX];
    let start = Vec3::new(section_center_x - SPAN_HALF_X, NODE_Y, ASTAR_SECTION_Z);
    let end = Vec3::new(section_center_x + SPAN_HALF_X, NODE_Y, ASTAR_SECTION_Z);
    let obstacle_position = Position::new(section_center_x, NODE_Y, ASTAR_SECTION_Z);
    let obstacle = Obstacle::new(OBSTACLE_HALF_EXTENTS, obstacle_position);

    entities::spawn_node_pair(commands, node_mesh, node_material, start, end);
    entities::spawn_cable(
        commands,
        start,
        end,
        Solver::Routed {
            path_strategy: PathStrategy::AStar,
            curve_kind:    CurveKind::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            resolution:    DEFAULT_CABLE_RESOLUTION,
        },
        vec![obstacle],
        cable_material,
    );

    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(
                OBSTACLE_HALF_EXTENTS.x * ASTAR_OBSTACLE_SIZE_MULTIPLIER,
                OBSTACLE_HALF_EXTENTS.y * ASTAR_OBSTACLE_SIZE_MULTIPLIER,
                OBSTACLE_HALF_EXTENTS.z * ASTAR_OBSTACLE_SIZE_MULTIPLIER,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: OBSTACLE_COLOR,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_translation(*obstacle_position),
            NotShadowCaster,
        ))
        .observe(input::on_mesh_clicked);
}

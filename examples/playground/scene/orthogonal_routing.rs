use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy_catenary::AttachedTo;
use bevy_catenary::Cable;
use bevy_catenary::CableEnd;
use bevy_catenary::CableEndpoint;
use bevy_catenary::CableMeshConfig;
use bevy_catenary::CurveKind;
use bevy_catenary::Obstacle;
use bevy_catenary::PathStrategy;
use bevy_catenary::Solver;

use super::constants::ORTHOGONAL_ROUTING_END_Z;
use super::constants::ORTHOGONAL_ROUTING_OBSTACLE_HALF_EXTENTS;
use super::constants::ORTHOGONAL_ROUTING_OBSTACLE_OFFSETS;
use super::constants::ORTHOGONAL_ROUTING_OBSTACLE_SIZE_MULTIPLIER;
use super::constants::ORTHOGONAL_ROUTING_START_Z;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::NODE_Y;
use crate::constants::OBSTACLE_COLOR;
use crate::constants::ORTHOGONAL_ROUTING_SECTION_INDEX;
use crate::constants::SECTION_X;
use crate::constants::SPAN_HALF_X;
use crate::entities;
use crate::entities::Draggable;
use crate::input;

#[derive(Component)]
pub(crate) struct MovableRoutingObstacle {
    cable:        Entity,
    half_extents: Vec3,
}

/// Section 5: orthogonal routing around obstacles.
pub(super) fn setup_section_orthogonal_routing(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    cable_material: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[ORTHOGONAL_ROUTING_SECTION_INDEX];
    let start = Vec3::new(
        section_center_x - SPAN_HALF_X,
        NODE_Y,
        ORTHOGONAL_ROUTING_START_Z,
    );
    let end = Vec3::new(
        section_center_x + SPAN_HALF_X,
        NODE_Y,
        ORTHOGONAL_ROUTING_END_Z,
    );
    let obstacle_positions = ORTHOGONAL_ROUTING_OBSTACLE_OFFSETS
        .map(|offset| Vec3::new(section_center_x + offset.x, NODE_Y + offset.y, offset.z));
    let obstacles = obstacle_positions
        .iter()
        .copied()
        .map(|position| Obstacle::new(ORTHOGONAL_ROUTING_OBSTACLE_HALF_EXTENTS, position))
        .collect();

    let start_node = entities::spawn_node_cube(commands, node_mesh, node_material, start)
        .insert(Draggable)
        .observe(input::on_drag_start)
        .id();
    let end_node = entities::spawn_node_cube(commands, node_mesh, node_material, end)
        .insert(Draggable)
        .observe(input::on_drag_start)
        .id();

    let cable = commands
        .spawn((
            Cable {
                solver: Solver::Routed {
                    path_strategy: PathStrategy::Orthogonal,
                    curve_kind:    CurveKind::Linear,
                    resolution:    DEFAULT_CABLE_RESOLUTION,
                },
                obstacles,
                resolution: DEFAULT_CABLE_RESOLUTION,
            },
            CableMeshConfig {
                material: Some(cable_material.clone()),
                ..default()
            },
        ))
        .id();

    commands.entity(cable).with_children(|parent| {
        parent.spawn((
            CableEndpoint::new(CableEnd::Start, Vec3::ZERO),
            AttachedTo(start_node),
        ));
        parent.spawn((
            CableEndpoint::new(CableEnd::End, Vec3::ZERO),
            AttachedTo(end_node),
        ));
    });

    for obstacle_position in obstacle_positions {
        commands
            .spawn((
                Mesh3d(meshes.add(Cuboid::new(
                    ORTHOGONAL_ROUTING_OBSTACLE_HALF_EXTENTS.x
                        * ORTHOGONAL_ROUTING_OBSTACLE_SIZE_MULTIPLIER,
                    ORTHOGONAL_ROUTING_OBSTACLE_HALF_EXTENTS.y
                        * ORTHOGONAL_ROUTING_OBSTACLE_SIZE_MULTIPLIER,
                    ORTHOGONAL_ROUTING_OBSTACLE_HALF_EXTENTS.z
                        * ORTHOGONAL_ROUTING_OBSTACLE_SIZE_MULTIPLIER,
                ))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: OBSTACLE_COLOR,
                    ..default()
                })),
                Transform::from_translation(obstacle_position),
                NotShadowCaster,
                Draggable,
                MovableRoutingObstacle {
                    cable,
                    half_extents: ORTHOGONAL_ROUTING_OBSTACLE_HALF_EXTENTS,
                },
            ))
            .observe(input::on_drag_start)
            .observe(input::on_mesh_clicked);
    }
}

pub(crate) fn sync_movable_obstacles(
    changed_obstacles: Query<(), (With<MovableRoutingObstacle>, Changed<Transform>)>,
    obstacle_entities: Query<(&Transform, &MovableRoutingObstacle)>,
    mut cables: Query<&mut Cable>,
) {
    if changed_obstacles.is_empty() {
        return;
    }

    let mut obstacle_groups: Vec<(Entity, Vec<Obstacle>)> = Vec::new();
    for (transform, movable_obstacle) in &obstacle_entities {
        let obstacle = Obstacle::new(movable_obstacle.half_extents, transform.translation);
        if let Some((_, obstacles)) = obstacle_groups
            .iter_mut()
            .find(|(cable, _)| *cable == movable_obstacle.cable)
        {
            obstacles.push(obstacle);
        } else {
            obstacle_groups.push((movable_obstacle.cable, vec![obstacle]));
        }
    }

    for (cable_entity, obstacles) in obstacle_groups {
        if let Ok(mut cable) = cables.get_mut(cable_entity) {
            cable.obstacles = obstacles;
        }
    }
}

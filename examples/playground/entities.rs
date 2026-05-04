//! Shared markers and spawn helpers used across the playground sections.

use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use bevy_catenary::Cable;
use bevy_catenary::CableEnd;
use bevy_catenary::CableEndpoint;
use bevy_catenary::CableMeshConfig;
use bevy_catenary::Obstacle;
use bevy_catenary::Solver;

use super::constants::DEFAULT_CABLE_RESOLUTION;
use super::input;

#[derive(Component)]
pub(crate) struct Selected;

#[derive(Component)]
pub(crate) struct NodeCube;

#[derive(Component)]
pub(crate) struct Draggable;

#[derive(Component)]
pub(crate) struct Despawnable;

/// Marker to exclude a cable from global +/- slack adjustment.
#[derive(Component)]
pub(crate) struct SlackLocked;

pub(crate) fn spawn_cable(
    commands: &mut Commands,
    start: Vec3,
    end: Vec3,
    solver: Solver,
    obstacles: Vec<Obstacle>,
    material: &Handle<StandardMaterial>,
) {
    commands
        .spawn((
            Cable {
                solver,
                obstacles,
                resolution: DEFAULT_CABLE_RESOLUTION,
            },
            CableMeshConfig {
                material: Some(material.clone()),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn(CableEndpoint::new(CableEnd::Start, start));
            parent.spawn(CableEndpoint::new(CableEnd::End, end));
        });
}

pub(crate) fn spawn_node_pair(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
) {
    for pos in [start, end] {
        spawn_node_cube(commands, mesh, material, pos);
    }
}

pub(crate) fn spawn_node_cube<'a>(
    commands: &'a mut Commands,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    pos: Vec3,
) -> EntityCommands<'a> {
    let mut entity_commands = commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(pos),
        NodeCube,
    ));
    entity_commands.observe(input::on_mesh_clicked);
    entity_commands
}

pub(crate) fn deselect_all(commands: &mut Commands, selected: &Query<Entity, With<Selected>>) {
    for entity in selected {
        commands.entity(entity).remove::<Selected>();
    }
}

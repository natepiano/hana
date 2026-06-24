//! `Selected`, `NodeCube`, `Draggable`, `Despawnable`, `SlackLocked`,
//! `FullSceneTarget`, and the `spawn_cable`, `spawn_node_pair`, and
//! `spawn_node_cube` helpers.

use bevy::ecs::system::EntityCommands;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_catenary::Cable;
use bevy_catenary::CableEnd;
use bevy_catenary::CableEndpoint;
use bevy_catenary::CableMeshConfig;
use bevy_catenary::Obstacle;
use bevy_catenary::Solver;
use bevy_diegetic::DiegeticText;
use bevy_diegetic::Sidedness;
use fairy_dust::Face;
use fairy_dust::cube_face_transform;

use super::constants::DEFAULT_CABLE_RESOLUTION;
use super::input;

/// Every cube face, so a label reads from whichever side the camera sees.
const CUBE_FACES: [Face; 6] = [
    Face::Front,
    Face::Back,
    Face::Left,
    Face::Right,
    Face::Top,
    Face::Bottom,
];

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

/// Marks the ground plane, the `F`-key full-scene overview framing target.
#[derive(Component)]
pub(crate) struct FullSceneTarget;

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
    for position in [start, end] {
        spawn_node_cube(commands, mesh, material, position);
    }
}

pub(crate) fn spawn_node_cube<'a>(
    commands: &'a mut Commands,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    position: Vec3,
) -> EntityCommands<'a> {
    let mut entity_commands = commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(position),
        NodeCube,
    ));
    entity_commands.observe(input::on_mesh_clicked);
    entity_commands
}

/// Spawns a centered `text` label of height `text_size` on every face of a
/// `cube_size` cube. The labels are unlit so `color` reads as emissive (use an
/// HDR color with camera bloom to make them glow), and `Pickable::IGNORE` so
/// pointer picks fall through to the draggable cube underneath.
pub(crate) fn add_cube_face_labels(
    cube: &mut EntityCommands,
    text: &str,
    cube_size: f32,
    text_size: f32,
    color: Color,
) {
    cube.with_children(|parent| {
        for face in CUBE_FACES {
            parent.spawn((
                Pickable::IGNORE,
                DiegeticText::world(text)
                    .size(text_size)
                    .color(color)
                    .unlit()
                    .sidedness(Sidedness::FrontOnly)
                    .transform(cube_face_transform(face, cube_size))
                    .build(),
            ));
        }
    });
}

pub(crate) fn deselect_all(commands: &mut Commands, selected: &Query<Entity, With<Selected>>) {
    for entity in selected {
        commands.entity(entity).remove::<Selected>();
    }
}

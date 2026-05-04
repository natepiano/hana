use bevy::prelude::*;
use bevy_catenary::AttachedTo;
use bevy_catenary::Cable;
use bevy_catenary::CableEnd;
use bevy_catenary::CableEndpoint;
use bevy_catenary::CableMeshConfig;
use bevy_catenary::CatenarySolver;
use bevy_catenary::Solver;

use super::constants::DRAGGABLE_CUBE_DIMENSION;
use super::constants::ENTITY_ATTACHMENT_Z;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::DRAGGABLE_COLOR;
use crate::constants::NODE_Y;
use crate::constants::SECTION_X;
use crate::constants::SLACK_NORMAL;
use crate::constants::SPAN_HALF_X;
use crate::entities::Draggable;
use crate::entities::NodeCube;
use crate::input;

/// Section 3: Cables attached to draggable cubes.
pub(super) fn setup_section_entity_attachment(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cable_mat: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[3];
    let drag_mesh = meshes.add(Cuboid::new(
        DRAGGABLE_CUBE_DIMENSION,
        DRAGGABLE_CUBE_DIMENSION,
        DRAGGABLE_CUBE_DIMENSION,
    ));
    let drag_mat = materials.add(StandardMaterial {
        base_color: DRAGGABLE_COLOR,
        ..default()
    });

    let left_cube = commands
        .spawn((
            Mesh3d(drag_mesh.clone()),
            MeshMaterial3d(drag_mat.clone()),
            Transform::from_translation(Vec3::new(
                section_center_x - SPAN_HALF_X,
                NODE_Y,
                ENTITY_ATTACHMENT_Z,
            )),
            Draggable,
            NodeCube,
        ))
        .observe(input::on_drag_start)
        .id();

    let right_cube = commands
        .spawn((
            Mesh3d(drag_mesh),
            MeshMaterial3d(drag_mat),
            Transform::from_translation(Vec3::new(
                section_center_x + SPAN_HALF_X,
                NODE_Y,
                ENTITY_ATTACHMENT_Z,
            )),
            Draggable,
            NodeCube,
        ))
        .observe(input::on_drag_start)
        .id();

    commands
        .spawn((
            Cable {
                solver:     Solver::Catenary(CatenarySolver::new().with_slack(SLACK_NORMAL)),
                obstacles:  vec![],
                resolution: DEFAULT_CABLE_RESOLUTION,
            },
            CableMeshConfig {
                material: Some(cable_mat.clone()),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                CableEndpoint::new(CableEnd::Start, Vec3::ZERO),
                AttachedTo(left_cube),
            ));
            parent.spawn((
                CableEndpoint::new(CableEnd::End, Vec3::ZERO),
                AttachedTo(right_cube),
            ));
        });
}

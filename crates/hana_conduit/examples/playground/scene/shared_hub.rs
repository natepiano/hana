use bevy::picking::Pickable;
use bevy::prelude::*;
use hana_conduit::AttachedTo;
use hana_conduit::Cable;
use hana_conduit::CableEnd;
use hana_conduit::CableEndpoint;
use hana_conduit::CableMeshConfig;
use hana_conduit::CatenarySolver;
use hana_conduit::DEFAULT_SLACK;
use hana_conduit::Solver;
use hana_diegetic::DiegeticText;
use hana_diegetic::PanelPicking;

use super::constants::SHARED_HUB_POSITION_Z;
use super::constants::SHARED_HUB_SPHERE_RINGS;
use super::constants::SHARED_HUB_SPHERE_SECTORS;
use super::constants::SHARED_HUB_SPOKE_CENTER_INDEX;
use super::constants::SHARED_HUB_SPOKE_LEFT_INDEX;
use super::constants::SHARED_HUB_SPOKE_RIGHT_INDEX;
use super::constants::SHARED_HUB_SPOKE_X_OFFSET;
use super::constants::SHARED_HUB_SPOKE_Y_OFFSET;
use super::constants::SHARED_HUB_SPOKE_Z;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::DRAGGABLE_COLOR;
use crate::constants::HUB_LABEL_COLOR;
use crate::constants::HUB_LABEL_HOVER_Y;
use crate::constants::HUB_LABEL_SIZE;
use crate::constants::HUB_LABEL_TEXT;
use crate::constants::HUB_SPHERE_RADIUS;
use crate::constants::NODE_Y;
use crate::constants::SECTION_X;
use crate::constants::SHARED_HUB_SECTION_INDEX;
use crate::entities;
use crate::entities::Draggable;
use crate::entities::NodeCube;
use crate::input;
use crate::labels::CameraFacingLabel;

/// Section 4: Three cables from a central draggable hub.
pub(super) fn setup_section_shared_hub(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    cable_material: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[SHARED_HUB_SECTION_INDEX];
    let drag_mesh = meshes.add(
        Sphere::new(HUB_SPHERE_RADIUS)
            .mesh()
            .uv(SHARED_HUB_SPHERE_SECTORS, SHARED_HUB_SPHERE_RINGS),
    );
    let drag_material = materials.add(StandardMaterial {
        base_color: DRAGGABLE_COLOR,
        ..default()
    });

    let mut hub_entity_commands = commands.spawn((
        Mesh3d(drag_mesh),
        MeshMaterial3d(drag_material),
        Transform::from_translation(Vec3::new(section_center_x, NODE_Y, SHARED_HUB_POSITION_Z)),
        Draggable,
        NodeCube,
    ));
    hub_entity_commands.observe(input::on_drag_start);
    hub_entity_commands.with_children(|parent| {
        parent.spawn((
            CameraFacingLabel,
            Pickable::IGNORE,
            PanelPicking::PASS_THROUGH,
            DiegeticText::world(HUB_LABEL_TEXT)
                .size(HUB_LABEL_SIZE)
                .color(HUB_LABEL_COLOR)
                .transform(Transform::from_xyz(0.0, HUB_LABEL_HOVER_Y, 0.0))
                .build(),
        ));
    });
    let hub = hub_entity_commands.id();

    let spokes = [
        Vec3::new(
            section_center_x - SHARED_HUB_SPOKE_X_OFFSET,
            NODE_Y + SHARED_HUB_SPOKE_Y_OFFSET,
            SHARED_HUB_SPOKE_Z[SHARED_HUB_SPOKE_LEFT_INDEX],
        ),
        Vec3::new(
            section_center_x + SHARED_HUB_SPOKE_X_OFFSET,
            NODE_Y + SHARED_HUB_SPOKE_Y_OFFSET,
            SHARED_HUB_SPOKE_Z[SHARED_HUB_SPOKE_RIGHT_INDEX],
        ),
        Vec3::new(
            section_center_x,
            NODE_Y + SHARED_HUB_SPOKE_Y_OFFSET,
            SHARED_HUB_SPOKE_Z[SHARED_HUB_SPOKE_CENTER_INDEX],
        ),
    ];

    for spoke_position in spokes {
        entities::spawn_node_cube(commands, node_mesh, node_material, spoke_position);
        commands
            .spawn((
                Cable {
                    solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
                    obstacles:  vec![],
                    resolution: DEFAULT_CABLE_RESOLUTION,
                },
                CableMeshConfig {
                    material: Some(cable_material.clone()),
                    ..default()
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    CableEndpoint::new(CableEnd::Start, Vec3::ZERO),
                    AttachedTo(hub),
                ));
                parent.spawn(CableEndpoint::new(CableEnd::End, spoke_position));
            });
    }
}

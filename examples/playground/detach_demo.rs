//! Section 6: detach demo — cables respond to endpoint detach events.

use bevy::prelude::*;
use bevy_catenary::AttachedTo;
use bevy_catenary::Cable;
use bevy_catenary::CableEnd;
use bevy_catenary::CableEndpoint;
use bevy_catenary::CableMeshConfig;
use bevy_catenary::CatenarySolver;
use bevy_catenary::DetachPolicy;
use bevy_catenary::Solver;

use super::constants::DEFAULT_CABLE_RESOLUTION;
use super::constants::DESPAWN_GREEN;
use super::constants::DESPAWN_RED;
use super::constants::DETACH_BUMP_BLUE;
use super::constants::DETACH_DEMO_ENDPOINT_X_OFFSET;
use super::constants::DETACH_DEMO_ROW_DESPAWN_INDEX;
use super::constants::DETACH_DEMO_ROW_FREEZE_INDEX;
use super::constants::DETACH_DEMO_ROW_SLACK_BUMP_INDEX;
use super::constants::DETACH_DEMO_ROW_Z;
use super::constants::DETACH_DEMO_SECTION_INDEX;
use super::constants::DETACH_DEMO_SLACK_BUMP;
use super::constants::DETACH_DEMO_SPHERE_RINGS;
use super::constants::DETACH_DEMO_SPHERE_SECTORS;
use super::constants::HUB_SPHERE_RADIUS;
use super::constants::NODE_Y;
use super::constants::SECTION_X;
use super::constants::SLACK_NORMAL;
use super::entities;
use super::entities::Despawnable;
use super::input;

/// Marker for entities belonging to the detach demo section (for reset).
#[derive(Component)]
pub(crate) struct DetachDemoEntity;

struct DetachDemoAssets<'a> {
    sphere_mesh:    Handle<Mesh>,
    node_mesh:      &'a Handle<Mesh>,
    node_material:  &'a Handle<StandardMaterial>,
    cable_material: &'a Handle<StandardMaterial>,
}

struct DetachDemoRow {
    z:               f32,
    sphere_color:    Color,
    catenary_solver: CatenarySolver,
    detach_policy:   DetachPolicy,
}

pub(crate) fn spawn_detach_demo(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    cable_material: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[DETACH_DEMO_SECTION_INDEX];
    let assets = DetachDemoAssets {
        sphere_mesh: meshes.add(
            Sphere::new(HUB_SPHERE_RADIUS)
                .mesh()
                .uv(DETACH_DEMO_SPHERE_SECTORS, DETACH_DEMO_SPHERE_RINGS),
        ),
        node_mesh,
        node_material,
        cable_material,
    };

    let rows = [
        DetachDemoRow {
            z:               DETACH_DEMO_ROW_Z[DETACH_DEMO_ROW_FREEZE_INDEX],
            sphere_color:    DESPAWN_GREEN,
            catenary_solver: CatenarySolver::new().with_slack(SLACK_NORMAL),
            detach_policy:   DetachPolicy::Remain,
        },
        DetachDemoRow {
            z:               DETACH_DEMO_ROW_Z[DETACH_DEMO_ROW_SLACK_BUMP_INDEX],
            sphere_color:    DETACH_BUMP_BLUE,
            catenary_solver: CatenarySolver::new()
                .with_slack(SLACK_NORMAL)
                .with_detach_slack_bump(DETACH_DEMO_SLACK_BUMP),
            detach_policy:   DetachPolicy::Remain,
        },
        DetachDemoRow {
            z:               DETACH_DEMO_ROW_Z[DETACH_DEMO_ROW_DESPAWN_INDEX],
            sphere_color:    DESPAWN_RED,
            catenary_solver: CatenarySolver::new().with_slack(SLACK_NORMAL),
            detach_policy:   DetachPolicy::Despawn,
        },
    ];

    for row in rows {
        spawn_detach_demo_row(commands, materials, &assets, section_center_x, row);
    }
}

fn spawn_detach_demo_row(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    assets: &DetachDemoAssets,
    section_center_x: f32,
    row: DetachDemoRow,
) {
    let sphere_material = materials.add(StandardMaterial {
        base_color: row.sphere_color,
        ..default()
    });
    let sphere = commands
        .spawn((
            Mesh3d(assets.sphere_mesh.clone()),
            MeshMaterial3d(sphere_material),
            Transform::from_translation(Vec3::new(
                section_center_x - DETACH_DEMO_ENDPOINT_X_OFFSET,
                NODE_Y,
                row.z,
            )),
            Despawnable,
            DetachDemoEntity,
        ))
        .observe(input::on_despawnable_clicked)
        .id();

    let anchor_position = Vec3::new(
        section_center_x + DETACH_DEMO_ENDPOINT_X_OFFSET,
        NODE_Y,
        row.z,
    );
    entities::spawn_node_cube(
        commands,
        assets.node_mesh,
        assets.node_material,
        anchor_position,
    )
    .insert(DetachDemoEntity);

    commands
        .spawn((
            Cable {
                solver:     Solver::Catenary(row.catenary_solver),
                obstacles:  vec![],
                resolution: DEFAULT_CABLE_RESOLUTION,
            },
            CableMeshConfig {
                material: Some(assets.cable_material.clone()),
                ..default()
            },
            DetachDemoEntity,
        ))
        .with_children(|parent| {
            parent.spawn((
                CableEndpoint::new(CableEnd::Start, Vec3::ZERO)
                    .with_detach_policy(row.detach_policy),
                AttachedTo(sphere),
            ));
            parent.spawn(CableEndpoint::new(CableEnd::End, anchor_position));
        });
}

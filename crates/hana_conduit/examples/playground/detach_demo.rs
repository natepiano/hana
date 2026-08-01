//! Section 6: detach demo — cables respond to endpoint detach events.

use bevy::picking::Pickable;
use bevy::prelude::*;
use hana_conduit::AttachedTo;
use hana_conduit::Cable;
use hana_conduit::CableEnd;
use hana_conduit::CableEndpoint;
use hana_conduit::CableMeshConfig;
use hana_conduit::CatenarySolver;
use hana_conduit::DetachPolicy;
use hana_conduit::Solver;
use hana_diegetic::Anchor;
use hana_diegetic::DiegeticText;
use hana_diegetic::PanelPicking;

use super::constants::DEFAULT_CABLE_RESOLUTION;
use super::constants::DESPAWN_GREEN;
use super::constants::DESPAWN_RED;
use super::constants::DETACH_BUMP_BLUE;
use super::constants::DETACH_DEMO_ENDPOINT_X_OFFSET;
use super::constants::DETACH_DEMO_LABEL_COLORS;
use super::constants::DETACH_DEMO_LABEL_SIDE_GAP;
use super::constants::DETACH_DEMO_LABEL_SIZE;
use super::constants::DETACH_DEMO_LABEL_WRAP_WIDTH;
use super::constants::DETACH_DEMO_LABELS;
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
use super::labels::CameraFacingLabel;

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
    label:           &'static str,
    label_color:     Color,
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
    let detach_demo_assets = DetachDemoAssets {
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
            label:           DETACH_DEMO_LABELS[DETACH_DEMO_ROW_FREEZE_INDEX],
            label_color:     DETACH_DEMO_LABEL_COLORS[DETACH_DEMO_ROW_FREEZE_INDEX],
        },
        DetachDemoRow {
            z:               DETACH_DEMO_ROW_Z[DETACH_DEMO_ROW_SLACK_BUMP_INDEX],
            sphere_color:    DETACH_BUMP_BLUE,
            catenary_solver: CatenarySolver::new()
                .with_slack(SLACK_NORMAL)
                .with_detach_slack_bump(DETACH_DEMO_SLACK_BUMP),
            detach_policy:   DetachPolicy::Remain,
            label:           DETACH_DEMO_LABELS[DETACH_DEMO_ROW_SLACK_BUMP_INDEX],
            label_color:     DETACH_DEMO_LABEL_COLORS[DETACH_DEMO_ROW_SLACK_BUMP_INDEX],
        },
        DetachDemoRow {
            z:               DETACH_DEMO_ROW_Z[DETACH_DEMO_ROW_DESPAWN_INDEX],
            sphere_color:    DESPAWN_RED,
            catenary_solver: CatenarySolver::new().with_slack(SLACK_NORMAL),
            detach_policy:   DetachPolicy::Despawn,
            label:           DETACH_DEMO_LABELS[DETACH_DEMO_ROW_DESPAWN_INDEX],
            label_color:     DETACH_DEMO_LABEL_COLORS[DETACH_DEMO_ROW_DESPAWN_INDEX],
        },
    ];

    for row in rows {
        spawn_detach_demo_row(
            commands,
            materials,
            &detach_demo_assets,
            section_center_x,
            row,
        );
    }
}

fn spawn_detach_demo_row(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    detach_demo_assets: &DetachDemoAssets,
    section_center_x: f32,
    row: DetachDemoRow,
) {
    let sphere_material = materials.add(StandardMaterial {
        base_color: row.sphere_color,
        ..default()
    });
    let sphere_position = Vec3::new(
        section_center_x - DETACH_DEMO_ENDPOINT_X_OFFSET,
        NODE_Y,
        row.z,
    );
    let sphere = commands
        .spawn((
            Mesh3d(detach_demo_assets.sphere_mesh.clone()),
            MeshMaterial3d(sphere_material),
            Transform::from_translation(sphere_position),
            Despawnable,
            DetachDemoEntity,
        ))
        .observe(input::on_despawnable_clicked)
        .id();

    // Emissive caption to the left of the sphere, wrapped onto two lines and
    // billboarded to the camera. Anchored at its right edge so it grows away
    // from the sphere. Independent of the sphere so it survives a
    // despawn-on-click; `R` clears it via the shared `DetachDemoEntity` marker.
    commands.spawn((
        CameraFacingLabel,
        Pickable::IGNORE,
        PanelPicking::PASS_THROUGH,
        DetachDemoEntity,
        DiegeticText::world(row.label)
            .size(DETACH_DEMO_LABEL_SIZE)
            .width(DETACH_DEMO_LABEL_WRAP_WIDTH)
            .anchor(Anchor::CenterRight)
            .color(row.label_color)
            .unlit()
            .transform(Transform::from_translation(
                sphere_position - Vec3::X * DETACH_DEMO_LABEL_SIDE_GAP,
            ))
            .build(),
    ));

    let anchor_position = Vec3::new(
        section_center_x + DETACH_DEMO_ENDPOINT_X_OFFSET,
        NODE_Y,
        row.z,
    );
    entities::spawn_node_cube(
        commands,
        detach_demo_assets.node_mesh,
        detach_demo_assets.node_material,
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
                material: Some(detach_demo_assets.cable_material.clone()),
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

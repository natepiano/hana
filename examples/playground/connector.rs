//! Section 8: Connector model — GLTF power plug at one cable end.

use bevy::prelude::*;
use bevy_catenary::AttachedTo;
use bevy_catenary::Cable;
use bevy_catenary::CableEnd;
use bevy_catenary::CableEndpoint;
use bevy_catenary::CableMeshConfig;
use bevy_catenary::Capping;
use bevy_catenary::EndpointAlignment;
use bevy_catenary::Solver;

use super::constants::CONNECTOR_LANE_AS_SPAWNED_INDEX;
use super::constants::CONNECTOR_LANE_FIXED_INDEX;
use super::constants::CONNECTOR_LANE_ROTATING_INDEX;
use super::constants::CONNECTOR_LANE_Z;
use super::constants::CONNECTOR_MODEL_PATH;
use super::constants::CONNECTOR_MODEL_SCALE;
use super::constants::CONNECTOR_SECTION_INDEX;
use super::constants::DEFAULT_CABLE_RESOLUTION;
use super::constants::NODE_Y;
use super::constants::SECTION_X;
use super::constants::SPAN_HALF_X;
use super::entities::Draggable;
use super::input;

/// # Connector model origin convention
///
/// When attaching a 3D model (GLTF/GLB) to a cable end, the model's **origin must be
/// at the cable attachment point** — the point where the cable tube meets the connector.
/// For a power plug, this is the cable exit (strain relief opening). For a TRS jack,
/// it would be the back of the barrel where the cable enters.
///
/// The cable endpoint uses `AttachedTo` with `Vec3::ZERO` offset, meaning the cable
/// terminates at the connector's origin. If the origin is elsewhere (e.g. center of
/// the plug body), the cable will visually end inside the model.
///
/// In Blender, set the origin by shifting the mesh vertices so the attachment point
/// sits at (0, 0, 0), then re-export the GLB. The connector's local +Y axis (Blender +Z)
/// should point along the cable-exit direction so the alignment system can orient it
/// to match the cable tangent.
pub(crate) fn setup_section_connector(
    commands: &mut Commands,
    cable_mat: &Handle<StandardMaterial>,
    asset_server: &AssetServer,
) {
    let section_center_x = SECTION_X[CONNECTOR_SECTION_INDEX];
    let plug_scene: Handle<Scene> = asset_server.load(CONNECTOR_MODEL_PATH);

    let configs = [
        (
            Vec3::new(
                section_center_x - SPAN_HALF_X,
                NODE_Y,
                CONNECTOR_LANE_Z[CONNECTOR_LANE_FIXED_INDEX],
            ),
            Vec3::new(
                section_center_x + SPAN_HALF_X,
                NODE_Y,
                CONNECTOR_LANE_Z[CONNECTOR_LANE_FIXED_INDEX],
            ),
            EndpointAlignment::Fixed,
        ),
        (
            Vec3::new(
                section_center_x - SPAN_HALF_X,
                NODE_Y,
                CONNECTOR_LANE_Z[CONNECTOR_LANE_AS_SPAWNED_INDEX],
            ),
            Vec3::new(
                section_center_x + SPAN_HALF_X,
                NODE_Y,
                CONNECTOR_LANE_Z[CONNECTOR_LANE_AS_SPAWNED_INDEX],
            ),
            EndpointAlignment::AsSpawned,
        ),
        (
            Vec3::new(
                section_center_x - SPAN_HALF_X,
                NODE_Y,
                CONNECTOR_LANE_Z[CONNECTOR_LANE_ROTATING_INDEX],
            ),
            Vec3::new(
                section_center_x + SPAN_HALF_X,
                NODE_Y,
                CONNECTOR_LANE_Z[CONNECTOR_LANE_ROTATING_INDEX],
            ),
            EndpointAlignment::Rotating,
        ),
    ];

    for (start, end, endpoint_alignment) in configs {
        let cable = commands
            .spawn((
                Cable {
                    solver:     Solver::Linear,
                    obstacles:  vec![],
                    resolution: DEFAULT_CABLE_RESOLUTION,
                },
                CableMeshConfig {
                    material: Some(cable_mat.clone()),
                    ..default()
                },
            ))
            .id();

        let plug = commands
            .spawn((
                SceneRoot(plug_scene.clone()),
                Transform::from_translation(end).with_scale(Vec3::splat(CONNECTOR_MODEL_SCALE)),
                Draggable,
            ))
            .observe(input::on_drag_start)
            .id();

        commands.entity(cable).with_children(|parent| {
            parent.spawn(CableEndpoint::new(CableEnd::Start, start));
            parent.spawn((
                CableEndpoint::new(CableEnd::End, Vec3::ZERO)
                    .with_cap(Capping::None)
                    .with_endpoint_alignment(endpoint_alignment),
                AttachedTo(plug),
            ));
        });
    }
}

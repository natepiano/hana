//! Mesh asset handle and child entity tracking, plus the observer that (re)generates
//! the tube mesh when a cable's computed geometry changes.

use bevy::prelude::*;

use super::CableMeshConfig;
use super::tube;
use crate::cable::CableEnd;
use crate::cable::CableEndpoint;
use crate::cable::ComputedCableGeometry;

/// Stores the mesh asset handle for a cable's generated tube mesh.
/// The library manages this — users don't need to interact with it directly.
#[derive(Component)]
pub struct CableMeshHandle(pub Handle<Mesh>);

/// Stores the entity ID of the mesh child spawned for a cable.
#[derive(Component)]
pub struct CableMeshChild(pub Entity);

/// Query type for accessing cable geometry, config, and mesh handles.
type CableMeshQuery<'w> = (
    &'w ComputedCableGeometry,
    &'w CableMeshConfig,
    &'w Children,
    Option<&'w CableMeshHandle>,
    Option<&'w CableMeshChild>,
);

/// Observer that generates or updates the cable mesh when geometry is (re)computed.
///
/// On first insert: creates a `Mesh` asset, spawns a mesh child entity, stores
/// `CableMeshHandle` and `CableMeshChild` on the cable.
/// On subsequent inserts: mutates the existing mesh asset in place (no entity churn).
pub(super) fn on_geometry_computed(
    trigger: On<Insert, ComputedCableGeometry>,
    cables: Query<CableMeshQuery>,
    endpoints: Query<&CableEndpoint>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut commands: Commands,
) {
    let Some(mut meshes) = meshes else {
        return;
    };
    let cable_entity = trigger.event_target();
    let Ok((computed, cable_mesh_config, children, mesh_handle, _)) = cables.get(cable_entity)
    else {
        return;
    };

    let Some(cable_geometry) = &computed.cable_geometry else {
        return;
    };

    // Read endpoint cap styles from children
    let mut cap_start = cable_mesh_config.cap_config.start.clone();
    let mut cap_end = cable_mesh_config.cap_config.end.clone();
    for child in children.iter() {
        if let Ok(endpoint) = endpoints.get(child) {
            match endpoint.end {
                CableEnd::Start => cap_start = endpoint.cap_style.clone(),
                CableEnd::End => cap_end = endpoint.cap_style.clone(),
            }
        }
    }

    // Build the config with endpoint cap styles applied
    let mut updated_cable_mesh_config = cable_mesh_config.clone();
    updated_cable_mesh_config.cap_config.start = cap_start;
    updated_cable_mesh_config.cap_config.end = cap_end;

    let new_mesh = tube::generate_tube_mesh(cable_geometry, &updated_cable_mesh_config);

    if let Some(handle) = mesh_handle {
        // Update existing mesh asset in place
        if let Some(mut existing) = meshes.get_mut(&handle.0) {
            *existing = new_mesh;
        }
    } else {
        // First time: create asset, spawn mesh child
        let handle = meshes.add(new_mesh);
        let mut child_commands = commands.spawn((Mesh3d(handle.clone()), ChildOf(cable_entity)));
        if let Some(ref mat) = cable_mesh_config.material {
            child_commands.insert(MeshMaterial3d(mat.clone()));
        }
        let child = child_commands.id();
        commands
            .entity(cable_entity)
            .insert((CableMeshHandle(handle), CableMeshChild(child)));
    }
}

//! `CableMeshHandle`, `CableMeshChild`, and `on_geometry_computed`.

use bevy::prelude::*;

use super::CableMeshConfig;
use super::tube;
use crate::cable::CableEnd;
use crate::cable::CableEndpoint;
use crate::cable::ComputedCableGeometry;

/// Stores the mesh asset handle for a cable's generated tube mesh.
/// `on_geometry_computed` writes `CableMeshHandle` and `CableMeshChild`;
/// users don't need to interact with them directly.
#[derive(Component)]
pub struct CableMeshHandle(pub Handle<Mesh>);

/// Stores the entity ID of the mesh child spawned for a cable.
#[derive(Component)]
pub struct CableMeshChild(pub Entity);

/// Query type for accessing `ComputedCableGeometry`, `CableMeshConfig`,
/// `CableMeshHandle`, and `CableMeshChild`.
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let cable_entity = trigger.event_target();
    let Ok((computed_cable_geometry, cable_mesh_config, children, mesh_handle, _)) =
        cables.get(cable_entity)
    else {
        return;
    };

    let Some(cable_geometry) = &computed_cable_geometry.cable_geometry else {
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
        // `CableMeshHandle` looks up the existing `Mesh` with `Assets<Mesh>::get_mut`.
        // Assigning `new_mesh` mutates the stored mesh asset.
        if let Some(mut existing) = meshes.get_mut(&handle.0) {
            *existing = new_mesh;
        }
    } else {
        // `Assets<Mesh>::add` creates the `Handle<Mesh>` for the spawned `Mesh3d` child.
        // `CableMeshChild` and `CableMeshHandle` are inserted on the cable entity.
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

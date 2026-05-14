use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;

use super::AttachedEndpoints;
use super::AttachedTo;
use super::Cable;
use super::CableEnd;
use super::CableEndpoint;
use super::endpoint::ResolvedEndpointPosition;
use crate::routing::CableGeometry;
use crate::routing::MIN_SEGMENT_LENGTH;
use crate::routing::RouteRequest;

/// `SystemSet` for cross-plugin ordering. `GizmosPlugin` render systems run
/// `.after(CableSystems::Compute)` to observe freshly-computed geometry in the
/// same frame.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CableSystems {
    Compute,
}

/// Cables queued for geometry recomputation this frame. Drained by
/// [`recompute_dirty_cables`].
#[derive(Resource, Default, Deref, DerefMut)]
pub(super) struct DirtyCables(pub(super) EntityHashSet);

pub(super) struct ComputePlugin;

impl Plugin for ComputePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DirtyCables>().add_systems(
            Update,
            (
                (
                    queue_changed_cables,
                    queue_endpoint_changes,
                    queue_attached_target_moves,
                ),
                recompute_dirty_cables,
            )
                .chain()
                .in_set(CableSystems::Compute),
        );
    }
}

/// `ComputedCableGeometry`, populated by the cable recompute queue.
///
/// Rendering systems read from this component.
#[derive(Component, Clone, Default)]
pub struct ComputedCableGeometry {
    /// The computed geometry, or `None` if not yet computed.
    pub cable_geometry: Option<CableGeometry>,
}

/// Queues cables whose own `Cable` component was inserted or mutated.
fn queue_changed_cables(
    cables: Query<Entity, Changed<Cable>>,
    mut dirty_cables: ResMut<DirtyCables>,
) {
    for cable_entity in &cables {
        dirty_cables.insert(cable_entity);
    }
}

/// Queues the parent cable of any endpoint whose `CableEndpoint` or `AttachedTo`
/// component was inserted or mutated.
fn queue_endpoint_changes(
    endpoints: Query<&ChildOf, Or<(Changed<CableEndpoint>, Changed<AttachedTo>)>>,
    mut dirty_cables: ResMut<DirtyCables>,
) {
    for child_of in &endpoints {
        dirty_cables.insert(child_of.parent());
    }
}

/// Queues cables whose attached targets had their world transform change.
fn queue_attached_target_moves(
    targets: Query<&AttachedEndpoints, Changed<GlobalTransform>>,
    endpoint_parents: Query<&ChildOf>,
    mut dirty_cables: ResMut<DirtyCables>,
) {
    for attached_endpoints in &targets {
        for endpoint in attached_endpoints.iter() {
            let Ok(child_of) = endpoint_parents.get(endpoint) else {
                continue;
            };
            dirty_cables.insert(child_of.parent());
        }
    }
}

/// Drains [`DirtyCables`] and recomputes geometry for each queued cable.
fn recompute_dirty_cables(
    mut commands: Commands,
    mut dirty_cables: ResMut<DirtyCables>,
    cables: Query<(&Cable, &Children)>,
    mut endpoints: Query<(
        &CableEndpoint,
        Option<&AttachedTo>,
        Option<&mut ResolvedEndpointPosition>,
    )>,
    transforms: Query<&GlobalTransform>,
) {
    for cable_entity in dirty_cables.drain() {
        recompute_cable_route(
            cable_entity,
            &mut commands,
            &cables,
            &mut endpoints,
            &transforms,
        );
    }
}

fn recompute_cable_route(
    cable_entity: Entity,
    commands: &mut Commands,
    cables: &Query<(&Cable, &Children)>,
    endpoints: &mut Query<(
        &CableEndpoint,
        Option<&AttachedTo>,
        Option<&mut ResolvedEndpointPosition>,
    )>,
    transforms: &Query<&GlobalTransform>,
) {
    let Ok((cable, children)) = cables.get(cable_entity) else {
        return;
    };

    let mut start_position = None;
    let mut end_position = None;

    for child in children.iter() {
        let Ok((endpoint, attached_to, resolved_endpoint_position)) = endpoints.get_mut(child)
        else {
            continue;
        };

        let pos = if let Some(attached) = attached_to
            && let Ok(target_transform) = transforms.get(attached.0)
        {
            target_transform.transform_point(endpoint.offset)
        } else {
            endpoint.offset
        };

        if let Some(mut resolved) = resolved_endpoint_position {
            if resolved.0 != pos {
                resolved.0 = pos;
            }
        } else {
            commands.entity(child).insert(ResolvedEndpointPosition(pos));
        }

        match endpoint.end {
            CableEnd::Start => start_position = Some(pos),
            CableEnd::End => end_position = Some(pos),
        }
    }

    let (Some(start), Some(end)) = (start_position, end_position) else {
        return;
    };

    if start.distance(end) < MIN_SEGMENT_LENGTH {
        return;
    }

    let request = RouteRequest {
        start,
        end,
        obstacles: &cable.obstacles,
        resolution: cable.resolution,
    };

    let cable_geometry = cable.solver.solve(&request);
    commands.entity(cable_entity).insert(ComputedCableGeometry {
        cable_geometry: Some(cable_geometry),
    });
}

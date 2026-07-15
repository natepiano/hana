use bevy::camera::primitives::Aabb;
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;

use super::AttachedEndpoints;
use super::AttachedTo;
use super::Cable;
use super::CableEnd;
use super::CableEndpoint;
use super::EndpointExit;
use super::RouteObstacle;
use super::animation;
use super::animation::RouteAnimation;
use super::animation::SolvedRoute;
use super::route_obstacle;
use crate::routing::Anchor;
use crate::routing::AnchorExit;
use crate::routing::CableGeometry;
use crate::routing::MIN_SEGMENT_LENGTH;
use crate::routing::Obstacle;
use crate::routing::RouteRequest;

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
                    queue_obstacle_changes,
                ),
                recompute_dirty_cables,
                animation::animate_routes,
            )
                .chain()
                .in_set(CableSystems::Compute),
        );
    }
}

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

/// Last world-space position resolved for an endpoint while it was attached.
#[derive(Component, Clone, Copy)]
pub(super) struct ResolvedEndpointPosition(pub(super) Vec3);

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

/// Queues every cable when any [`RouteObstacle`] entity moved, changed, or was
/// removed, so routes re-solve against the obstacle's current bounds.
fn queue_obstacle_changes(
    changed: Query<
        (),
        (
            With<RouteObstacle>,
            Or<(Changed<GlobalTransform>, Changed<RouteObstacle>)>,
        ),
    >,
    mut removed: RemovedComponents<RouteObstacle>,
    cables: Query<Entity, With<Cable>>,
    mut dirty_cables: ResMut<DirtyCables>,
) {
    if changed.is_empty() && removed.is_empty() {
        return;
    }
    removed.clear();
    for cable_entity in &cables {
        dirty_cables.insert(cable_entity);
    }
}

/// Drains [`DirtyCables`] and recomputes geometry for each queued cable.
fn recompute_dirty_cables(
    mut commands: Commands,
    mut dirty_cables: ResMut<DirtyCables>,
    cables: Query<(&Cable, &Children, Has<RouteAnimation>)>,
    mut endpoints: Query<(
        &CableEndpoint,
        Option<&AttachedTo>,
        Option<&mut ResolvedEndpointPosition>,
    )>,
    transforms: Query<&GlobalTransform>,
    route_obstacles: Query<(Entity, &RouteObstacle, &GlobalTransform)>,
    children: Query<&Children>,
    aabbs: Query<&Aabb>,
) {
    if dirty_cables.is_empty() {
        return;
    }
    let world_obstacles =
        route_obstacle::resolve_obstacles(&route_obstacles, &children, &aabbs, &transforms);
    for cable_entity in dirty_cables.drain() {
        recompute_cable_route(
            cable_entity,
            &mut commands,
            &cables,
            &mut endpoints,
            &transforms,
            &world_obstacles,
        );
    }
}

fn recompute_cable_route(
    cable_entity: Entity,
    commands: &mut Commands,
    cables: &Query<(&Cable, &Children, Has<RouteAnimation>)>,
    endpoints: &mut Query<(
        &CableEndpoint,
        Option<&AttachedTo>,
        Option<&mut ResolvedEndpointPosition>,
    )>,
    transforms: &Query<&GlobalTransform>,
    world_obstacles: &[Obstacle],
) {
    let Ok((cable, children, animated)) = cables.get(cable_entity) else {
        return;
    };

    let mut start_anchor = None;
    let mut end_anchor = None;

    for child in children.iter() {
        let Ok((endpoint, attached_to, resolved_endpoint_position)) = endpoints.get_mut(child)
        else {
            continue;
        };

        let target_transform = attached_to.and_then(|attached| transforms.get(attached.0).ok());

        let endpoint_position = target_transform.map_or(endpoint.offset, |target| {
            target.transform_point(endpoint.offset)
        });

        // `EndpointExit::Lead` declares its axis in the target's local space;
        // rotate it into world space for the routing layer's `AnchorExit`.
        let exit = match endpoint.exit {
            EndpointExit::Unconstrained => AnchorExit::Unconstrained,
            EndpointExit::Lead { axis, length } => AnchorExit::Lead {
                direction: target_transform.map_or(axis, |target| target.rotation() * axis),
                length,
            },
        };

        if let Some(mut resolved) = resolved_endpoint_position {
            if resolved.0 != endpoint_position {
                resolved.0 = endpoint_position;
            }
        } else {
            commands
                .entity(child)
                .insert(ResolvedEndpointPosition(endpoint_position));
        }

        let anchor = Anchor {
            position: endpoint_position,
            exit,
        };
        match endpoint.end {
            CableEnd::Start => start_anchor = Some(anchor),
            CableEnd::End => end_anchor = Some(anchor),
        }
    }

    let (Some(start), Some(end)) = (start_anchor, end_anchor) else {
        return;
    };

    if start.position.distance(end.position) < MIN_SEGMENT_LENGTH {
        return;
    }

    // The cable's own static obstacles, merged with this frame's resolved
    // `RouteObstacle` snapshots.
    let obstacles: Vec<Obstacle> = cable
        .obstacles
        .iter()
        .copied()
        .chain(world_obstacles.iter().copied())
        .collect();

    let route_request = RouteRequest {
        start,
        end,
        obstacles: &obstacles,
        resolution: cable.resolution,
    };

    let cable_geometry = cable.solver.solve(&route_request);
    if animated {
        // `animate_routes` runs after this system and blends the displayed
        // geometry toward the `SolvedRoute` before writing
        // `ComputedCableGeometry` itself. The anchors let it recognize the
        // geometry's lead segments and keep them out of the blend.
        commands.entity(cable_entity).insert(SolvedRoute {
            geometry: cable_geometry,
            start:    route_request.start,
            end:      route_request.end,
        });
    } else {
        commands.entity(cable_entity).insert(ComputedCableGeometry {
            cable_geometry: Some(cable_geometry),
        });
    }
}

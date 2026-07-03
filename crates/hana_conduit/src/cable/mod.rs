//! `Cable`, `CableEndpoint`, `AttachedTo`, `AttachedEndpoints`, `DetachPolicy`,
//! `EndpointAlignment`, `EndpointExit`, `RouteObstacle`, and `RouteAnimation`,
//! plus `on_endpoint_alignment_update` and `on_endpoint_detached`.

mod animation;
mod compute;
mod constants;
mod endpoint;
mod route_obstacle;

pub use animation::RouteAnimation;
use bevy::prelude::*;
pub(crate) use compute::CableSystems;
use compute::ComputePlugin;
pub use compute::ComputedCableGeometry;
pub use endpoint::AttachedEndpoints;
pub use endpoint::AttachedTo;
pub use endpoint::CableEnd;
pub use endpoint::CableEndpoint;
pub use endpoint::DetachPolicy;
pub use endpoint::EndpointAlignment;
pub use endpoint::EndpointExit;
pub use route_obstacle::RouteObstacle;

use crate::mesh::CableMeshConfig;
use crate::routing::Obstacle;
use crate::routing::Solver;

pub(super) struct CablePlugin;

impl Plugin for CablePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ComputePlugin)
            .add_observer(endpoint::on_endpoint_alignment_update)
            .add_observer(endpoint::on_endpoint_detached);
    }
}

/// A cable entity. Route computation is driven by its child [`CableEndpoint`] entities.
///
/// The cable itself stores the solver, obstacles, and resolution. Endpoint positions
/// come from child entities with [`CableEndpoint`] components.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
#[require(ComputedCableGeometry, CableMeshConfig, Transform, Visibility)]
pub struct Cable {
    /// The routing algorithm to use.
    pub solver:     Solver,
    /// Static obstacles to route around, merged at recompute time with the
    /// boxes resolved from every [`RouteObstacle`] entity in the world.
    pub obstacles:  Vec<Obstacle>,
    /// Number of sample points per segment (0 = use solver default).
    pub resolution: u32,
}

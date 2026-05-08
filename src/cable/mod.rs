//! Cable components and route computation — `Cable`, `CableEndpoint`, and supporting
//! relationship/policy types, plus the observers that align attached targets and handle
//! detachment.

mod compute;
mod constants;
mod endpoint;

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

use crate::mesh::CableMeshConfig;
use crate::routing::Obstacle;
use crate::routing::Solver;

pub(crate) struct CablePlugin;

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
    /// Obstacles to route around.
    pub obstacles:  Vec<Obstacle>,
    /// Number of sample points per segment (0 = use solver default).
    pub resolution: u32,
}

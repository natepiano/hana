//! `bevy_catenary` — Physics-based 3D cable routing for Bevy.
//!
//! Provides catenary curves, A* pathfinding, and orthogonal routing for 3D cables,
//! with a clean separation between route math and Bevy rendering.
//!
//!
//! # Architecture
//!
//! The crate is split into two layers:
//! - `routing` — Pure math module (depends only on `glam`). Produces [`CableGeometry`].
//! - Bevy integration layer — consumes [`CableGeometry`] for rendering.
//!
//! # Quick Start
//!
//! ```ignore
//! use bevy::prelude::*;
//! use bevy_catenary::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(CatenaryPlugin)
//!     .add_systems(Startup, setup)
//!     .run();
//!
//! fn setup(mut commands: Commands) {
//!     commands
//!         .spawn(Cable {
//!             solver: Solver::Catenary(CatenarySolver::new().with_slack(1.3)),
//!             obstacles: vec![],
//!             resolution: 0,
//!         })
//!         .with_children(|parent| {
//!             parent.spawn(CableEndpoint::new(CableEnd::Start, Vec3::new(-2.0, 2.0, 0.0)));
//!             parent.spawn(CableEndpoint::new(CableEnd::End, Vec3::new(2.0, 2.0, 0.0)));
//!         });
//! }
//! ```

mod cable;
mod constants;
mod gizmos;
mod mesh;
mod routing;

use bevy::prelude::*;
// Cable
pub use cable::AttachedEndpoints;
pub use cable::AttachedTo;
pub use cable::Cable;
pub use cable::CableEnd;
pub use cable::CableEndpoint;
use cable::CablePlugin;
pub use cable::ComputedCableGeometry;
pub use cable::DetachPolicy;
pub use cable::EndpointAlignment;
// Gizmos
pub use gizmos::CableGizmoGroup;
pub use gizmos::DebugGizmos;
use gizmos::GizmosPlugin;
// Mesh
pub use mesh::CableMeshChild;
pub use mesh::CableMeshConfig;
pub use mesh::CableMeshHandle;
pub use mesh::CapConfig;
pub use mesh::Capping;
pub use mesh::ElbowConfig;
pub use mesh::ElbowMetadata;
pub use mesh::Faces;
use mesh::MeshPlugin;
pub use mesh::TrimConfig;
pub use mesh::TubeConfig;
pub use mesh::compute_elbow_metadata;
pub use mesh::generate_tube_mesh;
// Routing
pub use routing::AStarPlanner;
pub use routing::Anchor;
pub use routing::AxisOrder;
pub use routing::CableGeometry;
pub use routing::CableSegment;
pub use routing::CatenarySolver;
pub use routing::CurveKind;
pub use routing::CurveSolver;
pub use routing::DEFAULT_GRAVITY;
pub use routing::DEFAULT_RESOLUTION;
pub use routing::DEFAULT_SLACK;
pub use routing::DirectPlanner;
pub use routing::LinearSolver;
pub use routing::Obstacle;
pub use routing::OrthogonalPlanner;
pub use routing::PathPlanner;
pub use routing::PathStrategy;
pub use routing::RouteRequest;
pub use routing::RouteSolver;
pub use routing::Router;
pub use routing::Solver;
pub use routing::evaluate;
pub use routing::sample_3d;
pub use routing::solve_parameter;

/// Plugin that adds cable routing support to a Bevy app.
///
/// Registers:
/// - [`DebugGizmos`] resource (default: off).
/// - [`CableGizmoGroup`] for controlling debug visibility.
/// - `CablePlugin`, including `queue_changed_cables`, `queue_endpoint_changes`,
///   `queue_attached_target_moves`, `recompute_dirty_cables`, `on_endpoint_alignment_update`, and
///   `on_endpoint_detached`.
/// - `MeshPlugin`, including `on_geometry_computed`.
/// - `GizmosPlugin`, including `render_cable_gizmos` and `render_debug_gizmos`.
pub struct CatenaryPlugin;

impl Plugin for CatenaryPlugin {
    fn build(&self, app: &mut App) { app.add_plugins((CablePlugin, MeshPlugin, GizmosPlugin)); }
}

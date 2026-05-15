//! [`CableEndpoint`] and the observers that align and detach it.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::Cable;
use super::ComputedCableGeometry;
use super::compute::DirtyCables;
use super::constants::ALIGNMENT_FEEDBACK_GUARD;
use crate::mesh::Capping;
use crate::routing::CurveKind;
use crate::routing::Solver;

/// Which end of the cable an endpoint represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum CableEnd {
    /// The starting end of the cable.
    Start,
    /// The ending end of the cable.
    End,
}

/// A cable endpoint entity. Spawned as a child of a [`Cable`] entity.
///
/// `offset` has different semantics depending on whether [`AttachedTo`] is present:
/// - **World-attached** (no `AttachedTo`): `offset` is the world-space position.
/// - **Entity-attached** (with `AttachedTo`): `offset` is in the target entity's local space. The
///   system transforms it to world space via the target's [`GlobalTransform`].
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct CableEndpoint {
    /// Which end of the cable this represents.
    pub end:           CableEnd,
    /// Position offset. World-space for world-attached, local-space for entity-attached.
    pub offset:        Vec3,
    /// How to cap this end of the tube mesh.
    pub cap_style:     Capping,
    /// What happens when the target entity is despawned.
    pub detach_policy: DetachPolicy,
    /// How the endpoint's [`AttachedTo`] target rotates to follow the cable's tangent.
    pub alignment:     EndpointAlignment,
}

impl CableEndpoint {
    /// Create a new endpoint with default cap style (`Round`), detach policy (`Remain`),
    /// and alignment (`AsSpawned`).
    #[must_use]
    pub fn new(end: CableEnd, offset: impl Into<Vec3>) -> Self {
        Self {
            end,
            offset: offset.into(),
            cap_style: Capping::Round,
            detach_policy: DetachPolicy::Remain,
            alignment: EndpointAlignment::AsSpawned,
        }
    }

    /// Set the cap style for this endpoint.
    #[must_use]
    pub const fn with_cap(mut self, cap_style: Capping) -> Self {
        self.cap_style = cap_style;
        self
    }

    /// Set the detach policy for this endpoint.
    #[must_use]
    pub const fn with_detach_policy(mut self, detach_policy: DetachPolicy) -> Self {
        self.detach_policy = detach_policy;
        self
    }

    /// Set the alignment mode for this endpoint's attached target.
    #[must_use]
    pub const fn with_alignment(mut self, alignment: EndpointAlignment) -> Self {
        self.alignment = alignment;
        self
    }
}

/// How the [`AttachedTo`] target of a [`CableEndpoint`] rotates to follow the cable's
/// tangent at that endpoint.
///
/// Assumes the target's `+Y` axis is its "cable-exit" axis (matches Bevy's GLTF import
/// convention). Models with a different local axis should wrap in a parent entity.
#[derive(Clone, Copy, Debug, Default, Reflect)]
pub enum EndpointAlignment {
    /// The target's rotation is not touched. It stays in whatever orientation it
    /// was spawned with.
    #[default]
    AsSpawned,
    /// Orient the target's `+Y` axis along the cable's tangent at this endpoint,
    /// constrained to world-`Y` up so the target never rolls as the cable sweeps.
    Fixed,
    /// Orient the target's `+Y` axis along the cable's tangent via shortest-arc
    /// rotation — the target rolls with the cable's natural twist.
    Rotating,
}

/// What happens when an endpoint's target entity is despawned.
///
/// When a target entity is despawned, Bevy auto-removes the [`AttachedTo`] relationship.
/// An `OnRemove<AttachedTo>` observer fires and reads this policy to decide behavior.
///
/// How the curve itself reacts to detachment (e.g. increasing slack on a catenary) is a
/// per-solver concern — see `CatenarySolver::with_detach_slack_bump`.
#[derive(Clone, Debug, Default, Reflect)]
pub enum DetachPolicy {
    /// Convert to world-attached at the last resolved position. Cable keeps its shape.
    #[default]
    Remain,
    /// Despawn the entire cable when this endpoint's target is removed.
    Despawn,
}

/// Relationship: endpoint → target entity it follows.
///
/// When present on a [`CableEndpoint`] entity, the endpoint's `offset` is interpreted
/// as a local-space offset from the target's [`GlobalTransform`]. The cable recomputes
/// reactively when the target's transform changes.
///
/// If the target entity is despawned, the endpoint's [`DetachPolicy`] determines behavior.
#[derive(Component)]
#[relationship(relationship_target = AttachedEndpoints)]
pub struct AttachedTo(pub Entity);

/// Back-reference on target entities. Auto-populated by Bevy's relationship system.
///
/// Query this to find all cable endpoints attached to a given entity.
#[derive(Component)]
#[relationship_target(relationship = AttachedTo)]
pub struct AttachedEndpoints(Vec<Entity>);

/// Last world-space position resolved for an endpoint while it was attached.
#[derive(Component, Clone, Copy)]
pub(super) struct ResolvedEndpointPosition(pub(super) Vec3);

/// Align the [`AttachedTo`] target of each [`CableEndpoint`] to the cable's tangent
/// per the endpoint's [`EndpointAlignment`].
///
/// Fires every time a cable's [`ComputedCableGeometry`] is (re)inserted, which the
/// library re-inserts on each recompute.
pub(super) fn on_endpoint_alignment_update(
    trigger: On<Insert, ComputedCableGeometry>,
    cables: Query<(&ComputedCableGeometry, &Children)>,
    endpoints: Query<(&CableEndpoint, &AttachedTo)>,
    mut targets: Query<&mut Transform>,
) {
    let cable = trigger.event_target();
    let Ok((computed, children)) = cables.get(cable) else {
        return;
    };
    let Some(cable_geometry) = &computed.cable_geometry else {
        return;
    };

    for child in children.iter() {
        let Ok((endpoint, attached)) = endpoints.get(child) else {
            continue;
        };
        let Ok(mut transform) = targets.get_mut(attached.0) else {
            continue;
        };

        // Tangent at this endpoint's end of the geometry.
        let tangent = match endpoint.end {
            CableEnd::Start => cable_geometry
                .segments
                .first()
                .and_then(|s| s.tangents.first().copied()),
            CableEnd::End => cable_geometry
                .segments
                .last()
                .and_then(|s| s.tangents.last().copied()),
        };
        let Some(tangent) = tangent else {
            continue;
        };

        // Cable-exit axis convention: target's +Y faces "outward" from the cable.
        // Negate so the cable enters from the target's -Y side.
        let direction = -tangent;

        let new_rotation = match endpoint.alignment {
            EndpointAlignment::AsSpawned => continue,
            EndpointAlignment::Fixed => {
                // `looking_to` orients -Z toward `direction` with Y up, so no roll.
                // Rotate -90° around X to remap the model's +Y to that -Z direction.
                let look = Transform::IDENTITY.looking_to(direction, Vec3::Y);
                look.rotation * Quat::from_rotation_x(-FRAC_PI_2)
            },
            EndpointAlignment::Rotating => Quat::from_rotation_arc(Vec3::Y, direction),
        };

        // Feedback-loop guard: writing `Transform` marks `GlobalTransform` dirty, which
        // re-triggers cable recomputation. Without this guard, each update would write a
        // rotation that redirties `GlobalTransform`, forming an infinite cycle of
        // geometry → alignment → geometry.
        let delta = transform.rotation.dot(new_rotation).abs();
        if delta < ALIGNMENT_FEEDBACK_GUARD {
            transform.rotation = new_rotation;
        }
    }
}

/// Observer that handles endpoint detachment when a target entity is despawned.
///
/// Bevy auto-removes `AttachedTo` when the target entity is despawned, which
/// triggers `OnRemove<AttachedTo>`. This observer reads the endpoint's
/// [`DetachPolicy`] and acts accordingly.
pub(super) fn on_endpoint_detached(
    trigger: On<Remove, AttachedTo>,
    mut endpoints: Query<(
        &mut CableEndpoint,
        &ChildOf,
        Option<&ResolvedEndpointPosition>,
    )>,
    mut cables: Query<&mut Cable>,
    mut commands: Commands,
    mut dirty_cables: ResMut<DirtyCables>,
) {
    let endpoint_entity = trigger.event_target();
    let Ok((mut endpoint, child_of, resolved_endpoint_position)) =
        endpoints.get_mut(endpoint_entity)
    else {
        return;
    };

    match endpoint.detach_policy {
        DetachPolicy::Remain => {
            preserve_resolved_world_position(&mut endpoint, resolved_endpoint_position);

            if let Ok(mut cable) = cables.get_mut(child_of.parent()) {
                apply_solver_detach_response(&mut cable.solver);
            }

            dirty_cables.insert(child_of.parent());
        },
        DetachPolicy::Despawn => {
            let cable_entity = child_of.parent();
            commands.entity(cable_entity).despawn();
        },
    }
}

const fn preserve_resolved_world_position(
    endpoint: &mut CableEndpoint,
    resolved_endpoint_position: Option<&ResolvedEndpointPosition>,
) {
    if let Some(resolved_endpoint_position) = resolved_endpoint_position {
        endpoint.offset = resolved_endpoint_position.0;
    }
}

fn apply_solver_detach_response(solver: &mut Solver) {
    match solver {
        Solver::Catenary(catenary_solver)
        | Solver::Routed {
            curve_kind: CurveKind::Catenary(catenary_solver),
            ..
        } => {
            if let Some(bump) = catenary_solver.detach_slack_bump {
                catenary_solver.slack += bump;
            }
        },
        Solver::Linear | Solver::Routed { .. } => {},
    }
}

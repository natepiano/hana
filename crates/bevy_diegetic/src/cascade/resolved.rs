//! The cascade's value-type trait ([`CascadeAttr`]), its matched
//! [`Override<A>`] / [`Resolved<A>`] component pair, the two attribute value
//! types, and the bounded parent-walk resolver ([`resolve_walk`]).

use bevy::prelude::*;
use bevy::reflect::GetTypeRegistration;
use bevy::reflect::Typed;

use super::constants::CASCADE_DEPTH_CAP;
use super::defaults::CascadeDefaults;
use crate::layout::Unit;

/// A cascading attribute — a pure value type that resolves *my own override,
/// else my parent's, else a global default*.
///
/// Wrapped in [`Override<A>`] (input) and [`Resolved<A>`] (cached output); the
/// value type itself is never a `Component`. The reflection supertraits are
/// what `register_type::<Override<A>>()` / `register_type::<Resolved<A>>()`
/// require; `FromReflect: Reflect`, so an explicit `Reflect` bound is
/// redundant. [`global_default`](Self::global_default) is the only per-attribute
/// method — it keeps [`CascadeDefaults`] the single resource the cascade reads.
pub(crate) trait CascadeAttr:
    Copy + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static
{
    /// Read this attribute's global default from [`CascadeDefaults`].
    fn global_default(defaults: &CascadeDefaults) -> Self;
}

/// Text alpha-mode cascade attribute.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct TextAlpha(pub AlphaMode);

impl CascadeAttr for TextAlpha {
    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}

/// Font-unit cascade attribute.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct FontUnit(pub Unit);

impl CascadeAttr for FontUnit {
    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.font_unit) }
}

/// A node's own override for attribute `A` — the cascade input.
///
/// Presence on an entity means "this node overrides `A`"; absence means
/// "inherit." There is exactly one override component type per attribute, and
/// an entity holds at most one of any component, so "two sources for one
/// attribute" cannot be written down — which is why the cascade needs no
/// exclusion marker.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub(crate) struct Override<A: CascadeAttr>(pub A);

/// Per-entity cache of a resolved cascading attribute.
///
/// Maintained by [`CascadePlugin`](super::CascadePlugin): seeded at spawn by
/// the node-kind authoring bridges, kept current by the propagation pass.
/// Crate-internal; readers query `&Resolved<A>` and filter on
/// `Changed<Resolved<A>>`.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub(crate) struct Resolved<A: CascadeAttr>(pub A);

/// Resolve attribute `A` for `entity` by walking up `ChildOf`: the first
/// ancestor (starting with `entity` itself) that carries an `Override<A>`
/// wins; a node with no override and no parent resolves to the global default.
///
/// The walk is bounded by [`CASCADE_DEPTH_CAP`] and tracks visited entities in
/// debug builds. A self-parent, a `ChildOf` cycle, a parentless node, or a
/// dangling `ChildOf` after a parent despawn all terminate at the global
/// default — never a hang. Exceeding the cap `warn!`-logs in both debug and
/// release so a malformed hierarchy is visible.
pub(crate) fn resolve_walk<A: CascadeAttr>(
    entity: Entity,
    overrides: &Query<&Override<A>>,
    parents: &Query<&ChildOf>,
    defaults: &CascadeDefaults,
) -> A {
    #[cfg(debug_assertions)]
    let mut visited = bevy::platform::collections::HashSet::new();
    let mut current = entity;
    for _ in 0..CASCADE_DEPTH_CAP {
        #[cfg(debug_assertions)]
        if !visited.insert(current) {
            warn!("cascade walk hit a ChildOf cycle at {current:?}; using global default");
            return A::global_default(defaults);
        }
        if let Ok(node_override) = overrides.get(current) {
            return node_override.0;
        }
        let Ok(child_of) = parents.get(current) else {
            return A::global_default(defaults);
        };
        current = child_of.parent();
    }
    warn!("cascade walk exceeded depth cap from {entity:?}; using global default");
    A::global_default(defaults)
}

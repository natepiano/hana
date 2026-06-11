//! The cascade's value-type traits, matched [`Override<A>`] / [`Resolved<A>`]
//! component pair, and bounded parent-walk resolvers.

use bevy::prelude::*;
use bevy::reflect::GetTypeRegistration;
use bevy::reflect::Typed;

use super::constants::CASCADE_DEPTH_CAP;
use crate::layout::GlyphLighting;
use crate::layout::GlyphSidedness;
use crate::layout::Unit;
use crate::render::HairlineFade;
use crate::render::TextAntiAlias;

mod private {
    pub trait Sealed {}
}

macro_rules! cascade_attr {
    // Joins an already-declared value type (one whose own name is the
    // attribute, e.g. `TextAntiAlias`) to the cascade instead of minting a
    // wrapper struct. The type must derive `Copy`, `PartialEq`, `Debug`, and
    // `Reflect`.
    (existing $name:ty, default = $default:expr) => {
        impl $crate::cascade::resolved::private::Sealed for $name {}

        impl $crate::cascade::resolved::CascadeProperty for $name {}

        impl $crate::cascade::resolved::CascadeAttr for $name {}

        impl Default for $crate::cascade::defaults::CascadeDefault<$name> {
            fn default() -> Self { Self($default) }
        }
    };

    ($(#[$meta:meta])* $name:ident($value:ty), default = $default:expr, eq) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Reflect)]
        pub struct $name(pub $value);

        impl $crate::cascade::resolved::private::Sealed for $name {}

        impl $crate::cascade::resolved::CascadeProperty for $name {}

        impl $crate::cascade::resolved::CascadeAttr for $name {}

        impl Default for $crate::cascade::defaults::CascadeDefault<$name> {
            fn default() -> Self { Self($name($default)) }
        }
    };

    ($(#[$meta:meta])* $name:ident($value:ty), default = $default:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Debug, Reflect)]
        pub struct $name(pub $value);

        impl $crate::cascade::resolved::private::Sealed for $name {}

        impl $crate::cascade::resolved::CascadeProperty for $name {}

        impl $crate::cascade::resolved::CascadeAttr for $name {}

        impl Default for $crate::cascade::defaults::CascadeDefault<$name> {
            fn default() -> Self { Self($name($default)) }
        }
    };
}

cascade_attr!(
    /// Text alpha-mode cascade attribute.
    TextAlpha(AlphaMode),
    default = AlphaMode::Blend,
    eq
);
cascade_attr!(
    /// Font-unit cascade attribute.
    FontUnit(Unit),
    default = Unit::Meters
);
cascade_attr!(
    /// Glyph-lighting cascade attribute. Global default is `Lit` (world text);
    /// the screen-panel construction bridge overrides it to `Unlit`.
    TextLighting(GlyphLighting),
    default = GlyphLighting::Lit,
    eq
);
cascade_attr!(
    /// Glyph-sidedness cascade attribute. Global default is `DoubleSided`
    /// (world text); the screen-panel construction bridge overrides it to
    /// `OneSided`.
    TextSidedness(GlyphSidedness),
    default = GlyphSidedness::DoubleSided,
    eq
);
// Anti-alias mode cascade attribute. The `TextAntiAlias` resource is the
// authored global; `sync_text_anti_alias` mirrors it into
// `CascadeDefault<TextAntiAlias>` as the cascade root default.
cascade_attr!(existing TextAntiAlias, default = TextAntiAlias::Both);
// Hairline-fade cascade attribute. `HairlineWidth::fade` is the authored
// global; `sync_hairline_fade` mirrors it into `CascadeDefault<HairlineFade>`
// as the cascade root default.
cascade_attr!(existing HairlineFade, default = HairlineFade::Full);

#[cfg(test)]
cascade_attr!(TestUnit(Unit), default = Unit::Meters);

/// Public marker for a cascading property value.
///
/// This is deliberately smaller than the crate-internal reflection contract
/// used by the ECS components. Public command/read APIs can name
/// `CascadeProperty` without leaking Bevy reflection bounds.
pub trait CascadeProperty: private::Sealed + Copy + PartialEq + Send + Sync + 'static {}

/// A cascading attribute — a pure value type that resolves *my own override,
/// else my parent's, else a global default*.
///
/// Wrapped in [`Override<A>`] (input) and [`Resolved<A>`] (cached output); the
/// value type itself is never a `Component`. The reflection supertraits are
/// what `register_type::<Override<A>>()` / `register_type::<Resolved<A>>()`
/// require; `FromReflect: Reflect`, so an explicit `Reflect` bound is
/// redundant.
pub(crate) trait CascadeAttr:
    CascadeProperty + FromReflect + TypePath + Typed + GetTypeRegistration
{
}

/// A node's own override for attribute `A` — the cascade input.
///
/// Presence on an entity means "this node overrides `A`"; absence means
/// "inherit." There is exactly one override component type per attribute, and
/// an entity holds at most one of any component, so "two sources for one
/// attribute" cannot be written down — which is why the cascade needs no
/// exclusion marker.
///
/// This component is `pub(crate)` and cannot be named by external
/// inspectors/tests. Revisit that boundary only if external tooling needs to
/// inspect authored cascade state directly.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub(crate) struct Override<A: CascadeAttr>(pub A);

/// Per-entity cache of a resolved cascading attribute.
///
/// Maintained by [`CascadePlugin`](super::CascadePlugin): seeded at spawn by
/// the node-kind authoring bridges, kept current by the propagation pass.
/// Crate-internal; readers query `&Resolved<A>` and filter on
/// `Changed<Resolved<A>>`.
///
/// This component is `pub(crate)` and cannot be named by external
/// inspectors/tests. Revisit that boundary only if external tooling needs to
/// inspect computed cascade state directly.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub(crate) struct Resolved<A: CascadeAttr>(pub A);

/// Resolve attribute `A` for `entity` from [`World`] by walking up `ChildOf`.
#[allow(
    dead_code,
    reason = "Phase 2 command self-heal will use the World-based resolver"
)]
pub(crate) fn resolve<A: CascadeAttr>(world: &World, entity: Entity, default: A) -> A {
    #[cfg(debug_assertions)]
    let mut visited = bevy::platform::collections::HashSet::new();
    let mut current = entity;
    for _ in 0..CASCADE_DEPTH_CAP {
        #[cfg(debug_assertions)]
        if !visited.insert(current) {
            warn!("cascade walk hit a ChildOf cycle at {current:?}; using global default");
            return default;
        }
        if let Some(node_override) = world.get::<Override<A>>(current) {
            return node_override.0;
        }
        let Some(child_of) = world.get::<ChildOf>(current) else {
            return default;
        };
        current = child_of.parent();
    }
    warn!("cascade walk exceeded depth cap from {entity:?}; using global default");
    default
}

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
    default: A,
) -> A {
    #[cfg(debug_assertions)]
    let mut visited = bevy::platform::collections::HashSet::new();
    let mut current = entity;
    for _ in 0..CASCADE_DEPTH_CAP {
        #[cfg(debug_assertions)]
        if !visited.insert(current) {
            warn!("cascade walk hit a ChildOf cycle at {current:?}; using global default");
            return default;
        }
        if let Ok(node_override) = overrides.get(current) {
            return node_override.0;
        }
        let Ok(child_of) = parents.get(current) else {
            return default;
        };
        current = child_of.parent();
    }
    warn!("cascade walk exceeded depth cap from {entity:?}; using global default");
    default
}

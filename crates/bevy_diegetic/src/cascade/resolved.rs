//! [`Resolved<A>`] — the per-entity cache component — plus the attribute
//! traits ([`CascadePanelChild`], [`CascadeTarget`]) and the pure resolvers
//! used by the plugin write paths.

use bevy::prelude::*;
use bevy::reflect::GetTypeRegistration;
use bevy::reflect::Typed;

use super::defaults::CascadeDefaults;
use crate::layout::Unit;

/// A 3-tier cascade: entity override → panel override → global default.
///
/// `Resolved<A>` lives on **both** the panel and every tier-1 child. The
/// panel's copy represents the effective "default my children inherit"; each
/// child's copy is the final rendered value.
pub(crate) trait CascadePanelChild:
    Copy + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static
{
    /// Tier-1 override component on the entity.
    type EntityOverride: Component;
    /// Tier-2 override component on the parent panel.
    type PanelOverride: Component;

    /// Project the tier-1 override value from its component.
    fn entity_value(entity_override: &Self::EntityOverride) -> Option<Self>;
    /// Project the tier-2 override value from its component.
    fn panel_value(panel_override: &Self::PanelOverride) -> Option<Self>;
    /// Read the tier-3 global default.
    fn global_default(defaults: &CascadeDefaults) -> Self;
}

/// A 2-tier cascade: entity override → global default, on a single target
/// entity.
///
/// Pick the plugin that matches the cascade's semantics:
/// [`CascadePanelPlugin`](super::CascadePanelPlugin) when the override lives
/// on a panel entity, [`CascadeEntityPlugin`](super::CascadeEntityPlugin)
/// when it lives on an arbitrary entity.
pub(crate) trait CascadeTarget:
    Copy + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static
{
    /// Component on the target entity carrying the tier-1 override.
    type Override: Component;

    /// Marker that disqualifies an entity from this cascade even when it holds
    /// the [`Override`](Self::Override). When two cascades share an override
    /// component, each names the other's entity marker here so a shared-override
    /// entity is enrolled by exactly one of them. Cascades that target every
    /// override-holder use [`ExcludeNone`].
    type Exclude: Component;

    /// Project the tier-1 override value from its component.
    fn override_value(entity_override: &Self::Override) -> Option<Self>;
    /// Read the tier-3 global default.
    fn global_default(defaults: &CascadeDefaults) -> Self;
}

/// [`CascadeTarget::Exclude`] marker for cascades that exclude no entity.
///
/// No entity ever holds this component, so `Without<ExcludeNone>` matches every
/// override-holder.
#[derive(Component)]
pub(crate) struct ExcludeNone;

/// Text alpha-mode cascade attribute. A pure value, wrapped in
/// [`Override<A>`] / [`Resolved<A>`]; never inserted bare, so it is not a
/// `Component`.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct TextAlpha(pub AlphaMode);

/// Font-unit cascade attribute. A pure value, wrapped in [`Override<A>`] /
/// [`Resolved<A>`]; never inserted bare, so it is not a `Component`.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct FontUnit(pub Unit);

/// A node's own override for attribute `A` — the cascade input.
///
/// Presence on an entity means "this node overrides `A`"; absence means
/// "inherit." There is exactly one override component type per attribute, and
/// an entity holds at most one of any component, so "two sources for one
/// attribute" cannot be written down.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub(crate) struct Override<A>(pub A)
where
    A: Copy
        + PartialEq
        + Send
        + Sync
        + FromReflect
        + TypePath
        + Typed
        + GetTypeRegistration
        + 'static;

/// Per-entity cache of a resolved cascading attribute.
///
/// Maintained by [`CascadePanelChildPlugin`](super::CascadePanelChildPlugin),
/// [`CascadePanelPlugin`](super::CascadePanelPlugin), or
/// [`CascadeEntityPlugin`](super::CascadeEntityPlugin). Crate-internal;
/// readers inside the crate query `&Resolved<A>` and filter on
/// `Changed<Resolved<A>>`.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub(crate) struct Resolved<A>(pub A)
where
    A: Copy
        + PartialEq
        + Send
        + Sync
        + FromReflect
        + TypePath
        + Typed
        + GetTypeRegistration
        + 'static;

/// Resolve a 3-tier cascade from its raw inputs.
pub(super) fn resolve_panel_child<A: CascadePanelChild>(
    entity_override: &A::EntityOverride,
    panel_override: Option<&A::PanelOverride>,
    defaults: &CascadeDefaults,
) -> A {
    A::entity_value(entity_override)
        .or_else(|| panel_override.and_then(A::panel_value))
        .unwrap_or_else(|| A::global_default(defaults))
}

/// Resolve a 2-tier cascade from its raw inputs.
pub(super) fn resolve_target<A: CascadeTarget>(
    entity_override: &A::Override,
    defaults: &CascadeDefaults,
) -> A {
    A::override_value(entity_override).unwrap_or_else(|| A::global_default(defaults))
}

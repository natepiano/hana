//! [`Resolved<A>`] — the per-entity cache component — plus the attribute
//! traits ([`CascadeAttribute`], [`CascadePanelChild`], [`CascadeTarget`])
//! and the pure resolvers used by the plugin write paths.

use bevy::prelude::*;
use bevy::reflect::GetTypeRegistration;
use bevy::reflect::Typed;

use super::defaults::CascadeDefaults;

/// Shared bounds for every cascading attribute newtype.
///
/// Attribute newtypes like `PanelTextAlpha(AlphaMode)` wrap a primitive; the
/// bounds here are what the generic machinery needs:
///
/// - `Copy + PartialEq` — inequality-gated writes and sentinel comparisons.
/// - `Send + Sync + 'static` — ECS requirements.
/// - `FromReflect + TypePath + Typed + GetTypeRegistration` — required by `#[derive(Reflect)]`
///   expansion on the generic [`Resolved<A>`] container, and by the per-monomorphization
///   `app.register_type::<Resolved<A>>()` call each plugin makes. A concrete `#[derive(Reflect)]`
///   type gets all of these for free.
pub trait CascadeAttribute:
    Copy + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static
{
}

impl<T> CascadeAttribute for T where
    T: Copy
        + PartialEq
        + Send
        + Sync
        + FromReflect
        + TypePath
        + Typed
        + GetTypeRegistration
        + 'static
{
}

/// A 3-tier cascade: entity override → panel override → global default.
///
/// `Resolved<A>` lives on **both** the panel and every tier-1 child. The
/// panel's copy represents the effective "default my children inherit"; each
/// child's copy is the final rendered value.
pub trait CascadePanelChild: CascadeAttribute {
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
pub trait CascadeTarget: CascadeAttribute {
    /// Component on the target entity carrying the tier-1 override.
    type Override: Component;

    /// Project the tier-1 override value from its component.
    fn override_value(entity_override: &Self::Override) -> Option<Self>;
    /// Read the tier-3 global default.
    fn global_default(defaults: &CascadeDefaults) -> Self;
}

/// Per-entity cache of a resolved cascading attribute.
///
/// Maintained by [`CascadePanelChildPlugin`](super::CascadePanelChildPlugin),
/// [`CascadePanelPlugin`](super::CascadePanelPlugin), or
/// [`CascadeEntityPlugin`](super::CascadeEntityPlugin). Crate-internal;
/// readers inside the crate query `&Resolved<A>` and filter on
/// `Changed<Resolved<A>>`.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Resolved<A: CascadeAttribute>(pub A);

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

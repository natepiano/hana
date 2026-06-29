//! The cascade's value-type traits, matched [`Override<A>`] / [`Resolved<A>`]
//! component pair, and bounded parent-walk resolvers.

use core::mem::size_of;

use bevy::asset::Handle;
use bevy::log::warn_once;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::reflect::GetTypeRegistration;
use bevy::reflect::Typed;
use private::Sealed;

use super::constants::CASCADE_ATTRIBUTE_BYTES;
use super::constants::CASCADE_DEPTH_CAP;
use super::defaults::CascadeDefault;
use crate::layout::Lighting;
use crate::layout::Sidedness;
use crate::layout::Unit;
use crate::render::AntiAlias;
use crate::render::HairlineFade;

mod private {
    pub trait Sealed {}
}

macro_rules! cascade_attr {
    // Joins an already-declared value type (one whose own name is the
    // attribute, e.g. `AntiAlias`) to the cascade instead of minting a
    // wrapper struct. The type must derive `Clone`, `PartialEq`, `Debug`, and
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
    /// HDR text coverage-bias cascade attribute.
    ///
    /// `0.0` leaves analytic glyph coverage unchanged. Positive values make
    /// fractional glyph-edge pixels more opaque, which can compensate for dark
    /// text looking too thin when an HDR camera renders into a float target.
    /// Negative values make fractional edges thinner. Use this for text that
    /// degrades under HDR, especially dark text on light backgrounds; avoid
    /// applying it broadly to light text on dark backgrounds unless that scene
    /// has been tuned, because the same compensation can make those glyphs look
    /// heavier.
    HdrTextCoverageBias(f32),
    default = 0.0
);

const HDR_TEXT_COVERAGE_BIAS_MIN: f32 = -4.0;
const HDR_TEXT_COVERAGE_BIAS_MAX: f32 = 4.0;

impl HdrTextCoverageBias {
    /// No HDR coverage compensation; the shader uses analytic text coverage
    /// unchanged.
    pub(crate) const NO_BIAS: Self = Self(0.0);

    /// Value sent to `PathRenderRecord::text_coverage_bias`.
    ///
    /// The public authored value is intentionally plain `f32` so it can be
    /// tuned live, including through reflection. The shader path clamps it to a
    /// bounded signed transfer and treats non-finite input as no compensation.
    #[must_use]
    pub(crate) fn shader_value(self) -> f32 {
        if self.0.is_finite() {
            self.0
                .clamp(HDR_TEXT_COVERAGE_BIAS_MIN, HDR_TEXT_COVERAGE_BIAS_MAX)
        } else {
            warn_once!(
                "HdrTextCoverageBias value {} is not finite; rendering text without HDR coverage compensation",
                self.0
            );
            0.0
        }
    }
}

/// Source-material handle cascade for SDF backgrounds, borders, and element surfaces.
///
/// `SdfMaterial` is authored source-material identity. It is not the batched
/// `SdfExtendedMaterial` render asset and not the migration-only
/// `LegacySdfExtendedMaterial` render asset.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub struct SdfMaterial(pub Handle<StandardMaterial>);

impl Sealed for SdfMaterial {}

impl CascadeProperty for SdfMaterial {}

impl CascadeAttr for SdfMaterial {}

impl Default for CascadeDefault<SdfMaterial> {
    fn default() -> Self { Self(SdfMaterial(Handle::default())) }
}

const _: () = assert!(size_of::<SdfMaterial>() <= CASCADE_ATTRIBUTE_BYTES);

/// Source-material handle cascade for text runs.
///
/// `TextMaterial` resolves the authored `StandardMaterial` handle before
/// analytic text projection. It is not a Bevy render material asset type.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub struct TextMaterial(pub Handle<StandardMaterial>);

impl Sealed for TextMaterial {}

impl CascadeProperty for TextMaterial {}

impl CascadeAttr for TextMaterial {}

impl Default for CascadeDefault<TextMaterial> {
    fn default() -> Self { Self(TextMaterial(Handle::default())) }
}

const _: () = assert!(size_of::<TextMaterial>() <= CASCADE_ATTRIBUTE_BYTES);

/// Source-material handle cascade for panel-shape primitives.
///
/// `ShapeMaterial` resolves the authored `StandardMaterial` handle before
/// analytic panel-shape projection. It is not a Bevy render material asset type.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub struct ShapeMaterial(pub Handle<StandardMaterial>);

impl Sealed for ShapeMaterial {}

impl CascadeProperty for ShapeMaterial {}

impl CascadeAttr for ShapeMaterial {}

impl Default for CascadeDefault<ShapeMaterial> {
    fn default() -> Self { Self(ShapeMaterial(Handle::default())) }
}

const _: () = assert!(size_of::<ShapeMaterial>() <= CASCADE_ATTRIBUTE_BYTES);

// Lighting cascade attribute. Global default is `Lit` (world text); the
// screen-panel construction bridge overrides it to `Unlit`. Consumed by both
// glyph runs and panel lines.
cascade_attr!(existing Lighting, default = Lighting::Lit);
// Sidedness cascade attribute. Global default is `BothSides` (world text);
// the screen-panel construction bridge overrides it to `FrontOnly`. Consumed by
// both glyph runs and panel lines.
cascade_attr!(existing Sidedness, default = Sidedness::BothSides);
// Anti-alias mode cascade attribute. The `AntiAlias` resource is the
// authored global; `sync_anti_alias` mirrors it into
// `CascadeDefault<AntiAlias>` as the cascade root default.
cascade_attr!(existing AntiAlias, default = AntiAlias::Both);
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
///
/// Cascaded values must stay cheap to clone because propagation clones them
/// when resolving and writing `Resolved<A>`. Small value wrappers and
/// `Handle<StandardMaterial>` are acceptable; owned `StandardMaterial` values
/// are not cascade attributes.
pub trait CascadeProperty: private::Sealed + Clone + PartialEq + Send + Sync + 'static {}

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
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component)]
pub(crate) struct Override<A: CascadeAttr>(pub A);

/// Per-entity cache of a resolved cascading attribute.
///
/// Maintained by [`CascadePlugin`](super::CascadePlugin): seeded at spawn by
/// the node-kind authoring bridges, kept current by the propagation pass.
/// Crate-internal; readers query `&Resolved<A>` and filter on
/// `Changed<Resolved<A>>`. `Resolved<A>` is a read-only cache maintained by
/// cascade propagation; author values through `Override<A>`.
///
/// This component is `pub(crate)` and cannot be named by external
/// inspectors/tests. Revisit that boundary only if external tooling needs to
/// inspect computed cascade state directly.
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component)]
pub(crate) struct Resolved<A: CascadeAttr>(pub A);

/// Resolve attribute `A` for `entity` from [`World`] by walking up `ChildOf`.
#[allow(
    dead_code,
    reason = "World-based resolver not yet called; retained for command self-heal"
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
            return node_override.0.clone();
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
            return node_override.0.clone();
        }
        let Ok(child_of) = parents.get(current) else {
            return default;
        };
        current = child_of.parent();
    }
    warn!("cascade walk exceeded depth cap from {entity:?}; using global default");
    default
}

use bevy::ecs::world::EntityWorldMut;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

use super::CascadeDefault;
use super::resolved;
use super::resolved::CascadeAttr;
pub use super::resolved::FontUnit;
pub use super::resolved::HdrTextCoverageBias;
use super::resolved::Override;
use super::resolved::Resolved;
pub use super::resolved::SdfMaterial;
pub use super::resolved::ShapeMaterial;
pub use super::resolved::TextAlpha;
pub use super::resolved::TextMaterial;
use crate::layout::Lighting;
use crate::layout::Sidedness;
use crate::layout::Unit;
use crate::render::AntiAlias;
use crate::render::HairlineFade;

/// Typed cascade commands for entity-local authored values.
///
/// `override_*` writes this entity's authored value. `inherit_*` removes that
/// authored value so the entity resolves from its parent or the global
/// [`CascadeDefault<A>`](CascadeDefault). Schedule writes before
/// [`CascadeSet::Propagate`](super::CascadeSet::Propagate) and reads after it
/// for same-frame descendant observation; direct entities are self-healed when
/// the command flushes.
pub trait CascadeEntityCommandsExt {
    /// Author this entity's text alpha mode.
    fn override_text_alpha(&mut self, alpha_mode: AlphaMode) -> &mut Self;

    /// Remove this entity's authored text alpha mode.
    fn inherit_text_alpha(&mut self) -> &mut Self;

    /// Author this entity's font unit.
    fn override_font_unit(&mut self, unit: Unit) -> &mut Self;

    /// Remove this entity's authored font unit.
    fn inherit_font_unit(&mut self) -> &mut Self;

    /// Author this entity's HDR text coverage bias.
    ///
    /// `0.0` leaves analytic text coverage unchanged. Positive values make
    /// fractional glyph edges more opaque and are useful for dark text that
    /// looks too thin under HDR. Negative values make edges thinner.
    fn override_hdr_text_coverage_bias(&mut self, bias: f32) -> &mut Self;

    /// Remove this entity's authored HDR text coverage bias.
    fn inherit_hdr_text_coverage_bias(&mut self) -> &mut Self;

    /// Author this entity's lighting mode.
    fn override_lighting(&mut self, lighting: Lighting) -> &mut Self;

    /// Remove this entity's authored lighting mode.
    fn inherit_lighting(&mut self) -> &mut Self;

    /// Author this entity's sidedness.
    fn override_sidedness(&mut self, sidedness: Sidedness) -> &mut Self;

    /// Remove this entity's authored sidedness.
    fn inherit_sidedness(&mut self) -> &mut Self;

    /// Author this entity's anti-alias mode.
    fn override_anti_alias(&mut self, anti_alias: AntiAlias) -> &mut Self;

    /// Remove this entity's authored anti-alias mode.
    fn inherit_anti_alias(&mut self) -> &mut Self;

    /// Author this entity's hairline fade policy.
    fn override_hairline_fade(&mut self, fade: HairlineFade) -> &mut Self;

    /// Remove this entity's authored hairline fade policy.
    fn inherit_hairline_fade(&mut self) -> &mut Self;

    /// Author this entity's SDF surface material source handle.
    fn override_sdf_material(&mut self, material: Handle<StandardMaterial>) -> &mut Self;

    /// Remove this entity's authored SDF surface material source handle.
    fn inherit_sdf_material(&mut self) -> &mut Self;

    /// Author this entity's text material source handle.
    fn override_text_material(&mut self, material: Handle<StandardMaterial>) -> &mut Self;

    /// Remove this entity's authored text material source handle.
    fn inherit_text_material(&mut self) -> &mut Self;

    /// Author this entity's panel-shape material source handle.
    fn override_shape_material(&mut self, material: Handle<StandardMaterial>) -> &mut Self;

    /// Remove this entity's authored panel-shape material source handle.
    fn inherit_shape_material(&mut self) -> &mut Self;
}

impl CascadeEntityCommandsExt for EntityCommands<'_> {
    fn override_text_alpha(&mut self, alpha_mode: AlphaMode) -> &mut Self {
        apply_cascade_override(self, TextAlpha(alpha_mode))
    }

    fn inherit_text_alpha(&mut self) -> &mut Self { remove_cascade_override::<TextAlpha>(self) }

    fn override_font_unit(&mut self, unit: Unit) -> &mut Self {
        apply_cascade_override(self, FontUnit(unit))
    }

    fn inherit_font_unit(&mut self) -> &mut Self { remove_cascade_override::<FontUnit>(self) }

    fn override_hdr_text_coverage_bias(&mut self, bias: f32) -> &mut Self {
        apply_cascade_override(self, HdrTextCoverageBias(bias))
    }

    fn inherit_hdr_text_coverage_bias(&mut self) -> &mut Self {
        remove_cascade_override::<HdrTextCoverageBias>(self)
    }

    fn override_lighting(&mut self, lighting: Lighting) -> &mut Self {
        apply_cascade_override(self, lighting)
    }

    fn inherit_lighting(&mut self) -> &mut Self { remove_cascade_override::<Lighting>(self) }

    fn override_sidedness(&mut self, sidedness: Sidedness) -> &mut Self {
        apply_cascade_override(self, sidedness)
    }

    fn inherit_sidedness(&mut self) -> &mut Self { remove_cascade_override::<Sidedness>(self) }

    fn override_anti_alias(&mut self, anti_alias: AntiAlias) -> &mut Self {
        apply_cascade_override(self, anti_alias)
    }

    fn inherit_anti_alias(&mut self) -> &mut Self { remove_cascade_override::<AntiAlias>(self) }

    fn override_hairline_fade(&mut self, fade: HairlineFade) -> &mut Self {
        apply_cascade_override(self, fade)
    }

    fn inherit_hairline_fade(&mut self) -> &mut Self {
        remove_cascade_override::<HairlineFade>(self)
    }

    fn override_sdf_material(&mut self, material: Handle<StandardMaterial>) -> &mut Self {
        apply_cascade_override(self, SdfMaterial(material))
    }

    fn inherit_sdf_material(&mut self) -> &mut Self { remove_cascade_override::<SdfMaterial>(self) }

    fn override_text_material(&mut self, material: Handle<StandardMaterial>) -> &mut Self {
        apply_cascade_override(self, TextMaterial(material))
    }

    fn inherit_text_material(&mut self) -> &mut Self {
        remove_cascade_override::<TextMaterial>(self)
    }

    fn override_shape_material(&mut self, material: Handle<StandardMaterial>) -> &mut Self {
        apply_cascade_override(self, ShapeMaterial(material))
    }

    fn inherit_shape_material(&mut self) -> &mut Self {
        remove_cascade_override::<ShapeMaterial>(self)
    }
}

/// Resolve an entity's current text alpha mode.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_text_alpha(world: &World, entity: Entity) -> AlphaMode {
    resolved_cascade::<TextAlpha>(world, entity).0
}

/// Resolve an entity's current font unit.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_font_unit(world: &World, entity: Entity) -> Unit {
    resolved_cascade::<FontUnit>(world, entity).0
}

/// Resolve an entity's current HDR text coverage bias.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_hdr_text_coverage_bias(world: &World, entity: Entity) -> f32 {
    resolved_cascade::<HdrTextCoverageBias>(world, entity).0
}

/// Resolve an entity's current lighting mode.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_lighting(world: &World, entity: Entity) -> Lighting {
    resolved_cascade::<Lighting>(world, entity)
}

/// Resolve an entity's current sidedness.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_sidedness(world: &World, entity: Entity) -> Sidedness {
    resolved_cascade::<Sidedness>(world, entity)
}

/// Resolve an entity's current anti-alias mode.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_anti_alias(world: &World, entity: Entity) -> AntiAlias {
    resolved_cascade::<AntiAlias>(world, entity)
}

/// Resolve an entity's current hairline fade policy.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_hairline_fade(world: &World, entity: Entity) -> HairlineFade {
    resolved_cascade::<HairlineFade>(world, entity)
}

/// Resolve an entity's current SDF surface material handle.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_sdf_material(world: &World, entity: Entity) -> Handle<StandardMaterial> {
    resolved_cascade::<SdfMaterial>(world, entity).0
}

/// Resolve an entity's current text material handle.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_text_material(world: &World, entity: Entity) -> Handle<StandardMaterial> {
    resolved_cascade::<TextMaterial>(world, entity).0
}

/// Resolve an entity's current panel-shape material handle.
///
/// Reads the cached resolved value when present. If the entity has not been
/// seeded yet, this falls back to the same parent walk used by propagation.
#[must_use]
pub fn resolved_shape_material(world: &World, entity: Entity) -> Handle<StandardMaterial> {
    resolved_cascade::<ShapeMaterial>(world, entity).0
}

pub(crate) fn apply_cascade_override<'a, 'w, A>(
    entity: &'a mut EntityCommands<'w>,
    value: A,
) -> &'a mut EntityCommands<'w>
where
    A: CascadeAttr,
    CascadeDefault<A>: Default + Resource,
{
    entity.queue(move |mut entity: EntityWorldMut| {
        apply_cascade_override_now(&mut entity, value);
    })
}

pub(crate) fn remove_cascade_override<'a, 'w, A>(
    entity: &'a mut EntityCommands<'w>,
) -> &'a mut EntityCommands<'w>
where
    A: CascadeAttr,
    CascadeDefault<A>: Default + Resource,
{
    entity.queue(move |mut entity: EntityWorldMut| {
        if entity.get::<Override<A>>().is_none() {
            return;
        }
        entity.remove::<Override<A>>();
        heal_resolved(&mut entity);
    })
}

fn apply_cascade_override_now<A>(entity: &mut EntityWorldMut<'_>, value: A)
where
    A: CascadeAttr,
    CascadeDefault<A>: Default + Resource,
{
    if entity
        .get::<Override<A>>()
        .is_some_and(|node_override| node_override.0 == value)
    {
        heal_resolved(entity);
        return;
    }
    entity.insert(Override(value));
    heal_resolved(entity);
}

fn heal_resolved<A>(entity: &mut EntityWorldMut<'_>)
where
    A: CascadeAttr,
    CascadeDefault<A>: Default + Resource,
{
    let entity_id = entity.id();
    let default = cascade_default(entity.world());
    let resolved = resolved::resolve::<A>(entity.world(), entity_id, default);
    if entity
        .get::<Resolved<A>>()
        .is_some_and(|current| current.0 == resolved)
    {
        return;
    }
    entity.insert(Resolved(resolved));
}

pub(crate) fn resolved_cascade<A>(world: &World, entity: Entity) -> A
where
    A: CascadeAttr,
    CascadeDefault<A>: Default + Resource,
{
    if let Some(resolved) = world.get::<Resolved<A>>(entity) {
        return resolved.0.clone();
    }
    let default = cascade_default(world);
    resolved::resolve::<A>(world, entity, default)
}

fn cascade_default<A>(world: &World) -> A
where
    A: CascadeAttr,
    CascadeDefault<A>: Default + Resource,
{
    world
        .get_resource::<CascadeDefault<A>>()
        .cloned()
        .unwrap_or_default()
        .0
}

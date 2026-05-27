use bevy::ecs::world::EntityWorldMut;
use bevy::prelude::*;

use super::CascadeDefault;
use super::resolved;
use super::resolved::CascadeAttr;
pub use super::resolved::FontUnit;
use super::resolved::Override;
use super::resolved::Resolved;
pub use super::resolved::TextAlpha;
use crate::layout::Unit;

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
        return resolved.0;
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
        .copied()
        .unwrap_or_default()
        .0
}

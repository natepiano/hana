//! Shared Bevy capability components for hardware providers.
//!
//! This crate defines capability vocabulary that belongs neither to the device-identity kernel nor
//! to a single provider. It has no systems, plugin, or dependency on `hana_rigging`.

use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::Component;
use bevy::prelude::Reflect;

/// Capability marker for a device that exposes one or more independently addressable light
/// endpoints.
///
/// `Illuminants` is shared because a Stream Deck backlight, LED ring, and DMX dimmer all expose
/// light-producing endpoints even though their providers use different device APIs and slot names.
/// The endpoint binding selects an individual light; this component only states that the device
/// has that class of endpoint.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Component, Reflect)]
#[reflect(Component)]
pub struct Illuminants;

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::error::Error;

    use bevy::ecs::reflect::ReflectComponent;
    use bevy::ecs::world::World;
    use bevy::reflect::TypeRegistry;

    use super::Illuminants;

    #[test]
    fn illuminants_inserts_through_registered_reflection() -> Result<(), Box<dyn Error>> {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<Illuminants>();
        let reflect_component = type_registry
            .get_type_data::<ReflectComponent>(TypeId::of::<Illuminants>())
            .ok_or("Illuminants must register ReflectComponent")?;
        let mut world = World::new();
        let mut entity = world.spawn_empty();

        reflect_component.insert(&mut entity, &Illuminants, &type_registry);

        assert_eq!(entity.get::<Illuminants>(), Some(&Illuminants));

        Ok(())
    }
}

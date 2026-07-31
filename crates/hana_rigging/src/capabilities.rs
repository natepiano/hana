use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::world::EntityWorldMut;
use bevy::prelude::Reflect;
use bevy::reflect::TypeRegistry;
use thiserror::Error;

/// Erased capability components a provider reports for one `crate::DeviceRecord`.
///
/// Providers can retain private capability types while reporting them to the kernel because
/// `Capabilities` stores `Reflect` trait objects instead of a kernel-owned device-class enum.
/// Reconciliation later compares overlapping component types and inserts their values on the
/// resolved device entity.
#[derive(Default)]
pub struct Capabilities(Vec<Box<dyn Reflect>>);

impl Capabilities {
    /// Create an empty declaration for a device that currently exposes no capability components.
    #[must_use]
    pub fn new() -> Self { Self::default() }

    /// Add one reflected capability component to this provider's declaration.
    ///
    /// The value may use a private provider type. `Self::attach` reports an error if its owner did
    /// not register it as a Bevy component before the completed scan reaches reconciliation.
    pub fn add(&mut self, capability: impl Reflect) { self.0.push(Box::new(capability)); }

    /// Add one reflected capability component and return this declaration for builder-style setup.
    #[must_use]
    pub fn with(mut self, capability: impl Reflect) -> Self {
        self.add(capability);
        self
    }

    /// Insert every declared capability component into `entity` through its reflected component
    /// registration.
    ///
    /// This uses `ReflectComponent::insert` rather than pairing `ComponentId` with an owning
    /// pointer, so Bevy owns the typed insertion. A provider type that was not registered as a
    /// reflected component returns `CapabilityAttachError` instead of being discarded.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityAttachError` when a declared capability is not registered as a Bevy
    /// reflected component in `type_registry`.
    pub fn attach(
        &self,
        entity: &mut EntityWorldMut,
        type_registry: &TypeRegistry,
    ) -> Result<(), CapabilityAttachError> {
        for capability in &self.0 {
            let type_path = capability.reflect_type_path().to_owned();
            let type_id = capability.as_any().type_id();
            if !type_registry.contains(type_id) {
                return Err(CapabilityAttachError::Unregistered { type_path });
            }
            let Some(reflect_component) = type_registry.get_type_data::<ReflectComponent>(type_id)
            else {
                return Err(CapabilityAttachError::NotAComponent { type_path });
            };

            reflect_component.insert(entity, capability.as_partial_reflect(), type_registry);
        }

        Ok(())
    }

    /// Report whether shared capability types carry equal values in both declarations.
    ///
    /// A type present from only one provider contributes to the later union. When both providers
    /// report the same component type, `PartialReflect::reflect_partial_eq` must return
    /// `Some(true)` for every pair; unavailable equality evidence or unequal values disarms the
    /// device during reconciliation.
    #[must_use]
    pub fn agrees_with(&self, other: &Self) -> bool {
        self.0.iter().all(|capability| {
            let type_id = capability.as_any().type_id();
            other
                .0
                .iter()
                .filter(|other_capability| other_capability.as_any().type_id() == type_id)
                .all(|other_capability| {
                    capability.reflect_partial_eq(other_capability.as_partial_reflect())
                        == Some(true)
                })
        })
    }
}

/// Failure while turning a provider's erased capability declaration into Bevy components.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CapabilityAttachError {
    /// The capability type was absent from the application's type registry, so Bevy cannot know
    /// how to retain the provider's value on the resolved device entity.
    #[error("capability `{type_path}` is not registered")]
    Unregistered {
        /// Reflected type path used to identify the provider capability that needs registration.
        type_path: String,
    },
    /// The type registry knows this type but it is not a reflected Bevy component, so attaching it
    /// would not produce an entity component.
    #[error("capability `{type_path}` is not a reflected component")]
    NotAComponent {
        /// Reflected type path used to identify the provider type that lacks `Component` support.
        type_path: String,
    },
}

#[cfg(test)]
mod tests {
    use bevy::ecs::component::Component;
    use bevy::ecs::reflect::ReflectComponent;
    use bevy::ecs::world::World;
    use bevy::prelude::Reflect;
    use bevy::reflect::TypeRegistry;

    use super::Capabilities;
    use super::CapabilityAttachError;

    #[derive(Component, Debug, PartialEq, Reflect)]
    #[reflect(Component, PartialEq)]
    struct ChannelCount(u8);

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct UnregisteredCapability;

    #[derive(Reflect)]
    struct RegisteredNonComponent;

    #[test]
    fn matching_shared_capabilities_agree_by_reflected_value() {
        let first = Capabilities::new().with(ChannelCount(2));
        let same = Capabilities::new().with(ChannelCount(2));
        let different = Capabilities::new().with(ChannelCount(4));

        assert!(first.agrees_with(&same));
        assert!(!first.agrees_with(&different));
    }

    #[test]
    fn attach_inserts_registered_capability_through_reflection() -> Result<(), CapabilityAttachError>
    {
        let capabilities = Capabilities::new().with(ChannelCount(2));
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<ChannelCount>();
        let mut world = World::new();
        let mut entity = world.spawn_empty();

        capabilities.attach(&mut entity, &type_registry)?;

        assert_eq!(entity.get::<ChannelCount>(), Some(&ChannelCount(2)));

        Ok(())
    }

    #[test]
    fn attach_rejects_unregistered_capability() {
        let capabilities = Capabilities::new().with(UnregisteredCapability);
        let type_registry = TypeRegistry::default();
        let mut world = World::new();
        let mut entity = world.spawn_empty();

        assert!(matches!(
            capabilities.attach(&mut entity, &type_registry),
            Err(CapabilityAttachError::Unregistered { .. })
        ));
    }

    #[test]
    fn attach_rejects_registered_capability_without_component_reflection() {
        let capabilities = Capabilities::new().with(RegisteredNonComponent);
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<RegisteredNonComponent>();
        let mut world = World::new();
        let mut entity = world.spawn_empty();

        assert!(matches!(
            capabilities.attach(&mut entity, &type_registry),
            Err(CapabilityAttachError::NotAComponent { .. })
        ));
    }
}

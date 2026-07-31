use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::Component;
use bevy::prelude::Reflect;

/// Retention and action policy fixed when a device binding is registered.
///
/// `Availability` keeps retention and `Act` in the same enum, so an
/// `Availability::Unregistered` binding cannot carry an instruction to re-apply configuration
/// that the kernel did not retain.
#[derive(Clone, PartialEq, Eq, Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub enum Availability {
    /// The device has no retained binding, as with a display whose window arrangement should be
    /// forgotten when its provider later reports the display absent.
    #[default]
    Unregistered,
    /// The kernel retains a device configuration, such as a camera panel's selected source, with
    /// the `Act` that limits reporting or later re-application after absence.
    Retained(Act),
}

/// Action permitted for a configuration that `Availability::Retained` keeps across absence.
///
/// `Act` is nested inside `Availability::Retained` instead of standing beside an optional saved
/// configuration, so an unregistered projector or light rig cannot select automatic output.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Reflect)]
pub enum Act {
    /// Keep a retained configuration available to reports without sending it to the device, as
    /// for a light rig whose operator must approve every blackout change.
    Never,
    /// Keep a configuration until application code asks to apply it, as for a projector whose
    /// next presentation determines when its shutter state should be restored.
    OnRequest,
    /// Apply a retained configuration when reconciliation verifies the returning physical unit,
    /// as when a replugged Stream Deck returns with the same reported serial.
    OnReturn,
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use bevy::app::App;
    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::ecs::reflect::ReflectComponent;

    use super::Act;
    use super::Availability;
    use crate::DeviceIdentity;
    use crate::DeviceKey;

    #[test]
    fn unregistered_is_the_default_availability() {
        assert!(matches!(
            Availability::default(),
            Availability::Unregistered
        ));
    }

    #[test]
    fn availability_clones_retained_policy() {
        let availability = Availability::Retained(Act::OnReturn);
        let clone = availability.clone();

        assert_eq!(clone, availability);
        assert!(matches!(clone, Availability::Retained(Act::OnReturn)));
    }

    #[test]
    fn device_components_register_reflection_metadata() {
        let app = App::new();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();

        for type_id in [
            TypeId::of::<Availability>(),
            TypeId::of::<DeviceKey>(),
            TypeId::of::<DeviceIdentity>(),
        ] {
            assert!(type_registry.contains(type_id));
            assert!(
                type_registry
                    .get_type_data::<ReflectComponent>(type_id)
                    .is_some()
            );
        }

        drop(type_registry);
    }
}

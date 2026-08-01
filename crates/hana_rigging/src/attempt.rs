use bevy::prelude::Reflect;

use crate::DeviceAccessError;

/// Identifier issued from the attempt registry's monotonic counter.
///
/// An `AttemptId` is never reused while the process runs, so a delayed provider poll cannot
/// resolve a later attempt after the original attempt has finished.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Reflect)]
pub struct AttemptId(u64);

/// Progress returned by a provider while the kernel polls an attempt.
///
/// Splitting the in-progress state from `AttemptOutcome` prevents a completion event from
/// carrying `Pending` as though the attempt had ended.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub enum AttemptProgress {
    /// The provider has not yet observed the requested configuration at its destination.
    Pending,
    /// The provider observed a terminal result for this attempt.
    Finished(AttemptOutcome),
}

/// Terminal result reported by a provider for one attempt.
///
/// `Aborted` is terminal and does not auto-retry because the kernel uses it when an attempt no
/// longer has authorization to continue, such as after a device departure or a safety gate
/// closes.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub enum AttemptOutcome {
    /// The device reached the requested configuration, including a provider-defined tolerance.
    Succeeded,
    /// The driver started but the device or platform rejected continued access.
    Failed(DeviceAccessError),
    /// The kernel stopped an attempt that must not continue or retry automatically.
    Aborted,
    /// The provider reached a different device or endpoint than the one the attempt addressed.
    Substituted,
}

/// Configuration a driver last applied successfully to one endpoint.
///
/// The kernel stores the driver's value without learning whether it represents window placement,
/// camera settings, or another device-specific configuration. The concrete type remains
/// recoverable by the erased driver registry entry and is mirrored later as the driver's own
/// component for inspection.
#[derive(Reflect)]
pub struct CapturedConfiguration(
    #[reflect(ignore, default = "default_erased_configuration")] Box<dyn Reflect>,
);

impl CapturedConfiguration {
    /// Erase a driver configuration after the driver reported a successful capture or apply.
    #[must_use]
    pub fn new(configuration: impl Reflect) -> Self { Self(Box::new(configuration)) }

    /// Borrow the driver value so the erased driver registry entry can recover its concrete type.
    #[must_use]
    pub fn as_reflect(&self) -> &dyn Reflect { self.0.as_ref() }
}

fn default_erased_configuration() -> Box<dyn Reflect> { Box::new(()) }

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use bevy::app::App;
    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::prelude::Reflect;
    use bevy::reflect::FromReflect;
    use bevy::reflect::ReflectFromReflect;

    use super::CapturedConfiguration;

    #[derive(Debug, PartialEq, Eq, Reflect)]
    struct ProviderConfiguration {
        frame_rate: u32,
    }

    #[derive(Reflect)]
    struct CapturedConfigurationRecord {
        captured_configuration: CapturedConfiguration,
    }

    #[test]
    fn captured_configuration_allows_its_enclosing_record_to_reflect() {
        fn assert_from_reflect<T: FromReflect>() {}

        assert_from_reflect::<CapturedConfigurationRecord>();

        let app = App::new();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let type_id = TypeId::of::<CapturedConfigurationRecord>();

        assert!(type_registry.contains(type_id));
        assert!(
            type_registry
                .get_type_data::<ReflectFromReflect>(type_id)
                .is_some()
        );

        drop(type_registry);
    }

    #[test]
    fn captured_configuration_recovers_the_provider_value_after_erasure() {
        let captured_configuration =
            CapturedConfiguration::new(ProviderConfiguration { frame_rate: 60 });

        let recovered = captured_configuration
            .as_reflect()
            .as_any()
            .downcast_ref::<ProviderConfiguration>();

        assert_eq!(recovered, Some(&ProviderConfiguration { frame_rate: 60 }));
    }
}

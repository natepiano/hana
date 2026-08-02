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

/// The most recent configuration a safe readback established on one endpoint.
///
/// This enum distinguishes the absence of endpoint evidence from an established driver value.
/// `RequestedConfiguration` is application intent; it must never fill this value after an apply
/// because a driver can normalize or substitute the value that actually reached the hardware.
#[derive(Default, Reflect)]
pub enum LastKnownGoodConfiguration {
    /// No successful safe readback has established what is on this endpoint.
    #[default]
    NotEstablished,
    /// A safe readback established this driver-specific endpoint value.
    Known(#[reflect(ignore, default = "default_erased_configuration")] Box<dyn Reflect>),
}

impl LastKnownGoodConfiguration {
    /// Erase one value returned by a successful safe endpoint readback.
    #[must_use]
    pub fn known(configuration: impl Reflect) -> Self { Self::Known(Box::new(configuration)) }

    pub(crate) fn as_reflect(&self) -> Result<&dyn Reflect, LastKnownGoodConfigurationAccessError> {
        match self {
            Self::NotEstablished => Err(LastKnownGoodConfigurationAccessError::NotEstablished),
            Self::Known(configuration) => Ok(configuration.as_ref()),
        }
    }
}

/// Reason erased dispatch could not borrow a readback value from lifecycle state.
pub(crate) enum LastKnownGoodConfigurationAccessError {
    /// No driver readback proved an endpoint value for the binding yet.
    NotEstablished,
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

    use super::LastKnownGoodConfiguration;

    #[derive(Debug, PartialEq, Eq, Reflect)]
    struct ProviderConfiguration {
        frame_rate: u32,
    }

    #[derive(Reflect)]
    struct LastKnownGoodConfigurationRecord {
        last_known_good_configuration: LastKnownGoodConfiguration,
    }

    #[test]
    fn last_known_good_configuration_allows_its_enclosing_record_to_reflect() {
        fn assert_from_reflect<T: FromReflect>() {}

        assert_from_reflect::<LastKnownGoodConfigurationRecord>();

        let app = App::new();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let type_id = TypeId::of::<LastKnownGoodConfigurationRecord>();

        assert!(type_registry.contains(type_id));
        assert!(
            type_registry
                .get_type_data::<ReflectFromReflect>(type_id)
                .is_some()
        );

        drop(type_registry);
    }

    #[test]
    fn known_configuration_recovers_the_provider_value_after_erasure() {
        let last_known_good =
            LastKnownGoodConfiguration::known(ProviderConfiguration { frame_rate: 60 });

        let recovered = last_known_good.as_reflect().ok().and_then(|configuration| {
            configuration
                .as_any()
                .downcast_ref::<ProviderConfiguration>()
        });

        assert_eq!(recovered, Some(&ProviderConfiguration { frame_rate: 60 }));
    }

    #[test]
    fn last_known_good_configuration_defaults_to_not_established() {
        assert!(matches!(
            LastKnownGoodConfiguration::default(),
            LastKnownGoodConfiguration::NotEstablished
        ));
    }
}

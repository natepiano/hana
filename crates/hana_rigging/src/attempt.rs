use bevy::platform::time::Instant;
use bevy::prelude::Reflect;

use crate::ApplyPermit;
use crate::DeviceAccessError;
use crate::DeviceEndpoint;
use crate::DeviceId;
use crate::RiggingRevision;
use crate::RoleKey;

/// Identifier issued from the attempt registry's monotonic counter.
///
/// An `AttemptId` is never reused while the process runs, so a delayed provider poll cannot
/// resolve a later attempt after the original attempt has finished.
///
/// Reflection sees the counter value opaquely, so a dynamic tuple struct cannot construct an
/// identifier the registry never issued: a reflected poll for a fabricated attempt would otherwise
/// resolve against an in-flight record belonging to a different role.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Reflect)]
#[reflect(opaque)]
pub struct AttemptId(u64);

impl AttemptId {
    /// Wrap the attempt registry's next counter value.
    ///
    /// Private to the crate because only `crate::Attempts` issues identifiers; a driver that could
    /// mint one would be claiming an authorization the kernel never granted.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "its only caller, `Attempts::issue`, is itself unused until applies are wired \
                      up"
        )
    )]
    pub(crate) const fn new(value: u64) -> Self { Self(value) }
}

/// One in-flight endpoint operation and the authorization it was started under.
///
/// The record exists so every poll can re-answer "is this still the unit the kernel authorized?"
/// without trusting the driver's own bookkeeping. Both re-checked fields are copied in at
/// authorization time rather than read live, because a driver that reported against a replaced unit
/// would otherwise look correct.
#[derive(Clone, Debug, Reflect)]
pub struct Attempt {
    /// Registry-issued identifier the driver echoes back on every poll and completion.
    pub id:                 AttemptId,
    /// Application role this operation is running for, so a completion reaches the binding entity
    /// that outlives the device.
    pub role:               RoleKey,
    /// Durable endpoint the operation addresses, retained so a poll can be compared with current
    /// resolution rather than with whatever the driver believes it opened.
    pub endpoint:           DeviceEndpoint,
    /// The authorisation this attempt was minted under. The kernel mints one value and both stores
    /// it here and hands it to the driver, so there is no second copy that could disagree.
    pub permit:             ApplyPermit,
    /// The identity this attempt was authorised against.
    /// Re-checked on every poll; a mismatch invalidates it.
    pub expected_device_id: DeviceId,
    /// Global rigging revision at authorisation. A newer revision invalidates.
    ///
    /// This is `RiggingRevision`, not `ReporterRevision`: a co-reported device has several
    /// contributing reporters, so one reporter's counter cannot say whether the topology that made
    /// this target valid still holds, and the driver need not be the reporter whose scan validated
    /// it.
    pub rigging_revision:   RiggingRevision,
    /// Bounds the attempt end to end, not per step.
    ///
    /// `bevy_platform::time::Instant` rather than `std::time::Instant` because this record is
    /// reflected and only the bevy type carries a `Reflect` impl; on native targets it is a
    /// re-export of the std type, so call sites are unchanged.
    pub deadline:           Instant,
}

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

    #[test]
    fn runtime_reflection_cannot_construct_an_attempt_identifier_the_registry_never_issued() {
        use bevy::reflect::FromReflect;
        use bevy::reflect::tuple_struct::DynamicTupleStruct;

        use super::AttemptId;

        let mut dynamic_attempt = DynamicTupleStruct::default();
        dynamic_attempt.insert(7_u64);

        assert!(AttemptId::from_reflect(&dynamic_attempt).is_none());
    }
}

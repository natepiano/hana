use std::any::Any;
use std::any::type_name;

use bevy::prelude::Component;
use bevy::prelude::Reflect;
use bevy::prelude::World;
use thiserror::Error;

use crate::ApplyPermit;
use crate::AttemptId;
use crate::AttemptProgress;
use crate::CapturedConfiguration;
use crate::DeviceAccessError;
use crate::DeviceEndpoint;
use crate::DeviceScan;
use crate::DeviceSet;
use crate::ReporterId;
use crate::ReporterRevision;

/// Reports the whole current set of hardware devices from an integration crate.
///
/// This trait is an external extension point: a monitor integration reports display records while
/// a camera integration reports camera records, and the kernel must not name either type. An
/// implementation never touches a device directly; hardware enumeration happens on its own
/// reporter-owned thread before `scan` drains the completed result. An
/// implementation must not insert, remove, or read the reporter registry while `scan` runs, and
/// it must not mutate kernel-owned resources, device entities, binding entities, or their kernel
/// components through the supplied `World`. Kernel dispatch temporarily removes the
/// reporter registry during this call, so accessing it would panic; returning `DeviceScan` is the
/// only reporting path.
///
/// An implementation panic is fatal, and the kernel does not catch it. Continuing after a
/// partially completed scan would leave the reporter's device set and scan progress
/// untrustworthy.
pub trait DeviceReporter: Send + Sync + 'static {
    /// Report the whole current device set once per `RiggingSystems::Collect`.
    ///
    /// The exclusive `World` access keeps this call on the main thread, so a monitor reporter may
    /// reach `!Send` state with `World::non_send_resource_mut` and a HID reporter may drain the
    /// channel owned by its background enumeration thread. This method must return immediately:
    /// hardware enumeration belongs on a reporter-owned thread, while `scan` drains that thread's
    /// channel. Return `DeviceScan::Unchanged` when no completed report is available.
    fn scan(&mut self, world: &mut World) -> DeviceScan;
}

/// Drives one endpoint from an integration crate after the kernel authorizes an apply.
///
/// This trait is an external extension point separate from `DeviceReporter`: a monitor reporter
/// only scans, while a window driver captures and applies window state for the same display. An
/// implementation may touch hardware through every method here. An
/// implementation must not insert, remove, or read the driver registry while these methods run,
/// and it must not mutate kernel-owned resources, device entities, binding entities, or their
/// kernel components through the supplied `World`. Kernel dispatch temporarily removes the
/// driver registry during each call, so accessing it would panic; the method result is the
/// only report back to the kernel.
///
/// An implementation panic is fatal, and the kernel does not catch it. Continuing after a
/// partially completed hardware operation would leave the endpoint and its attempt state
/// untrustworthy.
pub trait EndpointDriver: Send + Sync + 'static {
    /// The driver-specific configuration the kernel captures, erases, and later returns here.
    ///
    /// Driver authors must add `#[derive(Component, Reflect)]` and `#[reflect(Component)]` to the
    /// concrete configuration type. `Reflect` permits storage in `CapturedConfiguration`, while
    /// `#[reflect(Component)]` creates the `ReflectComponent` metadata the kernel needs to mirror
    /// the concrete configuration on the binding entity. The trait bounds alone do not create that
    /// metadata.
    type Configuration: Reflect + Component;

    /// Read the live endpoint configuration for a later restore or diagnostic inspection.
    ///
    /// The kernel calls `capture` only for a present endpoint with no in-flight attempt. A driver
    /// does not repeat those kernel checks because the decision can change after this call.
    fn capture(
        &mut self,
        world: &mut World,
        endpoint: &DeviceEndpoint,
    ) -> CaptureOutcome<Self::Configuration>;

    /// Start applying `configuration` to `endpoint` and return before the device operation ends.
    ///
    /// An apply is absolute: a window driver cannot rely on its current placement, and a camera
    /// driver cannot rely on its current stream settings. Drivers that need blocking hardware
    /// work start it on their own thread and report its result through later `poll` calls.
    fn start_apply(
        &mut self,
        world: &mut World,
        endpoint: &DeviceEndpoint,
        configuration: &Self::Configuration,
        attempt: AttemptId,
        permit: ApplyPermit,
    );

    /// Report whether an apply has reached the driver's own arrival condition.
    ///
    /// The kernel owns the end-to-end deadline while the driver defines arrival: a delivered
    /// camera frame, stable window position, or completed panel image write. A driver returns
    /// `AttemptProgress::Pending` between its own steps and reports a substituted endpoint when
    /// its comparison finds a different physical target.
    fn poll(&mut self, world: &mut World, attempt: AttemptId) -> AttemptProgress;
}

/// Result of asking an endpoint driver to read its current configuration.
///
/// This generic return value never reflects, lands on an entity, or enters a resource. It keeps
/// the driver's configuration type until the erased driver entry receives it, which is why this
/// exception to the kernel's non-generic-type rule is safe.
pub enum CaptureOutcome<Configuration> {
    /// The endpoint exposed a configuration value that the kernel may retain.
    Read(Configuration),
    /// The endpoint is reachable but has no readable configuration, such as a display API that
    /// reports geometry without exposing the current window arrangement.
    ///
    /// This is permanent for the endpoint, so the kernel stops calling `EndpointDriver::capture`.
    NotReadable,
    /// A transient read failed while the endpoint remained reachable, such as a camera that is
    /// switching modes.
    ///
    /// The kernel retains its prior `CapturedConfiguration` and tries again during a later
    /// reconcile.
    ReadFailed(DeviceAccessError),
}

/// Failure at the erased driver boundary before a typed `EndpointDriver` method can run.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DriverContractError {
    /// A binding addressed a `DriverId` that no registration call issued during this process.
    #[error("endpoint driver `{driver_id:?}` is not registered")]
    DriverNotRegistered {
        /// The process-local route that did not select a registered endpoint driver.
        driver_id: crate::DriverId,
    },
    /// A driver registry entry's erased value differs from the concrete driver type installed in
    /// its dispatch functions.
    ///
    /// Registration constructs the value and functions together, but erased dispatch keeps this
    /// internal contract failure recoverable so one invalid entry does not terminate the app.
    #[error("endpoint driver registry entry expected concrete driver `{expected_driver}`")]
    DriverTypeMismatch {
        /// The concrete `EndpointDriver` type required by the installed dispatch function.
        expected_driver: &'static str,
    },
    /// A binding or capture value reached a driver whose `Configuration` type differs from the
    /// concrete value inside `CapturedConfiguration`.
    ///
    /// This remains recoverable because two drivers can serve one device while accepting distinct
    /// configuration types; terminating the app would turn one authored routing error into loss
    /// of every endpoint.
    #[error(
        "endpoint driver expected configuration `{expected_configuration}` but received `{received_configuration}`"
    )]
    ConfigurationTypeMismatch {
        /// The concrete `EndpointDriver::Configuration` type the registered driver accepts.
        expected_configuration: &'static str,
        /// The reflected type path stored in the supplied `CapturedConfiguration`.
        received_configuration: String,
    },
}

type ErasedDriver = dyn Any + Send + Sync;
type CaptureFunction = fn(
    &mut ErasedDriver,
    &mut World,
    &DeviceEndpoint,
) -> Result<CaptureOutcome<CapturedConfiguration>, DriverContractError>;
type StartApplyFunction = fn(
    &mut ErasedDriver,
    &mut World,
    &DeviceEndpoint,
    &CapturedConfiguration,
    AttemptId,
    ApplyPermit,
) -> Result<(), DriverContractError>;
type PollFunction =
    fn(&mut ErasedDriver, &mut World, AttemptId) -> Result<AttemptProgress, DriverContractError>;

/// Erased reporter closure plus the identifier and revision the reporter cannot mint itself.
pub(crate) struct ReporterEntry {
    scan:        Box<dyn FnMut(&mut World) -> DeviceScan + Send + Sync>,
    reporter_id: ReporterId,
    revision:    u64,
}

impl ReporterEntry {
    pub(crate) fn new(
        scan: Box<dyn FnMut(&mut World) -> DeviceScan + Send + Sync>,
        reporter_id: ReporterId,
    ) -> Self {
        Self {
            scan,
            reporter_id,
            revision: 0,
        }
    }

    pub(crate) fn scan(&mut self, world: &mut World) -> Option<DeviceSet> {
        let DeviceScan::Complete(devices) = (self.scan)(world) else {
            return None;
        };

        self.revision += 1;

        Some(DeviceSet {
            reporter: self.reporter_id,
            devices,
            revision: ReporterRevision::new(self.revision),
        })
    }
}

/// Erased driver value and the typed functions the kernel routes by `DriverId`.
pub(crate) struct DriverEntry {
    driver:      Box<ErasedDriver>,
    capture:     CaptureFunction,
    start_apply: StartApplyFunction,
    poll:        PollFunction,
}

impl DriverEntry {
    pub(crate) fn new<Driver>(driver: Driver) -> Self
    where
        Driver: EndpointDriver,
    {
        Self {
            driver:      Box::new(driver),
            capture:     capture_driver::<Driver>,
            start_apply: start_apply_driver::<Driver>,
            poll:        poll_driver::<Driver>,
        }
    }

    pub(crate) fn capture(
        &mut self,
        world: &mut World,
        endpoint: &DeviceEndpoint,
    ) -> Result<CaptureOutcome<CapturedConfiguration>, DriverContractError> {
        (self.capture)(self.driver.as_mut(), world, endpoint)
    }

    pub(crate) fn start_apply(
        &mut self,
        world: &mut World,
        endpoint: &DeviceEndpoint,
        configuration: &CapturedConfiguration,
        attempt: AttemptId,
        permit: ApplyPermit,
    ) -> Result<(), DriverContractError> {
        (self.start_apply)(
            self.driver.as_mut(),
            world,
            endpoint,
            configuration,
            attempt,
            permit,
        )
    }

    pub(crate) fn poll(
        &mut self,
        world: &mut World,
        attempt: AttemptId,
    ) -> Result<AttemptProgress, DriverContractError> {
        (self.poll)(self.driver.as_mut(), world, attempt)
    }
}

fn capture_driver<Driver>(
    driver: &mut ErasedDriver,
    world: &mut World,
    endpoint: &DeviceEndpoint,
) -> Result<CaptureOutcome<CapturedConfiguration>, DriverContractError>
where
    Driver: EndpointDriver,
{
    let driver = typed_driver_mut::<Driver>(driver)?;

    Ok(match driver.capture(world, endpoint) {
        CaptureOutcome::Read(configuration) => {
            CaptureOutcome::Read(CapturedConfiguration::new(configuration))
        },
        CaptureOutcome::NotReadable => CaptureOutcome::NotReadable,
        CaptureOutcome::ReadFailed(error) => CaptureOutcome::ReadFailed(error),
    })
}

fn start_apply_driver<Driver>(
    driver: &mut ErasedDriver,
    world: &mut World,
    endpoint: &DeviceEndpoint,
    configuration: &CapturedConfiguration,
    attempt: AttemptId,
    permit: ApplyPermit,
) -> Result<(), DriverContractError>
where
    Driver: EndpointDriver,
{
    let driver = typed_driver_mut::<Driver>(driver)?;
    let Some(configuration) = configuration
        .as_reflect()
        .as_any()
        .downcast_ref::<Driver::Configuration>()
    else {
        return Err(DriverContractError::ConfigurationTypeMismatch {
            expected_configuration: type_name::<Driver::Configuration>(),
            received_configuration: configuration.as_reflect().reflect_type_path().to_owned(),
        });
    };

    driver.start_apply(world, endpoint, configuration, attempt, permit);

    Ok(())
}

fn poll_driver<Driver>(
    driver: &mut ErasedDriver,
    world: &mut World,
    attempt: AttemptId,
) -> Result<AttemptProgress, DriverContractError>
where
    Driver: EndpointDriver,
{
    Ok(typed_driver_mut::<Driver>(driver)?.poll(world, attempt))
}

fn typed_driver_mut<Driver>(driver: &mut ErasedDriver) -> Result<&mut Driver, DriverContractError>
where
    Driver: EndpointDriver,
{
    driver
        .downcast_mut::<Driver>()
        .ok_or_else(|| DriverContractError::DriverTypeMismatch {
            expected_driver: type_name::<Driver>(),
        })
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::any::type_name;
    use std::error::Error;

    use bevy::app::App;
    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::ecs::reflect::ReflectComponent;
    use bevy::prelude::Component;
    use bevy::prelude::Reflect;
    use bevy::prelude::Resource;
    use bevy::prelude::World;

    use super::CaptureOutcome;
    use super::DriverContractError;
    use super::DriverEntry;
    use super::EndpointDriver;
    use crate::ApplyPermit;
    use crate::AttemptId;
    use crate::AttemptProgress;
    use crate::CapturedConfiguration;
    use crate::DeviceEndpoint;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::EndpointId;
    use crate::ReportedId;
    use crate::SchemeName;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct TestConfiguration;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct OtherConfiguration;

    #[derive(Resource)]
    struct Running;

    struct TestDriver;

    impl EndpointDriver for TestDriver {
        type Configuration = TestConfiguration;

        fn capture(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
        ) -> CaptureOutcome<Self::Configuration> {
            CaptureOutcome::NotReadable
        }

        fn start_apply(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
            _: &Self::Configuration,
            _: AttemptId,
            _: ApplyPermit,
        ) {
        }

        fn poll(&mut self, _: &mut World, _: AttemptId) -> AttemptProgress {
            AttemptProgress::Pending
        }
    }

    #[test]
    fn wrong_configuration_type_returns_contract_error_without_stopping_the_app()
    -> Result<(), Box<dyn Error>> {
        let mut app = App::new();
        let mut driver_entry = DriverEntry::new(TestDriver);
        let configuration = CapturedConfiguration::new(OtherConfiguration);
        let result = driver_entry.start_apply(
            app.world_mut(),
            &display_endpoint()?,
            &configuration,
            AttemptId::default(),
            ApplyPermit::restore_only(),
        );

        assert!(matches!(
            result,
            Err(DriverContractError::ConfigurationTypeMismatch { .. })
        ));
        app.insert_resource(Running);
        assert!(app.world().contains_resource::<Running>());

        Ok(())
    }

    #[test]
    fn wrong_driver_type_returns_contract_error_without_stopping_the_app() {
        let mut app = App::new();
        let mut driver_entry = DriverEntry::new(TestDriver);
        driver_entry.driver = Box::new(());

        let result = driver_entry.poll(app.world_mut(), AttemptId::default());

        assert!(matches!(
            result,
            Err(DriverContractError::DriverTypeMismatch { expected_driver })
                if expected_driver == type_name::<TestDriver>()
        ));
        app.insert_resource(Running);
        assert!(app.world().contains_resource::<Running>());
    }

    #[test]
    fn driver_configuration_registers_reflect_component_metadata() {
        let app = App::new();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let type_id = TypeId::of::<TestConfiguration>();

        assert!(type_registry.contains(type_id));
        assert!(
            type_registry
                .get_type_data::<ReflectComponent>(type_id)
                .is_some()
        );

        drop(type_registry);
    }

    fn display_endpoint() -> Result<DeviceEndpoint, Box<dyn Error>> {
        Ok(DeviceEndpoint {
            device: DeviceKey {
                kind: DeviceKind::Display,
                id:   DeviceIdSource::Reported {
                    scheme: SchemeName::new("edid-serial")?,
                    value:  ReportedId::new("DELL-U2723QE-9J4K2H3")?,
                },
            },
            id:     EndpointId::Whole,
        })
    }
}

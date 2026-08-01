use bevy::app::App;
use bevy::prelude::Reflect;
use bevy::prelude::Resource;
use bevy::prelude::World;

use crate::DeviceReporter;
use crate::DeviceSet;
use crate::RegisteredSchemes;
use crate::ReporterId;
use crate::SchemeName;
use crate::contract::DriverEntry;
use crate::contract::EndpointDriver;
use crate::contract::ReporterEntry;

/// Process-local driver handle that the driver registry issues in registration order.
///
/// `DriverId` has no public constructor because a binding receives it from
/// `RiggingAppExt::add_endpoint_driver`; allowing app code or a driver to mint one could route an
/// apply to a different registered implementation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
#[reflect(opaque)]
pub struct DriverId(pub(crate) u32);

/// Proof that the kernel authorized an endpoint apply and selected its permitted purpose.
///
/// The private `Purpose` cases and the private constructors prevent a driver from manufacturing
/// permission for a device whose identity did not authorize it. `#[reflect(opaque)]` extends that
/// boundary to reflection: a dynamic reflected tuple cannot construct this token.
#[derive(Clone, Copy, Debug, Reflect)]
#[reflect(opaque)]
pub struct ApplyPermit(Purpose);

#[derive(Clone, Copy, Debug, Reflect)]
enum Purpose {
    /// The kernel may authorize a device with proven or authored identity for its bound work.
    InService,
    /// The kernel may authorize a device with restore-only identity to receive prior state only.
    RestoreOnly,
}

impl ApplyPermit {
    /// Report whether this token permits use for the device's current application role.
    ///
    /// A driver whose restore operation differs from in-service work checks this before choosing
    /// its hardware command; a driver with one command path can ignore the distinction.
    #[must_use]
    pub const fn allows_in_service_use(self) -> bool { matches!(self.0, Purpose::InService) }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 9/10 kernel dispatch")
    )]
    pub(crate) const fn in_service() -> Self { Self(Purpose::InService) }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 9/10 kernel dispatch")
    )]
    pub(crate) const fn restore_only() -> Self { Self(Purpose::RestoreOnly) }
}

/// Erased reporter implementations that the collect system borrows out of `World` while scanning.
#[derive(Resource, Default)]
pub(crate) struct Reporters {
    reporters: Vec<ReporterEntry>,
    next_id:   u32,
}

impl Reporters {
    pub(crate) fn add<Reporter>(&mut self, mut reporter: Reporter) -> ReporterId
    where
        Reporter: DeviceReporter,
    {
        let reporter_id = ReporterId(self.next_id);
        self.next_id += 1;
        self.reporters.push(ReporterEntry::new(
            Box::new(move |world| reporter.scan(world)),
            reporter_id,
        ));

        reporter_id
    }

    pub(crate) fn scan(&mut self, world: &mut World) -> Vec<DeviceSet> {
        self.reporters
            .iter_mut()
            .filter_map(|reporter| reporter.scan(world))
            .collect()
    }
}

/// Erased endpoint drivers that kernel systems borrow out of `World` for capture and apply.
///
/// Kernel systems call the crate-private dispatch methods through `World::resource_scope`.
/// Endpoint driver implementations must not access `Drivers` from their own trait methods because
/// the resource is absent for that call. Every apply still requires an `ApplyPermit`, whose private
/// construction prevents application code from authorizing a device by selecting a driver route.
#[derive(Resource, Default)]
pub(crate) struct Drivers {
    drivers: Vec<DriverEntry>,
    next_id: u32,
}

impl Drivers {
    pub(crate) fn add<Driver>(&mut self, driver: Driver) -> DriverId
    where
        Driver: EndpointDriver,
    {
        let driver_id = DriverId(self.next_id);
        self.next_id += 1;
        self.drivers.push(DriverEntry::new(driver));

        driver_id
    }

    /// Ask one registered driver to capture its endpoint configuration through erased dispatch.
    ///
    /// Kernel capture systems call this inside `World::resource_scope` after checking endpoint
    /// presence and in-flight attempts. The driver contract owns those checks, so callers outside
    /// the kernel should use their own endpoint state instead.
    ///
    /// # Errors
    ///
    /// Returns `DriverContractError::DriverNotRegistered` when `driver_id` is not in this
    /// process's registry, or another `DriverContractError` when erased dispatch cannot recover
    /// the typed driver boundary.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 9/10 kernel dispatch")
    )]
    pub(crate) fn capture(
        &mut self,
        world: &mut World,
        driver_id: DriverId,
        endpoint: &crate::DeviceEndpoint,
    ) -> Result<crate::CaptureOutcome<crate::CapturedConfiguration>, crate::DriverContractError>
    {
        self.get_mut(driver_id)
            .ok_or(crate::DriverContractError::DriverNotRegistered { driver_id })?
            .capture(world, endpoint)
    }

    /// Start one authorized endpoint apply through the driver selected by `driver_id`.
    ///
    /// Kernel attempt systems call this inside `World::resource_scope`; the `ApplyPermit` must be
    /// the same token stored on that attempt, so a driver receives the exact authorization the
    /// kernel recorded.
    ///
    /// # Errors
    ///
    /// Returns `DriverContractError::DriverNotRegistered` when `driver_id` is not in this
    /// process's registry, or `DriverContractError::ConfigurationTypeMismatch` when the erased
    /// configuration belongs to a different driver's concrete type.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 9/10 kernel dispatch")
    )]
    pub(crate) fn start_apply(
        &mut self,
        world: &mut World,
        driver_id: DriverId,
        endpoint: &crate::DeviceEndpoint,
        configuration: &crate::CapturedConfiguration,
        attempt: crate::AttemptId,
        permit: ApplyPermit,
    ) -> Result<(), crate::DriverContractError> {
        self.get_mut(driver_id)
            .ok_or(crate::DriverContractError::DriverNotRegistered { driver_id })?
            .start_apply(world, endpoint, configuration, attempt, permit)
    }

    /// Poll one in-flight apply through the driver selected by `driver_id`.
    ///
    /// Kernel attempt systems call this inside `World::resource_scope` after checking that the
    /// attempt still addresses the same device and rigging revision.
    ///
    /// # Errors
    ///
    /// Returns `DriverContractError::DriverNotRegistered` when `driver_id` is not in this
    /// process's registry, or another `DriverContractError` when erased dispatch cannot recover
    /// the typed driver boundary.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 9/10 kernel dispatch")
    )]
    pub(crate) fn poll(
        &mut self,
        world: &mut World,
        driver_id: DriverId,
        attempt: crate::AttemptId,
    ) -> Result<crate::AttemptProgress, crate::DriverContractError> {
        self.get_mut(driver_id)
            .ok_or(crate::DriverContractError::DriverNotRegistered { driver_id })?
            .poll(world, attempt)
    }

    fn get_mut(&mut self, driver_id: DriverId) -> Option<&mut DriverEntry> {
        self.drivers.get_mut(usize::try_from(driver_id.0).ok()?)
    }
}

/// Adds Hana Rigging reporters, endpoint drivers, and identity schemes to a Bevy `App`.
///
/// Bevy's `App` cannot receive inherent methods from this crate, so integration crates import
/// `RiggingAppExt` before registering their hardware-specific reporter or driver implementation.
pub trait RiggingAppExt {
    /// Register one reporter and return the process-local id that identifies its completed sets.
    ///
    /// This initializes `Reporters` itself, so adding a reporter before `RiggingPlugin` does not
    /// make plugin insertion order control whether the reporter can register.
    fn add_device_reporter<Reporter>(&mut self, reporter: Reporter) -> ReporterId
    where
        Reporter: DeviceReporter;

    /// Register one endpoint driver and return the process-local id that bindings use for routing.
    ///
    /// This initializes `Drivers` itself, so adding a driver before `RiggingPlugin` does not make
    /// plugin insertion order control whether the driver can register.
    fn add_endpoint_driver<Driver>(&mut self, driver: Driver) -> DriverId
    where
        Driver: EndpointDriver;

    /// Record a reportable device-identity scheme and return this app for further setup.
    ///
    /// The method initializes `RegisteredSchemes` before recording `name`, so an integration
    /// plugin can register its scheme before or after `RiggingPlugin`. Repeated names stay valid:
    /// two reporters using one scheme assert that their reported values are comparable.
    fn register_device_scheme(&mut self, name: SchemeName) -> &mut Self;
}

impl RiggingAppExt for App {
    fn add_device_reporter<Reporter>(&mut self, reporter: Reporter) -> ReporterId
    where
        Reporter: DeviceReporter,
    {
        self.init_resource::<Reporters>();
        self.world_mut().resource_mut::<Reporters>().add(reporter)
    }

    fn add_endpoint_driver<Driver>(&mut self, driver: Driver) -> DriverId
    where
        Driver: EndpointDriver,
    {
        self.init_resource::<Drivers>();
        self.world_mut().resource_mut::<Drivers>().add(driver)
    }

    fn register_device_scheme(&mut self, name: SchemeName) -> &mut Self {
        self.init_resource::<RegisteredSchemes>();
        self.world_mut()
            .resource_mut::<RegisteredSchemes>()
            .register(name);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use bevy::app::App;
    use bevy::app::Plugin;
    use bevy::ecs::reflect::ReflectComponent;
    use bevy::prelude::Component;
    use bevy::prelude::Reflect;
    use bevy::prelude::World;
    use bevy::reflect::FromReflect;
    use bevy::reflect::tuple_struct::DynamicTupleStruct;

    use super::ApplyPermit;
    use super::DriverId;
    use super::Drivers;
    use super::Purpose;
    use super::ReporterId;
    use super::Reporters;
    use super::RiggingAppExt;
    use crate::AttachmentPath;
    use crate::AttemptId;
    use crate::AttemptProgress;
    use crate::Capabilities;
    use crate::CaptureOutcome;
    use crate::CapturedConfiguration;
    use crate::Claim;
    use crate::DeviceAccessError;
    use crate::DeviceDescriptor;
    use crate::DeviceEndpoint;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::DeviceRecord;
    use crate::DeviceReporter;
    use crate::DeviceScan;
    use crate::EndpointDriver;
    use crate::EndpointId;
    use crate::OsDeviceId;
    use crate::Presence;
    use crate::RegisteredSchemes;
    use crate::ReportedAs;
    use crate::ReportedId;
    use crate::ReportedSerial;
    use crate::RiggingPlugin;
    use crate::SchemeName;

    struct CountingReporter {
        scans: Arc<AtomicUsize>,
    }

    impl DeviceReporter for CountingReporter {
        fn scan(&mut self, _: &mut World) -> DeviceScan {
            self.scans.fetch_add(1, Ordering::Relaxed);
            DeviceScan::Complete(Vec::new())
        }
    }

    struct RecordReporter {
        scans: Arc<AtomicUsize>,
    }

    impl DeviceReporter for RecordReporter {
        fn scan(&mut self, _: &mut World) -> DeviceScan {
            self.scans.fetch_add(1, Ordering::Relaxed);
            DeviceScan::Complete(vec![DeviceRecord {
                reported_as:  ReportedAs::MatchEvidenceOnly,
                transport:    None,
                presence:     Presence::Present,
                claim:        Claim::NotApplicable,
                capabilities: Capabilities::new(),
                serial:       ReportedSerial::NotExposedByUnit,
                os_id:        OsDeviceId::PlatformReportedNothing,
                attachment:   AttachmentPath::PlatformHasNoConcept,
                descriptor:   DeviceDescriptor::PlatformReportedNothing,
            }])
        }
    }

    struct UnchangedReporter {
        scans: Arc<AtomicUsize>,
    }

    impl DeviceReporter for UnchangedReporter {
        fn scan(&mut self, _: &mut World) -> DeviceScan {
            self.scans.fetch_add(1, Ordering::Relaxed);
            DeviceScan::Unchanged
        }
    }

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct TestConfiguration;

    struct TestDriver;

    impl EndpointDriver for TestDriver {
        type Configuration = TestConfiguration;

        fn capture(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
        ) -> CaptureOutcome<Self::Configuration> {
            CaptureOutcome::ReadFailed(DeviceAccessError::Absent {
                detail: String::from("test driver has no endpoint"),
            })
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

    struct FirstSchemePlugin(SchemeName);

    impl Plugin for FirstSchemePlugin {
        fn build(&self, app: &mut App) { app.register_device_scheme(self.0.clone()); }
    }

    struct SecondSchemePlugin(SchemeName);

    impl Plugin for SecondSchemePlugin {
        fn build(&self, app: &mut App) { app.register_device_scheme(self.0.clone()); }
    }

    #[test]
    fn registered_reporter_scans_once_per_app_update() {
        let scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let _: ReporterId = app.add_device_reporter(CountingReporter {
            scans: Arc::clone(&scans),
        });

        app.update();
        assert_eq!(scans.load(Ordering::Relaxed), 1);

        app.update();
        assert_eq!(scans.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn completed_and_unchanged_reporters_both_scan_in_one_update() {
        let complete_scans = Arc::new(AtomicUsize::new(0));
        let unchanged_scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        app.add_device_reporter(RecordReporter {
            scans: Arc::clone(&complete_scans),
        });
        app.add_device_reporter(UnchangedReporter {
            scans: Arc::clone(&unchanged_scans),
        });

        app.update();

        assert_eq!(complete_scans.load(Ordering::Relaxed), 1);
        assert_eq!(unchanged_scans.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn registration_returns_distinct_reporter_and_driver_ids() {
        let scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();

        let first_reporter = app.add_device_reporter(CountingReporter {
            scans: Arc::clone(&scans),
        });
        let second_reporter = app.add_device_reporter(CountingReporter { scans });
        let first_driver = app.add_endpoint_driver(TestDriver);
        let second_driver = app.add_endpoint_driver(TestDriver);

        assert_ne!(first_reporter, second_reporter);
        assert_ne!(first_driver, second_driver);
    }

    #[test]
    fn completed_scans_use_returned_reporter_id_and_advance_revision() {
        let scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        let reporter_id = app.add_device_reporter(CountingReporter { scans });

        let first = app
            .world_mut()
            .resource_scope::<Reporters, _>(|world, mut reporters| reporters.scan(world));
        let second = app
            .world_mut()
            .resource_scope::<Reporters, _>(|world, mut reporters| reporters.scan(world));

        assert_eq!(first.len(), 1);
        assert_eq!(first[0].reporter, reporter_id);
        assert_eq!(first[0].revision.get(), 1);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].reporter, reporter_id);
        assert_eq!(second[0].revision.get(), 2);
    }

    #[test]
    fn driver_registry_routes_each_erased_dispatch() -> Result<(), Box<dyn Error>> {
        let mut app = App::new();
        let driver_id = app.add_endpoint_driver(TestDriver);
        let endpoint = display_endpoint()?;
        let configuration = CapturedConfiguration::new(TestConfiguration);

        let (capture, start, poll) =
            app.world_mut()
                .resource_scope::<Drivers, _>(|world, mut drivers| {
                    let capture = drivers.capture(world, driver_id, &endpoint);
                    let start = drivers.start_apply(
                        world,
                        driver_id,
                        &endpoint,
                        &configuration,
                        AttemptId::default(),
                        ApplyPermit::restore_only(),
                    );
                    let poll = drivers.poll(world, driver_id, AttemptId::default());

                    (capture, start, poll)
                });

        assert!(matches!(
            capture,
            Ok(CaptureOutcome::ReadFailed(DeviceAccessError::Absent { .. }))
        ));
        assert_eq!(start, Ok(()));
        assert!(matches!(poll, Ok(AttemptProgress::Pending)));

        Ok(())
    }

    #[test]
    fn reflection_cannot_construct_apply_permit() {
        let mut dynamic_permit = DynamicTupleStruct::default();
        dynamic_permit.insert(Purpose::InService);

        assert!(ApplyPermit::from_reflect(&dynamic_permit).is_none());
    }

    #[test]
    fn reflection_cannot_construct_driver_id() {
        let mut dynamic_driver_id = DynamicTupleStruct::default();
        dynamic_driver_id.insert(0_u32);

        assert!(DriverId::from_reflect(&dynamic_driver_id).is_none());
    }

    #[test]
    fn apply_permit_reports_its_kernel_minted_purpose() {
        assert!(ApplyPermit::in_service().allows_in_service_use());
        assert!(!ApplyPermit::restore_only().allows_in_service_use());
    }

    #[test]
    fn scheme_registration_works_before_and_after_rigging_plugin() -> Result<(), Box<dyn Error>> {
        let scheme = SchemeName::new("edid-serial")?;
        let mut before = App::new();
        before.add_plugins((FirstSchemePlugin(scheme.clone()), RiggingPlugin));
        assert!(
            before
                .world()
                .resource::<RegisteredSchemes>()
                .contains(&scheme)
        );

        let mut after = App::new();
        after.add_plugins((RiggingPlugin, FirstSchemePlugin(scheme.clone())));
        assert!(
            after
                .world()
                .resource::<RegisteredSchemes>()
                .contains(&scheme)
        );

        Ok(())
    }

    #[test]
    fn duplicate_scheme_plugins_keep_one_registered_name() -> Result<(), Box<dyn Error>> {
        let scheme = SchemeName::new("edid-serial")?;
        let device_key = DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Reported {
                scheme: scheme.clone(),
                value:  ReportedId::new("DELL-U2723QE-9J4K2H3")?,
            },
        };
        let mut first_then_second = App::new();
        first_then_second.add_plugins((
            RiggingPlugin,
            FirstSchemePlugin(scheme.clone()),
            SecondSchemePlugin(scheme.clone()),
        ));

        let first_registered_schemes = first_then_second.world().resource::<RegisteredSchemes>();
        assert!(first_registered_schemes.contains(&scheme));
        assert_eq!(first_registered_schemes.count(), 1);
        first_registered_schemes.validate(&device_key)?;

        let mut second_then_first = App::new();
        second_then_first.add_plugins((
            RiggingPlugin,
            SecondSchemePlugin(scheme.clone()),
            FirstSchemePlugin(scheme.clone()),
        ));

        let second_registered_schemes = second_then_first.world().resource::<RegisteredSchemes>();
        assert!(second_registered_schemes.contains(&scheme));
        assert_eq!(second_registered_schemes.count(), 1);
        second_registered_schemes.validate(&device_key)?;

        Ok(())
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
